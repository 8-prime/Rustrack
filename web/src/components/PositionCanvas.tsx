import { useEffect, useRef, useState } from "react";
import init, { MapRenderer } from "../pkg/renderer";
import { getMap, systemWsUrl, type MapView } from "../lib/api";
import {
  agvSize,
  drawGrid,
  drawScaleBar,
  project,
  transformFromCamera,
} from "../lib/mapView";
import { AGV_MAX_PX, CANVAS, MAP_STATION_HALF } from "../lib/theme";
import { StatusBadge, ViewToggle, type Status, type View } from "./viewControls";

interface Props {
  systemId: string | null;
  view: View;
  onViewChange: (view: View) => void;
}

// Flatten a layout's polyline edges into segment endpoints
// `[ax, ay, bx, by, ...]` for the GPU's instanced line pipeline.
function buildSegments(map: MapView): Float32Array {
  const seg: number[] = [];
  for (const edge of map.edges) {
    for (let i = 0; i + 1 < edge.points.length; i++) {
      const [ax, ay] = edge.points[i];
      const [bx, by] = edge.points[i + 1];
      seg.push(ax, ay, bx, by);
    }
  }
  return new Float32Array(seg);
}

function buildNodePoints(map: MapView): Float32Array {
  const pts: number[] = [];
  for (const node of map.nodes) pts.push(node.x, node.y);
  return new Float32Array(pts);
}

// Decode /agv.png to raw RGBA and hand it to the renderer as a GPU texture.
function loadSprite(renderer: MapRenderer) {
  const img = new Image();
  img.src = "/agv.png";
  img.onload = () => {
    const canvas = document.createElement("canvas");
    canvas.width = img.naturalWidth;
    canvas.height = img.naturalHeight;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.drawImage(img, 0, 0);
    const { data, width, height } = ctx.getImageData(0, 0, canvas.width, canvas.height);
    renderer.set_sprite(new Uint8Array(data.buffer), width, height);
  };
}

export function PositionCanvas({ systemId, view, onViewChange }: Props) {
  const [status, setStatus] = useState<Status>("idle");
  const [robotCount, setRobotCount] = useState(0);
  const [paused, setPaused] = useState(false);
  const [hasMap, setHasMap] = useState(false);
  const [adjusted, setAdjusted] = useState(false);
  // Gates the map/ws/event effects until the async renderer is up.
  const [ready, setReady] = useState(false);

  const containerRef = useRef<HTMLDivElement>(null);
  const glCanvasRef = useRef<HTMLCanvasElement>(null);
  const overlayCanvasRef = useRef<HTMLCanvasElement>(null);
  const rendererRef = useRef<MapRenderer | null>(null);
  // The layout is kept on the JS side too, so the overlay can draw station
  // markers and labels (the GPU layer only draws edges/nodes/sprites).
  const mapRef = useRef<MapView | null>(null);

  // --- create the renderer once, size it, load the sprite ---
  useEffect(() => {
    let cancelled = false;
    const applyResize = () => {
      const container = containerRef.current;
      const gl = glCanvasRef.current;
      const overlay = overlayCanvasRef.current;
      const renderer = rendererRef.current;
      if (!container || !gl || !overlay) return;
      const dpr = window.devicePixelRatio || 1;
      const rect = container.getBoundingClientRect();
      const w = Math.max(1, Math.round(rect.width * dpr));
      const h = Math.max(1, Math.round(rect.height * dpr));
      for (const c of [gl, overlay]) {
        c.width = w;
        c.height = h;
        c.style.width = `${rect.width}px`;
        c.style.height = `${rect.height}px`;
      }
      renderer?.resize(w, h, dpr);
    };

    (async () => {
      try {
        await init();
        if (cancelled || !glCanvasRef.current) return;
        const renderer = await MapRenderer.create(glCanvasRef.current);
        if (cancelled) {
          renderer.destroy();
          return;
        }
        rendererRef.current = renderer;
        applyResize();
        loadSprite(renderer);
        setReady(true);
      } catch (err) {
        // Most likely WebGPU is unavailable (older Firefox/Safari, no adapter).
        console.error("map renderer unavailable:", err);
        setStatus("closed");
      }
    })();

    const ro = new ResizeObserver(applyResize);
    if (containerRef.current) ro.observe(containerRef.current);

    return () => {
      cancelled = true;
      ro.disconnect();
      rendererRef.current?.destroy();
      rendererRef.current = null;
      setReady(false);
    };
  }, []);

  // --- load the layout and push it to the GPU ---
  useEffect(() => {
    mapRef.current = null;
    setHasMap(false);
    const renderer = rendererRef.current;
    if (!systemId || !ready || !renderer) return;

    let cancelled = false;
    getMap(systemId)
      .then((loaded) => {
        if (cancelled) return;
        mapRef.current = loaded;
        setHasMap(true);
        if (loaded.bounds) {
          renderer.set_map(
            buildSegments(loaded),
            buildNodePoints(loaded),
            loaded.bounds.minX,
            loaded.bounds.maxX,
            loaded.bounds.minY,
            loaded.bounds.maxY,
          );
        }
      })
      .catch(() => {
        if (!cancelled) setHasMap(false);
      });

    return () => {
      cancelled = true;
    };
  }, [systemId, ready]);

  // --- live pose stream: forward raw frames straight into the renderer ---
  useEffect(() => {
    const renderer = rendererRef.current;
    if (!ready || !renderer) return;

    renderer.reset_view();
    setAdjusted(false);
    setRobotCount(0);

    if (!systemId) {
      setStatus("idle");
      return;
    }

    setStatus("connecting");
    const ws = new WebSocket(systemWsUrl(systemId));
    ws.binaryType = "arraybuffer";
    ws.onopen = () => setStatus("connected");
    ws.onclose = () => setStatus("closed");
    ws.onerror = () => setStatus("closed");
    ws.onmessage = (ev) => {
      if (!(ev.data instanceof ArrayBuffer)) return;
      rendererRef.current?.push_frame(new Uint8Array(ev.data), performance.now());
    };

    return () => ws.close();
  }, [systemId, ready]);

  // --- pan / zoom: forward pointer + wheel to the renderer's camera ---
  useEffect(() => {
    const canvas = overlayCanvasRef.current;
    if (!canvas || !ready) return;

    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const rect = canvas.getBoundingClientRect();
      rendererRef.current?.wheel(e.clientX - rect.left, e.clientY - rect.top, e.deltaY);
    };

    let dragging = false;
    let lastX = 0;
    let lastY = 0;
    const onPointerDown = (e: PointerEvent) => {
      if (e.button !== 0) return;
      rendererRef.current?.pointer_down();
      dragging = true;
      lastX = e.clientX;
      lastY = e.clientY;
      canvas.setPointerCapture(e.pointerId);
      canvas.style.cursor = "grabbing";
    };
    const onPointerMove = (e: PointerEvent) => {
      if (!dragging) return;
      rendererRef.current?.pan(e.clientX - lastX, e.clientY - lastY);
      lastX = e.clientX;
      lastY = e.clientY;
    };
    const endDrag = (e: PointerEvent) => {
      if (!dragging) return;
      dragging = false;
      if (canvas.hasPointerCapture(e.pointerId)) canvas.releasePointerCapture(e.pointerId);
      canvas.style.cursor = "grab";
    };

    canvas.style.cursor = "grab";
    canvas.addEventListener("wheel", onWheel, { passive: false });
    canvas.addEventListener("pointerdown", onPointerDown);
    canvas.addEventListener("pointermove", onPointerMove);
    canvas.addEventListener("pointerup", endDrag);
    canvas.addEventListener("pointercancel", endDrag);
    return () => {
      canvas.removeEventListener("wheel", onWheel);
      canvas.removeEventListener("pointerdown", onPointerDown);
      canvas.removeEventListener("pointermove", onPointerMove);
      canvas.removeEventListener("pointerup", endDrag);
      canvas.removeEventListener("pointercancel", endDrag);
    };
  }, [ready]);

  // --- overlay loop: read the camera back and draw grid / text / HUD ---
  useEffect(() => {
    const canvas = overlayCanvasRef.current;
    if (!canvas || !ready) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    let raf = 0;
    let lastCount = -1;
    let lastAdjusted: boolean | null = null;

    const draw = () => {
      const renderer = rendererRef.current;
      raf = requestAnimationFrame(draw);
      if (!renderer) return;

      // Sync the cheap React state that drives the header/controls.
      const count = renderer.robot_count();
      if (count !== lastCount) {
        lastCount = count;
        setRobotCount(count);
      }
      const adj = renderer.adjusted();
      if (adj !== lastAdjusted) {
        lastAdjusted = adj;
        setAdjusted(adj);
      }

      const dpr = window.devicePixelRatio || 1;
      const w = canvas.width / dpr;
      const h = canvas.height / dpr;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, w, h);

      const t = transformFromCamera(renderer.camera());
      if (!t) return;

      drawGrid(ctx, t);

      // --- station markers + labels ---
      const map = mapRef.current;
      if (map) {
        ctx.font = "10px ui-monospace, monospace";
        ctx.textBaseline = "middle";
        for (const s of map.stations) {
          const [sx, sy] = project(t, s.x, s.y);
          ctx.fillStyle = CANVAS.station;
          ctx.fillRect(
            sx - MAP_STATION_HALF,
            sy - MAP_STATION_HALF,
            MAP_STATION_HALF * 2,
            MAP_STATION_HALF * 2,
          );
          ctx.fillStyle = CANVAS.stationLabel;
          ctx.fillText(s.name ?? s.id, sx + MAP_STATION_HALF + 3, sy);
        }
      }

      // --- robot serial labels (sprites themselves are drawn on the GPU) ---
      const robots = renderer.robots();
      const serials = renderer.robot_serials();
      if (robots.length > 0) {
        const labelGap = Math.min(agvSize(t) / 2, AGV_MAX_PX / 2) + 4;
        ctx.fillStyle = CANVAS.robotLabel;
        ctx.font = "11px ui-monospace, monospace";
        ctx.textAlign = "center";
        ctx.textBaseline = "top";
        for (let i = 0; i < serials.length; i++) {
          const [sx, sy] = project(t, robots[i * 2], robots[i * 2 + 1]);
          ctx.fillText(serials[i], sx, sy + labelGap);
        }
        ctx.textAlign = "left";
        ctx.textBaseline = "middle";
      }

      // --- HUD ---
      drawScaleBar(ctx, t);
      ctx.fillStyle = CANVAS.hud;
      ctx.font = "10px ui-monospace, monospace";
      ctx.textBaseline = "bottom";
      ctx.fillText(`FPS ${Math.round(renderer.fps())}`, 16, h - 14);
      ctx.textBaseline = "middle";
    };

    raf = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(raf);
  }, [ready]);

  const togglePaused = () => {
    setPaused((p) => {
      const next = !p;
      rendererRef.current?.set_paused(next);
      return next;
    });
  };

  const resetView = () => {
    rendererRef.current?.reset_view();
    setAdjusted(false);
  };

  const showOverlay = !systemId || (robotCount === 0 && !hasMap);
  const showWaitingNote = !!systemId && robotCount === 0 && hasMap;

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-3 border-b border-white/10 bg-[#1f2124] px-4 py-2 text-sm text-gray-200">
        <span className="font-semibold">Published positions</span>
        <StatusBadge status={status} />
        <span className="text-gray-400">{robotCount} robot(s)</span>
        {hasMap && mapRef.current && (
          <span className="text-gray-500">
            🗺 {mapRef.current.layoutName ?? mapRef.current.layoutId}
          </span>
        )}
        <div className="ml-auto flex items-center gap-2">
          <ViewToggle view={view} onChange={onViewChange} />
          {adjusted && (
            <button
              type="button"
              onClick={resetView}
              className="rounded border border-white/15 px-2 py-1 text-xs text-gray-300 hover:bg-white/10"
            >
              Fit
            </button>
          )}
          <button
            type="button"
            disabled={!systemId}
            onClick={togglePaused}
            className="rounded border border-white/15 px-2 py-1 text-xs text-gray-300 hover:bg-white/10 disabled:opacity-40"
          >
            {paused ? "Resume" : "Pause"}
          </button>
        </div>
      </div>

      <div ref={containerRef} className="relative min-h-0 flex-1 bg-[#1b1d21]">
        <canvas ref={glCanvasRef} className="absolute inset-0 block h-full w-full" />
        <canvas
          ref={overlayCanvasRef}
          className="absolute inset-0 block h-full w-full touch-none"
        />
        {showOverlay && (
          <div className="pointer-events-none absolute inset-0 flex items-center justify-center text-sm text-gray-400">
            {!systemId
              ? "Select a system to view its positions."
              : "Waiting for frames… (start the system if it is stopped)"}
          </div>
        )}
        {showWaitingNote && (
          <div className="pointer-events-none absolute inset-x-0 bottom-0 p-2 text-center text-xs text-gray-500">
            Waiting for frames… (start the system if it is stopped)
          </div>
        )}
      </div>
    </div>
  );
}

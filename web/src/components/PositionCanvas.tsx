import { useEffect, useRef, useState } from "react";
import { getMap, systemWsUrl, type MapView } from "../lib/api";
import { decodeFrame, type AgvRecord } from "../lib/protocol";
import {
  AGV_FOOTPRINT_M,
  AGV_MAX_PX,
  AGV_MIN_PX,
  CANVAS,
  MAP_NODE_RADIUS,
  MAP_STATION_HALF,
  MAX_ZOOM,
  MIN_ZOOM,
  PADDING_FRAC,
} from "../lib/theme";
import { StatusBadge, ViewToggle, type Status, type View } from "./viewControls";

interface Props {
  systemId: string | null;
  view: View;
  onViewChange: (view: View) => void;
}

interface Bounds {
  minX: number;
  maxX: number;
  minY: number;
  maxY: number;
}

// One decoded frame plus the client-clock time it arrived, so we can interpolate
// between consecutive frames instead of snapping to the newest one.
interface Frame {
  records: AgvRecord[];
  byId: Map<string, AgvRecord>;
  time: number;
}

// The world -> screen transform. `scale`/`cx`/`cy` come from the auto-fit;
// `zoom` and the pan offsets are layered on top by the user's view controls.
interface Transform {
  scale: number;
  cx: number;
  cy: number;
  w: number;
  h: number;
  panX: number;
  panY: number;
}

// The user's adjustments to the auto-fit view. `frozen` holds the bounds the
// view was fitted to at the moment the user first interacted: live bounds only
// ever grow, so leaving them connected would slide the layout out from under a
// pan. Non-null also means "the user has taken over".
interface ViewState {
  zoom: number;
  panX: number;
  panY: number;
  frozen: Bounds | null;
}

const IDENTITY_VIEW: ViewState = { zoom: 1, panX: 0, panY: 0, frozen: null };

// The layout drawn into screen-space paths. Stroking a few thousand polylines
// every frame is wasted work when neither the map nor the transform has moved,
// so the paths are rebuilt only when one of them does.
interface MapGeometry {
  map: MapView;
  transform: Transform;
  edges: Path2D;
  nodes: Path2D;
}

// Bounds on the self-tuned render interval (ms): guards a divide-by-~zero when
// two frames arrive together and caps how far playback stretches after a stall.
const MIN_INTERVAL_MS = 1;
const MAX_INTERVAL_MS = 2000;

// Roughly how far apart grid lines and the scale bar aim to be, in pixels,
// before being snapped to a round number of metres.
const GRID_TARGET_PX = 80;
const SCALE_BAR_TARGET_PX = 120;
// Past this many lines the grid is noise, so it is dropped rather than drawn.
const MAX_GRID_LINES = 800;

// The vehicle sprite, shared by every canvas instance and every robot. Drawing
// starts as soon as it decodes; until then robots fall back to a plain dot.
const agvSprite = new Image();
agvSprite.src = "/agv.png";
function spriteReady(): boolean {
  return agvSprite.complete && agvSprite.naturalWidth > 0;
}

// Interpolate an angle along its shortest arc, handling the +pi/-pi seam.
function angleLerp(a: number, b: number, f: number): number {
  const twoPi = Math.PI * 2;
  let d = (b - a) % twoPi;
  if (d > Math.PI) d -= twoPi;
  else if (d < -Math.PI) d += twoPi;
  return a + d * f;
}

// Grow `into` to cover `add`, without mutating either.
function unionBounds(into: Bounds | null, add: Bounds): Bounds {
  if (!into) return { ...add };
  return {
    minX: Math.min(into.minX, add.minX),
    maxX: Math.max(into.maxX, add.maxX),
    minY: Math.min(into.minY, add.minY),
    maxY: Math.max(into.maxY, add.maxY),
  };
}

// Fit world bounds into the canvas, preserving aspect ratio, with padding.
function fit(b: Bounds, w: number, h: number, view: ViewState): Transform {
  const spanX = Math.max(b.maxX - b.minX, 1e-3);
  const spanY = Math.max(b.maxY - b.minY, 1e-3);
  const availW = w * (1 - 2 * PADDING_FRAC);
  const availH = h * (1 - 2 * PADDING_FRAC);
  return {
    scale: Math.min(availW / spanX, availH / spanY) * view.zoom,
    cx: (b.minX + b.maxX) / 2,
    cy: (b.minY + b.maxY) / 2,
    w,
    h,
    panX: view.panX,
    panY: view.panY,
  };
}

// World -> screen (flip Y so world y-up maps to canvas y-down).
function project(t: Transform, x: number, y: number): [number, number] {
  return [
    t.w / 2 + (x - t.cx) * t.scale + t.panX,
    t.h / 2 - (y - t.cy) * t.scale + t.panY,
  ];
}

// Screen -> world, the exact inverse of `project`.
function unproject(t: Transform, sx: number, sy: number): [number, number] {
  return [
    t.cx + (sx - t.w / 2 - t.panX) / t.scale,
    t.cy - (sy - t.h / 2 - t.panY) / t.scale,
  ];
}

function sameTransform(a: Transform, b: Transform): boolean {
  return (
    a.scale === b.scale &&
    a.cx === b.cx &&
    a.cy === b.cy &&
    a.w === b.w &&
    a.h === b.h &&
    a.panX === b.panX &&
    a.panY === b.panY
  );
}

// Snap a raw distance up or down to the nearest 1/2/5 x 10^n, so grid spacing
// and scale bars land on numbers a human reads without effort.
function niceStep(raw: number): number {
  if (!(raw > 0) || !Number.isFinite(raw)) return 1;
  const magnitude = Math.pow(10, Math.floor(Math.log10(raw)));
  const f = raw / magnitude;
  const mult = f < 1.5 ? 1 : f < 3.5 ? 2 : f < 7.5 ? 5 : 10;
  return mult * magnitude;
}

// Label a distance in metres, dropping to centimetres when zoomed in far enough
// that the round number would otherwise be all decimals.
function formatDistance(metres: number): string {
  if (metres >= 1) return `${+metres.toFixed(3)} m`;
  return `${+(metres * 100).toFixed(1)} cm`;
}

// Flatten the layout into two screen-space paths, so drawing it costs one
// stroke and one fill per frame regardless of how large the layout is.
function buildGeometry(map: MapView, transform: Transform): MapGeometry {
  const edges = new Path2D();
  for (const edge of map.edges) {
    let first = true;
    for (const [x, y] of edge.points) {
      const [sx, sy] = project(transform, x, y);
      if (first) {
        edges.moveTo(sx, sy);
        first = false;
      } else {
        edges.lineTo(sx, sy);
      }
    }
  }

  const nodes = new Path2D();
  for (const node of map.nodes) {
    const [sx, sy] = project(transform, node.x, node.y);
    nodes.moveTo(sx + MAP_NODE_RADIUS, sy);
    nodes.arc(sx, sy, MAP_NODE_RADIUS, 0, Math.PI * 2);
  }

  return { map, transform, edges, nodes };
}

// A metric grid, aligned to whole multiples of `step` in world space so the
// lines stay put as the view moves. Every fifth line is emphasised.
function drawGrid(ctx: CanvasRenderingContext2D, t: Transform) {
  const step = niceStep((1 / t.scale) * GRID_TARGET_PX);
  const [left, top] = unproject(t, 0, 0);
  const [right, bottom] = unproject(t, t.w, t.h);

  const i0 = Math.ceil(left / step);
  const i1 = Math.floor(right / step);
  const j0 = Math.ceil(bottom / step);
  const j1 = Math.floor(top / step);
  if (i1 - i0 + (j1 - j0) > MAX_GRID_LINES) return;

  const minor = new Path2D();
  const major = new Path2D();
  for (let i = i0; i <= i1; i++) {
    const [sx] = project(t, i * step, 0);
    const path = i % 5 === 0 ? major : minor;
    path.moveTo(sx, 0);
    path.lineTo(sx, t.h);
  }
  for (let j = j0; j <= j1; j++) {
    const [, sy] = project(t, 0, j * step);
    const path = j % 5 === 0 ? major : minor;
    path.moveTo(0, sy);
    path.lineTo(t.w, sy);
  }

  ctx.lineWidth = 1;
  ctx.strokeStyle = CANVAS.gridMinor;
  ctx.stroke(minor);
  ctx.strokeStyle = CANVAS.gridMajor;
  ctx.stroke(major);
}

// Bottom-right scale bar: a round distance, and how long it is on screen.
function drawScaleBar(ctx: CanvasRenderingContext2D, t: Transform) {
  const metres = niceStep((1 / t.scale) * SCALE_BAR_TARGET_PX);
  const px = metres * t.scale;
  if (!Number.isFinite(px) || px <= 0) return;

  const right = t.w - 16;
  const left = right - px;
  const y = t.h - 20;

  ctx.strokeStyle = CANVAS.hud;
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(left, y - 4);
  ctx.lineTo(left, y + 4);
  ctx.moveTo(left, y);
  ctx.lineTo(right, y);
  ctx.moveTo(right, y - 4);
  ctx.lineTo(right, y + 4);
  ctx.stroke();

  ctx.fillStyle = CANVAS.hud;
  ctx.font = "10px ui-monospace, monospace";
  ctx.textAlign = "center";
  ctx.textBaseline = "bottom";
  ctx.fillText(formatDistance(metres), (left + right) / 2, y - 6);
  ctx.textAlign = "left";
  ctx.textBaseline = "middle";
}

export function PositionCanvas({ systemId, view, onViewChange }: Props) {
  const [status, setStatus] = useState<Status>("idle");
  const [robotCount, setRobotCount] = useState(0);
  const [paused, setPaused] = useState(false);
  const [hasMap, setHasMap] = useState(false);
  // Mirrors `viewState.current.frozen !== null`, purely to show the Fit button.
  const [adjusted, setAdjusted] = useState(false);

  const curFrame = useRef<Frame | null>(null);
  const prevFrame = useRef<Frame | null>(null);
  const bounds = useRef<Bounds | null>(null);
  const map = useRef<MapView | null>(null);
  const geometry = useRef<MapGeometry | null>(null);
  const viewState = useRef<ViewState>({ ...IDENTITY_VIEW });
  // The transform the last frame was drawn with, so pointer handlers can work
  // in world space without recomputing the fit.
  const lastTransform = useRef<Transform | null>(null);
  const pausedRef = useRef(paused);
  pausedRef.current = paused;

  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);

  const resetView = () => {
    viewState.current = { ...IDENTITY_VIEW };
    setAdjusted(false);
  };

  // Load the system's layout. A 404 just means no layout has been uploaded —
  // the live view still works, it simply has no track drawn behind it.
  useEffect(() => {
    map.current = null;
    geometry.current = null;
    setHasMap(false);

    if (!systemId) return;

    let cancelled = false;
    getMap(systemId)
      .then((loaded) => {
        // The user may have switched systems while this was in flight.
        if (cancelled) return;
        map.current = loaded;
        setHasMap(true);
        // Seed the view from the layout rather than waiting for robots to
        // reveal it. Union with anything already observed so a robot that
        // arrived first is not cropped out.
        if (loaded.bounds) {
          bounds.current = unionBounds(bounds.current, loaded.bounds);
        }
      })
      .catch(() => {
        if (!cancelled) setHasMap(false);
      });

    return () => {
      cancelled = true;
    };
  }, [systemId]);

  // Connect / reconnect when the selected system changes.
  useEffect(() => {
    curFrame.current = null;
    prevFrame.current = null;
    bounds.current = null;
    viewState.current = { ...IDENTITY_VIEW };
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
      const records = decodeFrame(ev.data);
      setRobotCount(records.length);
      if (pausedRef.current) return;

      // Shift the newest frame into `cur` and the outgoing one into `prev`, each
      // tagged with its arrival time so the render loop can interpolate between
      // them. `byId` is prebuilt here so the per-RAF lookup stays cheap.
      prevFrame.current = curFrame.current;
      curFrame.current = {
        records,
        byId: new Map(records.map((r) => [r.serial, r])),
        time: performance.now(),
      };

      // Grow persistent bounds so the view never rescales away from a robot.
      for (const r of records) {
        if (!Number.isFinite(r.x) || !Number.isFinite(r.y)) continue;
        if (bounds.current === null) {
          bounds.current = { minX: r.x, maxX: r.x, minY: r.y, maxY: r.y };
        } else {
          const b = bounds.current;
          if (r.x < b.minX) b.minX = r.x;
          if (r.x > b.maxX) b.maxX = r.x;
          if (r.y < b.minY) b.minY = r.y;
          if (r.y > b.maxY) b.maxY = r.y;
        }
      }
    };

    return () => ws.close();
  }, [systemId]);

  // Keep the canvas backing store sized to its container at device resolution.
  useEffect(() => {
    const container = containerRef.current;
    const canvas = canvasRef.current;
    if (!container || !canvas) return;

    const resize = () => {
      const dpr = window.devicePixelRatio || 1;
      const rect = container.getBoundingClientRect();
      canvas.width = Math.max(1, Math.round(rect.width * dpr));
      canvas.height = Math.max(1, Math.round(rect.height * dpr));
      canvas.style.width = `${rect.width}px`;
      canvas.style.height = `${rect.height}px`;
    };

    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(container);
    return () => ro.disconnect();
  }, []);

  // Zoom and pan. Both freeze the fit on first use: from then on the view is
  // the user's, and incoming positions no longer rescale it.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    // Detach the view from the growing live bounds, keeping what is on screen.
    const takeOver = () => {
      if (viewState.current.frozen) return;
      viewState.current.frozen = bounds.current ? { ...bounds.current } : null;
      if (viewState.current.frozen) setAdjusted(true);
    };

    const onWheel = (e: WheelEvent) => {
      const t = lastTransform.current;
      if (!t) return;
      e.preventDefault();
      takeOver();

      const rect = canvas.getBoundingClientRect();
      const px = e.clientX - rect.left;
      const py = e.clientY - rect.top;
      const [wx, wy] = unproject(t, px, py);

      const v = viewState.current;
      const next = Math.min(Math.max(v.zoom * Math.exp(-e.deltaY * 0.0015), MIN_ZOOM), MAX_ZOOM);
      // `scale` is proportional to zoom, so the new scale follows from the ratio
      // without refitting; then solve for the pan that pins (wx, wy) under the
      // cursor.
      const scale = (t.scale / v.zoom) * next;
      v.zoom = next;
      v.panX = px - t.w / 2 - (wx - t.cx) * scale;
      v.panY = py - t.h / 2 + (wy - t.cy) * scale;
    };

    let dragging = false;
    let lastX = 0;
    let lastY = 0;

    const onPointerDown = (e: PointerEvent) => {
      if (e.button !== 0) return;
      takeOver();
      dragging = true;
      lastX = e.clientX;
      lastY = e.clientY;
      canvas.setPointerCapture(e.pointerId);
      canvas.style.cursor = "grabbing";
    };

    const onPointerMove = (e: PointerEvent) => {
      if (!dragging) return;
      viewState.current.panX += e.clientX - lastX;
      viewState.current.panY += e.clientY - lastY;
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
  }, []);

  // Render loop: read the latest records each frame and draw them.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    let raf = 0;
    // Smoothed frame rate, refreshed a few times a second so it stays readable.
    let frames = 0;
    let fpsSince = performance.now();
    let fps = 0;

    const draw = () => {
      const dpr = window.devicePixelRatio || 1;
      const w = canvas.width / dpr;
      const h = canvas.height / dpr;

      const now = performance.now();
      frames++;
      if (now - fpsSince >= 250) {
        fps = Math.round((frames * 1000) / (now - fpsSince));
        frames = 0;
        fpsSince = now;
      }

      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.fillStyle = CANVAS.bg;
      ctx.fillRect(0, 0, w, h);

      const cur = curFrame.current;
      // Once the user has taken over, fit against the bounds captured then, so
      // late-arriving positions cannot shift the view under them.
      const b = viewState.current.frozen ?? bounds.current;

      // Nothing to place anything against yet.
      if (!b) {
        raf = requestAnimationFrame(draw);
        return;
      }

      const transform = fit(b, w, h, viewState.current);
      lastTransform.current = transform;
      const toScreen = (x: number, y: number): [number, number] =>
        project(transform, x, y);

      drawGrid(ctx, transform);

      // --- map layer ---
      const loaded = map.current;
      if (loaded) {
        // Rebuild the screen-space paths only when the map or the view changed.
        const cached = geometry.current;
        if (!cached || cached.map !== loaded || !sameTransform(cached.transform, transform)) {
          geometry.current = buildGeometry(loaded, transform);
        }
        const geo = geometry.current!;

        ctx.strokeStyle = CANVAS.edge;
        ctx.lineWidth = 1.5;
        ctx.stroke(geo.edges);

        ctx.fillStyle = CANVAS.node;
        ctx.fill(geo.nodes);

        // Stations are few, and each carries a label, so they stay per-frame.
        ctx.font = "10px ui-monospace, monospace";
        ctx.textBaseline = "middle";
        for (const station of loaded.stations) {
          const [sx, sy] = toScreen(station.x, station.y);
          ctx.fillStyle = CANVAS.station;
          ctx.fillRect(
            sx - MAP_STATION_HALF,
            sy - MAP_STATION_HALF,
            MAP_STATION_HALF * 2,
            MAP_STATION_HALF * 2,
          );
          ctx.fillStyle = CANVAS.stationLabel;
          ctx.fillText(station.name ?? station.id, sx + MAP_STATION_HALF + 3, sy);
        }
      }

      // --- robot layer, on top ---
      if (cur && cur.records.length > 0) {
        // Render one interval behind real time and interpolate `prev -> cur`.
        // `now == cur.time` (a frame just arrived) gives f=0; a full interval
        // later gives f=1; beyond that it holds at `cur` until the next frame.
        const prev = prevFrame.current;
        let f = 1;
        if (prev) {
          const interval = Math.min(
            Math.max(cur.time - prev.time, MIN_INTERVAL_MS),
            MAX_INTERVAL_MS,
          );
          f = Math.min(Math.max((now - cur.time) / interval, 0), 1);
        }

        const size = Math.min(
          Math.max(AGV_FOOTPRINT_M * transform.scale, AGV_MIN_PX),
          AGV_MAX_PX,
        );
        const ready = spriteReady();
        ctx.imageSmoothingEnabled = true;
        ctx.imageSmoothingQuality = "high";

        for (const r of cur.records) {
          if (!Number.isFinite(r.x) || !Number.isFinite(r.y)) continue;

          // Blend from the robot's previous pose toward this one. Robots absent
          // from the previous frame (just appeared) render at their current pose.
          const p = prev?.byId.get(r.serial);
          const x = p ? p.x + (r.x - p.x) * f : r.x;
          const y = p ? p.y + (r.y - p.y) * f : r.y;
          const theta = p ? angleLerp(p.theta, r.theta, f) : r.theta;

          const [sx, sy] = toScreen(x, y);

          if (ready) {
            // The sprite's nose points up in image space, i.e. along (0, -1).
            // `project` flips Y, so heading theta points along (cos, -sin) on
            // screen, and the rotation that takes one to the other is pi/2 - theta.
            ctx.save();
            ctx.translate(sx, sy);
            ctx.rotate(Math.PI / 2 - theta);
            ctx.drawImage(agvSprite, -size / 2, -size / 2, size, size);
            ctx.restore();
          } else {
            // Still decoding: a dot keeps the robots visible in the meantime.
            ctx.fillStyle = CANVAS.robotLabel;
            ctx.beginPath();
            ctx.arc(sx, sy, 5, 0, Math.PI * 2);
            ctx.fill();
          }

          // Label below the sprite, so neighbouring vehicles do not overlap it.
          ctx.fillStyle = CANVAS.robotLabel;
          ctx.font = "11px ui-monospace, monospace";
          ctx.textAlign = "center";
          ctx.textBaseline = "top";
          ctx.fillText(r.serial, sx, sy + size / 2 + 4);
          ctx.textAlign = "left";
          ctx.textBaseline = "middle";
        }
      }

      // --- HUD ---
      drawScaleBar(ctx, transform);
      ctx.fillStyle = CANVAS.hud;
      ctx.font = "10px ui-monospace, monospace";
      ctx.textBaseline = "bottom";
      ctx.fillText(`FPS ${fps}`, 16, h - 14);
      ctx.textBaseline = "middle";

      raf = requestAnimationFrame(draw);
    };

    raf = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(raf);
  }, []);

  // With a layout drawn there is already something to look at, so the
  // "waiting" message steps aside and becomes a footnote.
  const showOverlay = !systemId || (robotCount === 0 && !hasMap);
  const showWaitingNote = !!systemId && robotCount === 0 && hasMap;

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-3 border-b border-white/10 bg-[#1f2124] px-4 py-2 text-sm text-gray-200">
        <span className="font-semibold">Published positions</span>
        <StatusBadge status={status} />
        <span className="text-gray-400">{robotCount} robot(s)</span>
        {hasMap && map.current && (
          <span className="text-gray-500">
            🗺 {map.current.layoutName ?? map.current.layoutId}
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
            onClick={() => setPaused((p) => !p)}
            className="rounded border border-white/15 px-2 py-1 text-xs text-gray-300 hover:bg-white/10 disabled:opacity-40"
          >
            {paused ? "Resume" : "Pause"}
          </button>
        </div>
      </div>

      <div ref={containerRef} className="relative min-h-0 flex-1 bg-[#1b1d21]">
        <canvas ref={canvasRef} className="block h-full w-full touch-none" />
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

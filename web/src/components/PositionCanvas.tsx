import { useEffect, useRef, useState } from "react";
import { systemWsUrl } from "../lib/api";
import { decodeFrame, type AgvRecord } from "../lib/protocol";
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

const DOT_RADIUS = 6;
const HEADING_LEN = 18;
const PADDING_FRAC = 0.1;

// Bounds on the self-tuned render interval (ms): guards a divide-by-~zero when
// two frames arrive together and caps how far playback stretches after a stall.
const MIN_INTERVAL_MS = 1;
const MAX_INTERVAL_MS = 2000;

// Interpolate an angle along its shortest arc, handling the +pi/-pi seam.
function angleLerp(a: number, b: number, f: number): number {
  const twoPi = Math.PI * 2;
  let d = (b - a) % twoPi;
  if (d > Math.PI) d -= twoPi;
  else if (d < -Math.PI) d += twoPi;
  return a + d * f;
}

// Deterministic HSL color from a serial, so each robot keeps a stable hue.
function colorFor(serial: string): string {
  let hash = 0;
  for (let i = 0; i < serial.length; i++) {
    hash = (hash * 31 + serial.charCodeAt(i)) | 0;
  }
  const hue = ((hash % 360) + 360) % 360;
  return `hsl(${hue}, 70%, 50%)`;
}

export function PositionCanvas({ systemId, view, onViewChange }: Props) {
  const [status, setStatus] = useState<Status>("idle");
  const [robotCount, setRobotCount] = useState(0);
  const [paused, setPaused] = useState(false);

  const curFrame = useRef<Frame | null>(null);
  const prevFrame = useRef<Frame | null>(null);
  const bounds = useRef<Bounds | null>(null);
  const pausedRef = useRef(paused);
  pausedRef.current = paused;

  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);

  // Connect / reconnect when the selected system changes.
  useEffect(() => {
    curFrame.current = null;
    prevFrame.current = null;
    bounds.current = null;
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

  // Render loop: read the latest records each frame and draw them.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    let raf = 0;

    const draw = () => {
      const dpr = window.devicePixelRatio || 1;
      const w = canvas.width / dpr;
      const h = canvas.height / dpr;

      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, w, h);

      const cur = curFrame.current;
      const b = bounds.current;

      if (cur && cur.records.length > 0 && b) {
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
          f = Math.min(Math.max((performance.now() - cur.time) / interval, 0), 1);
        }

        // Fit world bounds into the canvas, preserving aspect ratio, with padding.
        const spanX = Math.max(b.maxX - b.minX, 1e-3);
        const spanY = Math.max(b.maxY - b.minY, 1e-3);
        const pad = PADDING_FRAC;
        const availW = w * (1 - 2 * pad);
        const availH = h * (1 - 2 * pad);
        const scale = Math.min(availW / spanX, availH / spanY);
        const cx = (b.minX + b.maxX) / 2;
        const cy = (b.minY + b.maxY) / 2;

        // World -> screen (flip Y so world y-up maps to canvas y-down).
        const toScreen = (x: number, y: number): [number, number] => [
          w / 2 + (x - cx) * scale,
          h / 2 - (y - cy) * scale,
        ];

        for (const r of cur.records) {
          if (!Number.isFinite(r.x) || !Number.isFinite(r.y)) continue;

          // Blend from the robot's previous pose toward this one. Robots absent
          // from the previous frame (just appeared) render at their current pose.
          const p = prev?.byId.get(r.serial);
          const x = p ? p.x + (r.x - p.x) * f : r.x;
          const y = p ? p.y + (r.y - p.y) * f : r.y;
          const theta = p ? angleLerp(p.theta, r.theta, f) : r.theta;

          const [sx, sy] = toScreen(x, y);
          const color = colorFor(r.serial);

          // Heading tick (dy negated because screen Y is flipped).
          ctx.strokeStyle = color;
          ctx.lineWidth = 2;
          ctx.beginPath();
          ctx.moveTo(sx, sy);
          ctx.lineTo(
            sx + Math.cos(theta) * HEADING_LEN,
            sy - Math.sin(theta) * HEADING_LEN,
          );
          ctx.stroke();

          // Dot.
          ctx.fillStyle = color;
          ctx.beginPath();
          ctx.arc(sx, sy, DOT_RADIUS, 0, Math.PI * 2);
          ctx.fill();

          // Label.
          ctx.fillStyle = "#374151";
          ctx.font = "11px ui-monospace, monospace";
          ctx.textBaseline = "middle";
          ctx.fillText(r.serial, sx + DOT_RADIUS + 4, sy);
        }
      }

      raf = requestAnimationFrame(draw);
    };

    raf = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(raf);
  }, []);

  const showOverlay = !systemId || robotCount === 0;

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-3 border-b border-gray-200 px-4 py-2 text-sm">
        <span className="font-semibold">Published positions</span>
        <StatusBadge status={status} />
        <span className="text-gray-500">{robotCount} robot(s)</span>
        <div className="ml-auto flex items-center gap-2">
          <ViewToggle view={view} onChange={onViewChange} />
          <button
            type="button"
            disabled={!systemId}
            onClick={() => setPaused((p) => !p)}
            className="rounded border border-gray-300 px-2 py-1 text-xs disabled:opacity-40"
          >
            {paused ? "Resume" : "Pause"}
          </button>
        </div>
      </div>

      <div ref={containerRef} className="relative min-h-0 flex-1 bg-gray-50">
        <canvas ref={canvasRef} className="block h-full w-full" />
        {showOverlay && (
          <div className="pointer-events-none absolute inset-0 flex items-center justify-center text-sm text-gray-500">
            {!systemId
              ? "Select a system to view its positions."
              : "Waiting for frames… (start the system if it is stopped)"}
          </div>
        )}
      </div>
    </div>
  );
}

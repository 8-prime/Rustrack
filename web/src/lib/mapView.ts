// Pure map-view helpers shared by the 2D overlay (grid, scale bar, labels).
//
// The wgpu layer owns the camera; each overlay frame reads it back as a flat
// `[scale, zoom, cx, cy, panX, panY, w, h]` array (see `MapRenderer::camera`),
// which `transformFromCamera` turns into the `Transform` these helpers consume.
// The projection here is the exact JS twin of `camera.rs`'s, so the text lines
// up with the GPU geometry pixel-for-pixel.

import {
  AGV_FOOTPRINT_M,
  AGV_MAX_PX,
  AGV_MIN_PX,
  CANVAS,
} from "./theme";

export interface Transform {
  // Composite px-per-metre (auto-fit scale already multiplied by `zoom`).
  scale: number;
  // The user's multiplier relative to the auto-fit; needed to size the sprite.
  zoom: number;
  cx: number;
  cy: number;
  w: number;
  h: number;
  panX: number;
  panY: number;
}

const GRID_TARGET_PX = 80;
const SCALE_BAR_TARGET_PX = 120;
const MAX_GRID_LINES = 800;

/** Build a `Transform` from the renderer's flat camera readout, or null if the
 *  view isn't ready yet (empty array). */
export function transformFromCamera(c: Float32Array): Transform | null {
  if (c.length < 8) return null;
  return {
    scale: c[0],
    zoom: c[1],
    cx: c[2],
    cy: c[3],
    panX: c[4],
    panY: c[5],
    w: c[6],
    h: c[7],
  };
}

/** World -> screen (CSS px), flipping Y so world y-up maps to canvas y-down. */
export function project(t: Transform, x: number, y: number): [number, number] {
  return [
    t.w / 2 + (x - t.cx) * t.scale + t.panX,
    t.h / 2 - (y - t.cy) * t.scale + t.panY,
  ];
}

/** The AGV sprite's on-screen size in px — matches `camera::agv_size`, used
 *  only to place the label below the vehicle. */
export function agvSize(t: Transform): number {
  const fitted = AGV_FOOTPRINT_M * (t.scale / t.zoom);
  return Math.min(Math.max(fitted, AGV_MIN_PX), AGV_MAX_PX) * t.zoom;
}

// Snap a raw distance to the nearest 1/2/5 x 10^n, so grid spacing and scale
// bars land on numbers a human reads without effort.
function niceStep(raw: number): number {
  if (!(raw > 0) || !Number.isFinite(raw)) return 1;
  const magnitude = Math.pow(10, Math.floor(Math.log10(raw)));
  const f = raw / magnitude;
  const mult = f < 1.5 ? 1 : f < 3.5 ? 2 : f < 7.5 ? 5 : 10;
  return mult * magnitude;
}

function formatDistance(metres: number): string {
  if (metres >= 1) return `${+metres.toFixed(3)} m`;
  return `${+(metres * 100).toFixed(1)} cm`;
}

/** A metric grid aligned to whole multiples of `step` in world space. */
export function drawGrid(ctx: CanvasRenderingContext2D, t: Transform) {
  const step = niceStep((1 / t.scale) * GRID_TARGET_PX);
  const left = t.cx + (0 - t.w / 2 - t.panX) / t.scale;
  const right = t.cx + (t.w - t.w / 2 - t.panX) / t.scale;
  const top = t.cy - (0 - t.h / 2 - t.panY) / t.scale;
  const bottom = t.cy - (t.h - t.h / 2 - t.panY) / t.scale;

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

/** Bottom-right scale bar: a round distance, and how long it is on screen. */
export function drawScaleBar(ctx: CanvasRenderingContext2D, t: Transform) {
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

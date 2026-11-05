// Canvas palette. The map is drawn on a dark surface so the light track lines
// and the AGV sprites carry the contrast; everything structural sits well below
// the vehicles in brightness so it reads as background.
export const CANVAS = {
  bg: "#1b1d21",
  gridMinor: "rgba(255, 255, 255, 0.035)",
  gridMajor: "rgba(255, 255, 255, 0.075)",
  edge: "#c3c9d1",
  node: "#5d646e",
  station: "#f0821e",
  stationLabel: "#c9a06a",
  robotLabel: "#d6dae0",
  followedLabel: "#f59e0b",
  hud: "#8b929b",
} as const;

// AGVs are drawn at a fixed physical footprint so they stay in proportion to
// the track, except that they never shrink below AGV_MIN_PX and vanish.
export const AGV_FOOTPRINT_M = 1.2;
export const AGV_MIN_PX = 14;
// How large a vehicle appears after zooming to it.
export const AGV_FOCUS_PX = 96;
// The serial label sits just below the sprite, up to this far from its centre.
export const AGV_LABEL_MAX_GAP_PX = 48;

// Map layer geometry, in screen pixels.
export const MAP_NODE_RADIUS = 2.5;
export const MAP_STATION_HALF = 3.5;

// Fraction of the canvas left empty on each side when auto-fitting.
export const PADDING_FRAC = 0.1;

// How far the user may zoom relative to the auto-fit view.
export const MIN_ZOOM = 0.2;
export const MAX_ZOOM = 40;

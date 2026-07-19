// Typed client for the backend systems API (see `backend/src/api`).

export type RuntimeState = "Running" | "Stopped";

export interface Configuration {
  id: string;
  name: string;
  mqtt_url: string;
  mqtt_port: number;
  mqtt_username: string | null;
  mqtt_password: string | null;
  tls_skip_verify: boolean;
  vda5050_topic_prefix: string;
  created_at: string;
}

/**
 * Description of a system's uploaded LIF layout.
 *
 * Only the summary travels with SystemInfo — the layout document itself can be
 * tens of megabytes and is fetched separately via `getLif`.
 */
export interface LifSummary {
  projectIdentification: string;
  lifVersion: string;
  layoutCount: number;
  nodeCount: number;
  edgeCount: number;
  stationCount: number;
  rawBytes: number;
  uploadedAt: string;
}

export interface SystemInfo {
  config: Configuration;
  state: RuntimeState;
  lif: LifSummary | null;
}

/** Axis-aligned extent of a layout, in metres. */
export interface Bounds {
  minX: number;
  minY: number;
  maxX: number;
  maxY: number;
}

export interface MapNode {
  id: string;
  x: number;
  y: number;
  /** Radians, when a vehicle type constrains the orientation here. */
  theta: number | null;
}

export interface MapEdge {
  id: string;
  from: string;
  to: string;
  /** At least two `[x, y]` points. Curved edges arrive already tessellated. */
  points: [number, number][];
}

export interface MapStation {
  id: string;
  name: string | null;
  x: number;
  y: number;
}

/**
 * A layout projected into drawable geometry (see `shared/src/lif/map.rs`).
 *
 * The renderable counterpart to the raw LIF document: the per-vehicle-type
 * indirection is collapsed, NURBS trajectories are already tessellated into
 * polylines, and the bounding box is precomputed. Nothing here needs LIF
 * knowledge to draw.
 */
export interface MapView {
  layoutId: string;
  layoutName: string | null;
  /** Every layout in the document, for a layer selector. Usually just one. */
  availableLayouts: string[];
  /** Null only when the layout has no geometry at all. */
  bounds: Bounds | null;
  nodes: MapNode[];
  edges: MapEdge[];
  stations: MapStation[];
}

/** Body for `POST /api/systems`. */
export interface CreateSystem {
  name: string;
  mqtt_url: string;
  mqtt_port: number;
  mqtt_username?: string | null;
  mqtt_password?: string | null;
  tls_skip_verify: boolean;
  vda5050_topic_prefix: string;
}

async function request<T>(input: string, init?: RequestInit): Promise<T> {
  const res = await fetch(input, init);
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(`${res.status} ${res.statusText}${text ? `: ${text}` : ""}`);
  }
  // 204 No Content (delete) has no body.
  if (res.status === 204) return undefined as T;
  const contentType = res.headers.get("content-type") ?? "";
  return contentType.includes("application/json")
    ? ((await res.json()) as T)
    : (undefined as T);
}

export function listSystems(): Promise<SystemInfo[]> {
  return request<SystemInfo[]>("/api/systems");
}

export function createSystem(body: CreateSystem): Promise<SystemInfo> {
  return request<SystemInfo>("/api/systems", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

/** Body for `PUT /api/systems/{id}` — same shape as CreateSystem. */
export type UpdateSystem = CreateSystem;

export function updateSystem(id: string, body: UpdateSystem): Promise<SystemInfo> {
  return request<SystemInfo>(`/api/systems/${encodeURIComponent(id)}`, {
    method: "PUT",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

export function deleteSystem(id: string): Promise<void> {
  return request<void>(`/api/systems/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
}

export function startSystem(id: string): Promise<void> {
  return request<void>(`/api/systems/${encodeURIComponent(id)}/start`, {
    method: "POST",
  });
}

export function stopSystem(id: string): Promise<void> {
  return request<void>(`/api/systems/${encodeURIComponent(id)}/stop`, {
    method: "POST",
  });
}

/**
 * Upload a LIF layout for a system.
 *
 * The `File` is passed straight through as the request body — a File is a Blob
 * and a valid BodyInit, so the browser streams it from disk. Reading it with
 * `file.text()` first would materialise the whole document (tens of megabytes)
 * as a JS string for no benefit.
 */
export function uploadLif(id: string, file: File): Promise<LifSummary> {
  return request<LifSummary>(`/api/systems/${encodeURIComponent(id)}/lif`, {
    method: "POST",
    // Set explicitly: the browser would otherwise infer the type from the file.
    headers: { "content-type": "application/json" },
    body: file,
  });
}

/**
 * Fetch a system's stored LIF layout.
 *
 * Served gzipped; `fetch` decompresses transparently. Note this can be a very
 * large document — prefer the `lif` summary on SystemInfo for display.
 */
export function getLif(id: string): Promise<unknown> {
  return request<unknown>(`/api/systems/${encodeURIComponent(id)}/lif`);
}

/**
 * Fetch a system's layout as drawable geometry.
 *
 * Prefer this over `getLif` for rendering: the projection is a few hundred
 * kilobytes where the source document can be tens of megabytes, and it needs
 * no NURBS evaluation client-side. Rejects with a 404 when the system has no
 * layout uploaded.
 *
 * `layout` picks one layout out of a multi-layout document; omit it for the
 * first, which is also the one the backend keeps cached.
 */
export function getMap(id: string, layout?: string): Promise<MapView> {
  const query = layout ? `?layout=${encodeURIComponent(layout)}` : "";
  return request<MapView>(`/api/systems/${encodeURIComponent(id)}/map${query}`);
}

export function deleteLif(id: string): Promise<void> {
  return request<void>(`/api/systems/${encodeURIComponent(id)}/lif`, {
    method: "DELETE",
  });
}

/** Build the WebSocket URL for a system's live pose stream. */
export function systemWsUrl(id: string): string {
  const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${proto}//${window.location.host}/api/systems/${encodeURIComponent(id)}/ws`;
}

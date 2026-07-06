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

export interface SystemInfo {
  config: Configuration;
  state: RuntimeState;
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

/** Build the WebSocket URL for a system's live pose stream. */
export function systemWsUrl(id: string): string {
  const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${proto}//${window.location.host}/api/systems/${encodeURIComponent(id)}/ws`;
}

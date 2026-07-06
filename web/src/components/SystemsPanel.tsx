import { useCallback, useEffect, useState } from "react";
import {
  type CreateSystem,
  type SystemInfo,
  createSystem,
  deleteSystem,
  listSystems,
  startSystem,
  stopSystem,
} from "../lib/api";

interface Props {
  selectedId: string | null;
  onSelect: (id: string | null) => void;
}

const EMPTY_FORM: CreateSystem = {
  name: "",
  mqtt_url: "localhost",
  mqtt_port: 1883,
  mqtt_username: "",
  mqtt_password: "",
  tls_skip_verify: false,
  vda5050_topic_prefix: "uagv/v2",
};

export function SystemsPanel({ selectedId, onSelect }: Props) {
  const [systems, setSystems] = useState<SystemInfo[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [form, setForm] = useState<CreateSystem>(EMPTY_FORM);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const next = await listSystems();
      setSystems(next);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  // Initial load + light polling so state badges reflect start/stop.
  useEffect(() => {
    void refresh();
    const t = setInterval(() => void refresh(), 3000);
    return () => clearInterval(t);
  }, [refresh]);

  const runAction = async (id: string, action: () => Promise<void>) => {
    setBusyId(id);
    try {
      await action();
      await refresh();
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusyId(null);
    }
  };

  const onDelete = async (id: string) => {
    await runAction(id, () => deleteSystem(id));
    if (selectedId === id) onSelect(null);
  };

  const onSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setCreating(true);
    try {
      // Send empty optional strings as null.
      const body: CreateSystem = {
        ...form,
        name: form.name.trim(),
        mqtt_username: form.mqtt_username?.trim() ? form.mqtt_username.trim() : null,
        mqtt_password: form.mqtt_password?.trim() ? form.mqtt_password.trim() : null,
      };
      await createSystem(body);
      setForm(EMPTY_FORM);
      await refresh();
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setCreating(false);
    }
  };

  return (
    <div className="flex h-full flex-col gap-4 overflow-y-auto p-4">
      <h1 className="text-lg font-semibold">Systems</h1>

      {error && (
        <div className="rounded bg-red-100 px-3 py-2 text-sm text-red-800">
          {error}
        </div>
      )}

      {/* System list */}
      <ul className="flex flex-col gap-2">
        {systems.length === 0 && (
          <li className="text-sm text-gray-500">No systems yet.</li>
        )}
        {systems.map(({ config, state }) => {
          const selected = config.id === selectedId;
          const running = state === "Running";
          const busy = busyId === config.id;
          return (
            <li
              key={config.id}
              onClick={() => onSelect(config.id)}
              className={`cursor-pointer rounded border p-3 ${
                selected
                  ? "border-blue-500 bg-blue-50"
                  : "border-gray-200 hover:border-gray-300"
              }`}
            >
              <div className="flex items-center justify-between gap-2">
                <span className="font-medium">{config.name || "(unnamed)"}</span>
                <span
                  className={`rounded-full px-2 py-0.5 text-xs font-semibold ${
                    running
                      ? "bg-green-100 text-green-800"
                      : "bg-gray-200 text-gray-700"
                  }`}
                >
                  {state}
                </span>
              </div>
              <div className="mt-1 text-xs text-gray-500">
                {config.mqtt_url}:{config.mqtt_port} · {config.vda5050_topic_prefix}
              </div>
              <div className="mt-2 flex gap-2" onClick={(e) => e.stopPropagation()}>
                <button
                  type="button"
                  disabled={busy || running}
                  onClick={() => runAction(config.id, () => startSystem(config.id))}
                  className="rounded bg-green-600 px-2 py-1 text-xs font-medium text-white disabled:opacity-40"
                >
                  Start
                </button>
                <button
                  type="button"
                  disabled={busy || !running}
                  onClick={() => runAction(config.id, () => stopSystem(config.id))}
                  className="rounded bg-amber-600 px-2 py-1 text-xs font-medium text-white disabled:opacity-40"
                >
                  Stop
                </button>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => onDelete(config.id)}
                  className="rounded bg-red-600 px-2 py-1 text-xs font-medium text-white disabled:opacity-40"
                >
                  Delete
                </button>
              </div>
            </li>
          );
        })}
      </ul>

      {/* Create form */}
      <form
        onSubmit={onSubmit}
        className="mt-auto flex flex-col gap-2 rounded border border-gray-200 p-3"
      >
        <h2 className="text-sm font-semibold">New system</h2>
        <input
          required
          placeholder="Name"
          value={form.name}
          onChange={(e) => setForm({ ...form, name: e.target.value })}
          className="rounded border border-gray-300 px-2 py-1 text-sm"
        />
        <div className="flex gap-2">
          <input
            required
            placeholder="MQTT host"
            value={form.mqtt_url}
            onChange={(e) => setForm({ ...form, mqtt_url: e.target.value })}
            className="min-w-0 flex-1 rounded border border-gray-300 px-2 py-1 text-sm"
          />
          <input
            required
            type="number"
            placeholder="Port"
            value={form.mqtt_port}
            onChange={(e) =>
              setForm({ ...form, mqtt_port: Number(e.target.value) })
            }
            className="w-20 rounded border border-gray-300 px-2 py-1 text-sm"
          />
        </div>
        <div className="flex gap-2">
          <input
            placeholder="Username (optional)"
            value={form.mqtt_username ?? ""}
            onChange={(e) => setForm({ ...form, mqtt_username: e.target.value })}
            className="min-w-0 flex-1 rounded border border-gray-300 px-2 py-1 text-sm"
          />
          <input
            type="password"
            placeholder="Password (optional)"
            value={form.mqtt_password ?? ""}
            onChange={(e) => setForm({ ...form, mqtt_password: e.target.value })}
            className="min-w-0 flex-1 rounded border border-gray-300 px-2 py-1 text-sm"
          />
        </div>
        <input
          required
          placeholder="VDA5050 topic prefix"
          value={form.vda5050_topic_prefix}
          onChange={(e) =>
            setForm({ ...form, vda5050_topic_prefix: e.target.value })
          }
          className="rounded border border-gray-300 px-2 py-1 text-sm"
        />
        <label className="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={form.tls_skip_verify}
            onChange={(e) =>
              setForm({ ...form, tls_skip_verify: e.target.checked })
            }
          />
          Skip TLS verification
        </label>
        <button
          type="submit"
          disabled={creating}
          className="rounded bg-blue-600 px-3 py-1.5 text-sm font-medium text-white disabled:opacity-40"
        >
          {creating ? "Creating…" : "Create system"}
        </button>
      </form>
    </div>
  );
}

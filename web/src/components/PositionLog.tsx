import { useEffect, useRef, useState } from "react";
import { systemWsUrl } from "../lib/api";
import { decodeFrame } from "../lib/protocol";

interface Props {
  systemId: string | null;
}

type Status = "idle" | "connecting" | "connected" | "closed";

const MAX_LINES = 1000;
const FLUSH_MS = 250;

function timestamp(): string {
  const d = new Date();
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  const ss = String(d.getSeconds()).padStart(2, "0");
  const ms = String(d.getMilliseconds()).padStart(3, "0");
  return `${hh}:${mm}:${ss}.${ms}`;
}

export function PositionLog({ systemId }: Props) {
  const [status, setStatus] = useState<Status>("idle");
  const [robotCount, setRobotCount] = useState(0);
  const [lines, setLines] = useState<string[]>([]);
  const [paused, setPaused] = useState(false);

  // Incoming lines buffered here and flushed to state on a timer, so a 20 Hz
  // stream doesn't trigger a React re-render per frame.
  const pending = useRef<string[]>([]);
  const pausedRef = useRef(paused);
  pausedRef.current = paused;

  const scrollRef = useRef<HTMLDivElement>(null);
  const autoScroll = useRef(true);

  // Connect / reconnect when the selected system changes.
  useEffect(() => {
    pending.current = [];
    setLines([]);
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
      const ts = timestamp();
      for (const r of records) {
        pending.current.push(
          `${ts}  ${r.serial}  x=${r.x.toFixed(3)} y=${r.y.toFixed(3)} θ=${r.theta.toFixed(3)}`,
        );
      }
    };

    return () => ws.close();
  }, [systemId]);

  // Flush buffered lines into state on a fixed cadence.
  useEffect(() => {
    const t = setInterval(() => {
      if (pending.current.length === 0) return;
      setLines((prev) => {
        const next = prev.concat(pending.current);
        pending.current = [];
        return next.length > MAX_LINES ? next.slice(next.length - MAX_LINES) : next;
      });
    }, FLUSH_MS);
    return () => clearInterval(t);
  }, []);

  // Auto-scroll to bottom unless the user has scrolled up.
  useEffect(() => {
    const el = scrollRef.current;
    if (el && autoScroll.current) el.scrollTop = el.scrollHeight;
  }, [lines]);

  const onScroll = () => {
    const el = scrollRef.current;
    if (!el) return;
    autoScroll.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
  };

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-3 border-b border-gray-200 px-4 py-2 text-sm">
        <span className="font-semibold">Published positions</span>
        <StatusBadge status={status} />
        <span className="text-gray-500">{robotCount} robot(s)</span>
        <div className="ml-auto flex gap-2">
          <button
            type="button"
            disabled={!systemId}
            onClick={() => setPaused((p) => !p)}
            className="rounded border border-gray-300 px-2 py-1 text-xs disabled:opacity-40"
          >
            {paused ? "Resume" : "Pause"}
          </button>
          <button
            type="button"
            onClick={() => {
              pending.current = [];
              setLines([]);
            }}
            className="rounded border border-gray-300 px-2 py-1 text-xs"
          >
            Clear
          </button>
        </div>
      </div>

      <div
        ref={scrollRef}
        onScroll={onScroll}
        className="flex-1 overflow-y-auto bg-gray-950 p-3 font-mono text-xs leading-relaxed text-gray-100"
      >
        {!systemId && (
          <div className="text-gray-500">Select a system to view its positions.</div>
        )}
        {systemId && lines.length === 0 && (
          <div className="text-gray-500">
            Waiting for frames… (start the system if it is stopped)
          </div>
        )}
        {lines.map((line, i) => (
          <div key={i} className="whitespace-pre">
            {line}
          </div>
        ))}
      </div>
    </div>
  );
}

function StatusBadge({ status }: { status: Status }) {
  const styles: Record<Status, string> = {
    idle: "bg-gray-200 text-gray-700",
    connecting: "bg-amber-100 text-amber-800",
    connected: "bg-green-100 text-green-800",
    closed: "bg-red-100 text-red-800",
  };
  return (
    <span className={`rounded-full px-2 py-0.5 text-xs font-semibold ${styles[status]}`}>
      {status}
    </span>
  );
}

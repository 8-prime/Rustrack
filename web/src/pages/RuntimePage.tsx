import { useEffect, useState } from "react";
import { Link, Navigate, useNavigate, useParams } from "react-router-dom";
import { PositionCanvas } from "../components/PositionCanvas";
import { PositionLog } from "../components/PositionLog";
import type { View } from "../components/viewControls";
import { getSystem } from "../lib/api";

type Load =
  | { kind: "loading" }
  | { kind: "missing" }
  | { kind: "error"; message: string }
  | { kind: "ready"; name: string };

function isView(value: string | undefined): value is View {
  return value === "map" || value === "log";
}

/**
 * The live view for one system, addressed by `/systems/:systemId/:view`.
 *
 * The URL is the whole navigable state: a reload, a bookmark or a shared link
 * reopens exactly this system in exactly this view.
 */
export function RuntimePage() {
  const { systemId, view } = useParams<{ systemId: string; view: string }>();
  const navigate = useNavigate();
  const [load, setLoad] = useState<Load>({ kind: "loading" });

  // Resolve the system's name, and — more importantly for a deep link — confirm
  // the id exists at all. Without this an unknown id renders an empty canvas
  // and a "closed" badge, with nothing saying why.
  useEffect(() => {
    if (!systemId) return;
    let cancelled = false;
    setLoad({ kind: "loading" });
    getSystem(systemId)
      .then((info) => {
        if (!cancelled) setLoad({ kind: "ready", name: info.config.name });
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        const message = e instanceof Error ? e.message : String(e);
        setLoad(
          message.startsWith("404") ? { kind: "missing" } : { kind: "error", message },
        );
      });
    return () => {
      cancelled = true;
    };
  }, [systemId]);

  if (!systemId) return <Navigate to="/systems" replace />;
  // An absolute path: relative resolution across a dynamic segment is subtle.
  if (!isView(view)) return <Navigate to={`/systems/${systemId}/map`} replace />;

  if (load.kind === "loading") {
    return (
      <div className="flex h-full w-full items-center justify-center text-sm text-gray-500">
        Loading…
      </div>
    );
  }

  if (load.kind !== "ready") {
    return (
      <div className="flex h-full w-full flex-col items-center justify-center gap-3">
        <p className="text-sm text-gray-400">
          {load.kind === "missing" ? "System not found." : load.message}
        </p>
        <Link to="/systems" className="text-sm text-amber-400 hover:underline">
          ← Systems
        </Link>
      </div>
    );
  }

  const onViewChange = (next: View) => navigate(`/systems/${systemId}/${next}`);
  const props = { systemId, systemName: load.name, view, onViewChange };

  return (
    <div className="h-full w-full">
      {view === "map" ? <PositionCanvas {...props} /> : <PositionLog {...props} />}
    </div>
  );
}

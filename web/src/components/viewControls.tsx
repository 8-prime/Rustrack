export type Status = "idle" | "connecting" | "connected" | "closed";
export type View = "map" | "log";

export function StatusBadge({ status }: { status: Status }) {
  const styles: Record<Status, string> = {
    idle: "bg-white/10 text-gray-300",
    connecting: "bg-amber-500/15 text-amber-300",
    connected: "bg-green-500/15 text-green-300",
    closed: "bg-red-500/15 text-red-300",
  };
  return (
    <span className={`rounded-full px-2 py-0.5 text-xs font-semibold ${styles[status]}`}>
      {status}
    </span>
  );
}

interface ViewToggleProps {
  view: View;
  onChange: (view: View) => void;
}

export function ViewToggle({ view, onChange }: ViewToggleProps) {
  const options: { value: View; label: string }[] = [
    { value: "map", label: "Map" },
    { value: "log", label: "Log" },
  ];
  return (
    <div className="inline-flex overflow-hidden rounded border border-white/15">
      {options.map((opt) => (
        <button
          key={opt.value}
          type="button"
          onClick={() => onChange(opt.value)}
          className={`px-2 py-1 text-xs ${
            view === opt.value
              ? "bg-amber-500 font-medium text-black"
              : "bg-transparent text-gray-300 hover:bg-white/10"
          }`}
        >
          {opt.label}
        </button>
      ))}
    </div>
  );
}

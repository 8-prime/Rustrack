export type Status = "idle" | "connecting" | "connected" | "closed";
export type View = "map" | "log";

export function StatusBadge({ status }: { status: Status }) {
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
    <div className="inline-flex overflow-hidden rounded border border-gray-300">
      {options.map((opt) => (
        <button
          key={opt.value}
          type="button"
          onClick={() => onChange(opt.value)}
          className={`px-2 py-1 text-xs ${
            view === opt.value
              ? "bg-gray-800 text-white"
              : "bg-white text-gray-700 hover:bg-gray-100"
          }`}
        >
          {opt.label}
        </button>
      ))}
    </div>
  );
}

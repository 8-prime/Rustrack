import { useState } from "react";
import { DeviceOrders } from "./DeviceOrders";

interface Props {
  systemId: string;
  serials: string[];
  followed: string | null;
  onZoomTo: (serial: string) => void;
  onToggleFollow: (serial: string) => void;
}

export function DevicePanel({ systemId, serials, followed, onZoomTo, onToggleFollow }: Props) {
  const [open, setOpen] = useState(true);
  const [query, setQuery] = useState("");
  const [expanded, setExpanded] = useState<string | null>(null);

  const needle = query.trim().toLowerCase();
  const matches = serials.filter((serial) => serial.toLowerCase().includes(needle));

  return (
    <div className="absolute right-3 top-3 w-64 rounded border border-white/10 bg-[#1f2124]/95 text-xs text-gray-200 shadow-lg">
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
        className="flex w-full items-center gap-2 px-2 py-1.5 font-semibold hover:bg-white/5"
      >
        <span>Devices ({serials.length})</span>
        <span aria-hidden className="ml-auto text-gray-400">
          {open ? "▾" : "▸"}
        </span>
      </button>

      {followed && (
        <div className="truncate border-t border-white/10 px-2 py-1 text-amber-300">
          Following {followed}
        </div>
      )}

      {open && (
        <div className="border-t border-white/10 p-2">
          <input
            type="search"
            aria-label="Search devices"
            placeholder="Search…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            className="mb-1 w-full rounded border border-white/15 bg-[#17181b] px-2 py-1 text-gray-200 placeholder:text-gray-500"
          />
          <ul className="max-h-60 overflow-y-auto">
            {matches.length === 0 && (
              <li className="px-1 py-1 text-gray-500">
                {serials.length === 0 ? "No devices yet." : "No matches."}
              </li>
            )}
            {matches.map((serial) => {
              const following = serial === followed;
              const showOrders = serial === expanded;
              return (
                <li key={serial}>
                  <div className="flex items-center gap-1">
                    <button
                      type="button"
                      aria-expanded={showOrders}
                      title={`${showOrders ? "Hide" : "Show"} orders for ${serial}`}
                      onClick={() => setExpanded(showOrders ? null : serial)}
                      className="rounded px-1 py-0.5 text-gray-400 hover:bg-white/10"
                    >
                      <span aria-hidden>{showOrders ? "▾" : "▸"}</span>
                    </button>
                    <button
                      type="button"
                      title={`Zoom to ${serial}`}
                      onClick={() => onZoomTo(serial)}
                      className="min-w-0 flex-1 truncate rounded px-1 py-1 text-left font-mono hover:bg-white/10"
                    >
                      {serial}
                    </button>
                    <button
                      type="button"
                      aria-pressed={following}
                      title={following ? `Stop following ${serial}` : `Follow ${serial}`}
                      onClick={() => onToggleFollow(serial)}
                      className={`rounded px-1.5 py-0.5 ${
                        following
                          ? "bg-amber-500 font-medium text-black"
                          : "text-gray-400 hover:bg-white/10"
                      }`}
                    >
                      Follow
                    </button>
                  </div>
                  {showOrders && <DeviceOrders key={systemId} systemId={systemId} serial={serial} />}
                </li>
              );
            })}
          </ul>
        </div>
      )}
    </div>
  );
}

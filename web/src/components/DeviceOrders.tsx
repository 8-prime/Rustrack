import { useEffect, useState } from "react";
import { getDeviceOrders, type OrderRecord, type OrderStatus } from "../lib/api";

const POLL_INTERVAL_MS = 2000;

const STATUS: Record<OrderStatus, { label: string; className: string }> = {
  active: { label: "Active", className: "text-sky-300" },
  finished: { label: "Finished", className: "text-emerald-400" },
  failed: { label: "Failed", className: "text-red-400" },
  unknown: { label: "Unknown", className: "text-gray-400" },
};

function clockTime(iso: string): string {
  return new Date(iso).toLocaleTimeString();
}

/** A device's last few orders, kept current for as long as it is on screen. */
export function DeviceOrders({ systemId, serial }: { systemId: string; serial: string }) {
  const [orders, setOrders] = useState<OrderRecord[] | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout>;

    // Chained rather than on an interval, so a slow backend can't pile up requests.
    async function poll() {
      try {
        const next = await getDeviceOrders(systemId, serial);
        if (cancelled) return;
        setOrders(next);
        setFailed(false);
      } catch {
        if (cancelled) return;
        setFailed(true);
      }
      timer = setTimeout(poll, POLL_INTERVAL_MS);
    }

    void poll();
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [systemId, serial]);

  if (failed) {
    return <p className="px-1 py-1 text-red-400">Could not load orders.</p>;
  }
  if (orders === null) {
    return <p className="px-1 py-1 text-gray-500">Loading…</p>;
  }
  if (orders.length === 0) {
    return <p className="px-1 py-1 text-gray-500">No orders yet.</p>;
  }

  return (
    <ol className="mb-1 ml-1 border-l border-white/10 pl-2">
      {orders.map((order) => {
        const status = STATUS[order.status];
        return (
          <li key={`${order.orderId}@${order.startedAt}`} className="py-0.5">
            <div className="flex items-baseline justify-between gap-2">
              <span className="min-w-0 truncate font-mono" title={order.orderId}>
                {order.orderId}
              </span>
              <span className={`shrink-0 ${status.className}`}>{status.label}</span>
            </div>
            <div className="text-[11px] text-gray-500">
              {clockTime(order.startedAt)}
              {order.endedAt && ` – ${clockTime(order.endedAt)}`}
              {order.orderUpdateId > 0 && ` · update ${order.orderUpdateId}`}
            </div>
          </li>
        );
      })}
    </ol>
  );
}

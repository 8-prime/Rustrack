import { SystemsPanel } from "../components/SystemsPanel";

/** Administration: manage the configured systems. */
export function AdminPage() {
  return (
    <div className="h-full w-full overflow-y-auto">
      <div className="mx-auto flex max-w-3xl flex-col gap-4 p-6">
        <h1 className="text-lg font-semibold">Rustrack — Systems</h1>
        <SystemsPanel />
      </div>
    </div>
  );
}

import { useState } from "react";
import "./App.css";
import { SystemsPanel } from "./components/SystemsPanel";
import { PositionLog } from "./components/PositionLog";
import { PositionCanvas } from "./components/PositionCanvas";
import type { View } from "./components/viewControls";

function App() {
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [view, setView] = useState<View>("map");

  return (
    <div className="flex h-screen w-screen bg-[#17181b] text-gray-200">
      <aside className="w-80 shrink-0 border-r border-white/10">
        <SystemsPanel selectedId={selectedId} onSelect={setSelectedId} />
      </aside>
      <main className="min-w-0 flex-1">
        {view === "map" ? (
          <PositionCanvas systemId={selectedId} view={view} onViewChange={setView} />
        ) : (
          <PositionLog systemId={selectedId} view={view} onViewChange={setView} />
        )}
      </main>
    </div>
  );
}

export default App;

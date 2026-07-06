import { useState } from "react";
import "./App.css";
import { SystemsPanel } from "./components/SystemsPanel";
import { PositionLog } from "./components/PositionLog";

function App() {
  const [selectedId, setSelectedId] = useState<string | null>(null);

  return (
    <div className="flex h-screen w-screen">
      <aside className="w-80 shrink-0 border-r border-gray-200">
        <SystemsPanel selectedId={selectedId} onSelect={setSelectedId} />
      </aside>
      <main className="min-w-0 flex-1">
        <PositionLog systemId={selectedId} />
      </main>
    </div>
  );
}

export default App;

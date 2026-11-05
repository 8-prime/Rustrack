import { Link, Navigate, Route, Routes, useParams } from "react-router-dom";
import "./App.css";
import { AdminPage } from "./pages/AdminPage";
import { RuntimePage } from "./pages/RuntimePage";

/** `/systems/:systemId` with no view segment defaults to the map. */
function RuntimeIndexRedirect() {
  const { systemId } = useParams<{ systemId: string }>();
  return <Navigate to={`/systems/${systemId}/map`} replace />;
}

function NotFound() {
  return (
    <div className="flex h-full w-full flex-col items-center justify-center gap-3">
      <p className="text-sm text-gray-400">This page does not exist.</p>
      <Link to="/systems" className="text-sm text-amber-400 hover:underline">
        ← Systems
      </Link>
    </div>
  );
}

function App() {
  return (
    <div className="flex h-screen w-screen bg-[#17181b] text-gray-200">
      <Routes>
        <Route path="/" element={<Navigate to="/systems" replace />} />
        <Route path="/systems" element={<AdminPage />} />
        <Route path="/systems/:systemId" element={<RuntimeIndexRedirect />} />
        <Route path="/systems/:systemId/:view" element={<RuntimePage />} />
        <Route path="*" element={<NotFound />} />
      </Routes>
    </div>
  );
}

export default App;

import { Routes, Route } from "react-router-dom";
import Home from "./pages/Home";
import GettingStarted from "./pages/GettingStarted";
import Concepts from "./pages/Concepts";
import State from "./pages/State";
import Nodes from "./pages/Nodes";
import Edges from "./pages/Edges";
import Runner from "./pages/Runner";

export default function App() {
  return (
    <Routes>
      <Route path="/" element={<Home />} />
      <Route path="/getting-started" element={<GettingStarted />} />
      <Route path="/concepts" element={<Concepts />} />
      <Route path="/api/state" element={<State />} />
      <Route path="/api/nodes" element={<Nodes />} />
      <Route path="/api/edges" element={<Edges />} />
      <Route path="/api/runner" element={<Runner />} />
    </Routes>
  );
}

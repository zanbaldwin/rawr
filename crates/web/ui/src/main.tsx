import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { sync } from "./data/instance";
import { attachSyncTriggers } from "./data/sync";
import { attachRouter } from "./router";
import "./styles.css";

attachRouter();
attachSyncTriggers(sync);
void sync.boot();

const root = document.getElementById("root");
if (!root) throw new Error("missing #root");
createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);

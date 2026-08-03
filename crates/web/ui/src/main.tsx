import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { sync } from "./data/instance";
import { attachSyncTriggers } from "./data/sync";
import { attachRouter } from "./router";
import { attachPwa } from "./sw/register";
import "./styles.css";

attachRouter();
attachSyncTriggers(sync);
attachPwa(sync);
void sync.boot();

const root = document.getElementById("root");
if (!root) throw new Error("missing #root");
createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);

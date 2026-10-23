import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { reads, sync } from "./data/instance";
import { followSync } from "./data/reads";
import { attachSyncTriggers } from "./data/sync";
import { attachRouter } from "./router";
import { attachPwa } from "./sw/register";
import "./styles.css";

attachRouter();
attachSyncTriggers(sync);
attachPwa(sync);
followSync(reads, sync.syncStore);
void reads.boot();
void sync.boot();

const root = document.getElementById("root");
if (!root) throw new Error("missing #root");
createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);

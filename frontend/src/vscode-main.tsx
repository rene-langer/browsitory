import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import "./index.css";
import { vscodeRepoClient } from "./ipc/vscodeRepoClient";
import { installGlobalErrorLogging } from "./lib/logger";

installGlobalErrorLogging();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App client={vscodeRepoClient} />
  </StrictMode>,
);

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import "./index.css";
import "./styles/vscode-tokens.css";
import { vscodeRepoClient } from "./ipc/vscodeRepoClient";
import { installGlobalErrorLogging } from "./lib/logger";
import { thirdPartyNoticesLoader, thirdPartyNoticesUrl } from "./lib/thirdPartyNotices";
import { followVscodeTheme } from "./lib/vscodeTheme";

installGlobalErrorLogging(vscodeRepoClient);
// The editor's theme colours the panel (#676).
followVscodeTheme();

// The bundle runs as one classic script, so `document.currentScript` is set while this runs:
// the notices file is found relative to it (see thirdPartyNoticesUrl).
const loadThirdPartyNotices = thirdPartyNoticesLoader(
  thirdPartyNoticesUrl(document.currentScript as HTMLScriptElement | null, document.baseURI),
);

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App client={vscodeRepoClient} toolHost="vscode" loadThirdPartyNotices={loadThirdPartyNotices} />
  </StrictMode>,
);

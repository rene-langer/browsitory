import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import "./index.css";
import "./styles/vscode-tokens.css";
import { vscodeRepoClient } from "./ipc/vscodeRepoClient";
import { installGlobalErrorLogging } from "./lib/logger";
import { thirdPartyNoticesLoader, thirdPartyNoticesUrl } from "./lib/thirdPartyNotices";
import { followVscodeTheme } from "./lib/vscodeTheme";
import { setHighlightTransport } from "./lib/highlight";
import { blobWorker, highlightWorkerScriptUrl } from "./lib/highlightBlobWorker";
import { inProcessTransport } from "./lib/highlightInProcess";
import { workerTransport } from "./lib/highlightWorkerProtocol";

installGlobalErrorLogging(vscodeRepoClient);
// The editor's theme colours the panel (#676).
followVscodeTheme();

// The bundle runs as one classic script, so `document.currentScript` is set while this runs:
// the notices file is found relative to it (see thirdPartyNoticesUrl).
const loadThirdPartyNotices = thirdPartyNoticesLoader(
  thirdPartyNoticesUrl(document.currentScript as HTMLScriptElement | null, document.baseURI),
);

// Syntax highlighting runs in a Web Worker started from a blob: (#843; see highlightBlobWorker),
// or on the page in chunks if it can't start. The URL is read now, while `currentScript` is set.
const highlightWorkerUrl = highlightWorkerScriptUrl(document.currentScript as HTMLScriptElement | null, document.baseURI);
setHighlightTransport(workerTransport(() => blobWorker(highlightWorkerUrl), inProcessTransport()));

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App client={vscodeRepoClient} toolHost="vscode" loadThirdPartyNotices={loadThirdPartyNotices} />
  </StrictMode>,
);

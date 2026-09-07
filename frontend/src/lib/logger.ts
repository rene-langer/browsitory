import type { RepoClient } from "../ipc/RepoClient";

/**
 * Global choke point for frontend failure logging, shared by both transports: routes through
 * `RepoClient.logFrontendError` so a bug report doesn't need console access to a running
 * session to diagnose, regardless of whether the app is running as the Tauri desktop app or
 * the VSCode extension's webview (which has no Tauri plugins available). See
 * `tauriRepoClient.ts` and `vscodeRepoClient.ts`/`sidecarBridge.ts` for where each transport
 * actually persists the message.
 */
export function installGlobalErrorLogging(client: RepoClient): () => void {
  const onError = (event: ErrorEvent) => {
    void client.logFrontendError("Uncaught error", event.error ?? event.message);
  };
  const onUnhandledRejection = (event: PromiseRejectionEvent) => {
    void client.logFrontendError("Unhandled rejection", event.reason);
  };

  window.addEventListener("error", onError);
  window.addEventListener("unhandledrejection", onUnhandledRejection);

  return () => {
    window.removeEventListener("error", onError);
    window.removeEventListener("unhandledrejection", onUnhandledRejection);
  };
}

/** Installs browser-safe diagnostics for the VSCode webview (which has no Tauri plugins). */
export function installWebviewErrorLogging(): () => void {
  const onError = (event: ErrorEvent) => {
    console.error("Uncaught error", event.error ?? event.message);
  };
  const onUnhandledRejection = (event: PromiseRejectionEvent) => {
    console.error("Unhandled rejection", event.reason);
  };

  window.addEventListener("error", onError);
  window.addEventListener("unhandledrejection", onUnhandledRejection);

  return () => {
    window.removeEventListener("error", onError);
    window.removeEventListener("unhandledrejection", onUnhandledRejection);
  };
}

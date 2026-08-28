import { error as logError } from "@tauri-apps/plugin-log";

/**
 * Single choke point for frontend failure logging: writes into the same rotated log file
 * as the Rust backend (see crates/tauri-app/src/main.rs's log plugin setup), so a bug report
 * doesn't need console access to a running dev session to diagnose.
 */
export function logFrontendError(context: string, error: unknown): void {
  void logError(`${context}: ${String(error)}`);
}

export function installGlobalErrorLogging(): void {
  window.addEventListener("error", (event) => {
    logFrontendError("Uncaught error", event.error ?? event.message);
  });
  window.addEventListener("unhandledrejection", (event) => {
    logFrontendError("Unhandled rejection", event.reason);
  });
}

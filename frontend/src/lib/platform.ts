/** Whether the page runs on macOS or iOS, where shortcuts use ⌘ and Back/Forward are ⌘[ / ⌘]. */
export function isApplePlatform(): boolean {
  if (typeof navigator === "undefined") return false;
  return /mac|iphone|ipad/i.test(`${navigator.platform ?? ""} ${navigator.userAgent ?? ""}`);
}

/**
 * Whether the page runs in the VSCode extension's webview (`data-host="vscode"` on the root
 * element, set by the extension's HTML).
 */
export function isVscodeHost(): boolean {
  return typeof document !== "undefined" && document.documentElement.dataset.host === "vscode";
}

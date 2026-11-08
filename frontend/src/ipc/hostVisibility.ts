/**
 * Whether the user can see Browsitory right now: the page is visible and the host has not
 * hidden it. A transport-neutral seam like `transportStatus.ts`: the desktop app relies on the
 * page's own `visibilitychange` (a minimized window hides the webview), while the VSCode
 * extension host also reports its panel (a webview kept alive behind another editor tab still
 * counts as a visible page) through `publishHostVisibility`.
 */
type VisibilityListener = (visible: boolean) => void;

const listeners = new Set<VisibilityListener>();
let hostVisible = true;

function pageVisible(): boolean {
  return typeof document === "undefined" || document.visibilityState !== "hidden";
}

export function isHostVisible(): boolean {
  return hostVisible && pageVisible();
}

function notify(): void {
  const visible = isHostVisible();
  for (const listener of listeners) listener(visible);
}

/** Called by a transport whose host can hide the page without the page noticing. */
export function publishHostVisibility(visible: boolean): void {
  if (hostVisible === visible) return;
  hostVisible = visible;
  notify();
}

/** Calls `listener` with `isHostVisible()` whenever the page or the host visibility changes. */
export function subscribeHostVisibility(listener: VisibilityListener): () => void {
  if (listeners.size === 0 && typeof document !== "undefined") {
    document.addEventListener("visibilitychange", notify);
  }
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0 && typeof document !== "undefined") {
      document.removeEventListener("visibilitychange", notify);
    }
  };
}

/** Test-only lifecycle cleanup for Vitest's shared module graph. */
export function __resetHostVisibilityForTests(): void {
  if (listeners.size > 0 && typeof document !== "undefined") {
    document.removeEventListener("visibilitychange", notify);
  }
  listeners.clear();
  hostVisible = true;
}

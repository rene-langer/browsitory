/** Whether the page runs on macOS or iOS, where Back/Forward are Cmd+[ / Cmd+]. */
export function isApplePlatform(): boolean {
  if (typeof navigator === "undefined") return false;
  return /mac|iphone|ipad/i.test(`${navigator.platform ?? ""} ${navigator.userAgent ?? ""}`);
}

type KeyLike = Pick<KeyboardEvent, "key" | "altKey" | "ctrlKey" | "metaKey" | "shiftKey">;

/**
 * Back (`-1`), Forward (`1`) or neither (`null`) for a key press (#618): Alt+Left / Alt+Right,
 * or Cmd+[ / Cmd+] on macOS (where Alt+Left moves by word in text).
 */
export function navigationKeyDirection(event: KeyLike, apple: boolean = isApplePlatform()): -1 | 1 | null {
  if (event.shiftKey) return null;
  if (apple) {
    if (!event.metaKey || event.ctrlKey || event.altKey) return null;
    return event.key === "[" ? -1 : event.key === "]" ? 1 : null;
  }
  if (!event.altKey || event.ctrlKey || event.metaKey) return null;
  return event.key === "ArrowLeft" ? -1 : event.key === "ArrowRight" ? 1 : null;
}

/** The Back and Forward keys as the platform writes them, for tooltips. */
export function navigationKeyLabels(apple: boolean = isApplePlatform()): { back: string; forward: string } {
  return apple ? { back: "⌘[", forward: "⌘]" } : { back: "Alt+Left", forward: "Alt+Right" };
}

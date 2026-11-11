/** Whether the page runs on macOS or iOS, where shortcuts use ⌘ and Back/Forward are ⌘[ / ⌘]. */
export function isApplePlatform(): boolean {
  if (typeof navigator === "undefined") return false;
  return /mac|iphone|ipad/i.test(`${navigator.platform ?? ""} ${navigator.userAgent ?? ""}`);
}

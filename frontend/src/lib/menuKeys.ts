/** Keys that open a context menu: the Menu key, or Shift+F10. */
export function isMenuKey(event: { key: string; shiftKey: boolean }): boolean {
  return event.key === "ContextMenu" || (event.shiftKey && event.key === "F10");
}

/** The element's bottom-left corner, for a menu opened by a key or a button. */
export function rectAnchor(element: Element): { x: number; y: number } {
  const rect = element.getBoundingClientRect();
  return { x: rect.left, y: rect.bottom };
}

/**
 * Where a menu opened by a `contextmenu` event goes: the pointer when it lies inside `element`
 * (a real right-click), else the element's bottom-left corner. Keyboard-generated events carry
 * `(0,0)` or browser-chosen coordinates, and `event.button` is not reliable across WebKitGTK,
 * WKWebView, WebView2 and Electron.
 */
export function menuAnchor(event: { clientX: number; clientY: number }, element: Element): { x: number; y: number } {
  const rect = element.getBoundingClientRect();
  const inside =
    event.clientX >= rect.left && event.clientX <= rect.right && event.clientY >= rect.top && event.clientY <= rect.bottom;
  return inside ? { x: event.clientX, y: event.clientY } : { x: rect.left, y: rect.bottom };
}

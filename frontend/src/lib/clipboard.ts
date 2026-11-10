/**
 * Writes plain text to the system clipboard (#614): the one path every copy action takes (the
 * context menus' "Copy …" items and the commit details' "Copy" button). Both frontends run in a
 * browser engine with the asynchronous Clipboard API (the desktop webview and the editor
 * extension's webview), so no transport is involved.
 *
 * Rejects when the clipboard is unavailable or refuses the write, so the caller can show an
 * error instead of failing silently.
 */
export async function copyText(text: string): Promise<void> {
  const clipboard = typeof navigator === "undefined" ? undefined : navigator.clipboard;
  if (typeof clipboard?.writeText !== "function") {
    throw new Error("The clipboard isn't available.");
  }
  await clipboard.writeText(text);
}

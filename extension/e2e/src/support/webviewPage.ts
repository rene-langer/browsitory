import { chromium, type Frame } from "playwright-core";

export async function connectToWebview(
  cdpHttpUrl: string,
  timeoutMs = 15000,
): Promise<Frame> {
  const browser = await chromium.connectOverCDP(cdpHttpUrl);
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    for (const context of browser.contexts()) {
      for (const page of context.pages()) {
        // The webview never appears as its own top-level CDP `page` target — VSCode renders it
        // as an `iframe` target nested inside the single workbench `page` target. Search that
        // page's frames rather than `context.pages()` for the `vscode-webview://` origin.
        //
        // VSCode also nests TWO frames under that origin, not one: an outer "presenter"/sandbox
        // wrapper frame (its `document.title` stays empty — it never holds the extension's real
        // DOM) and, nested inside it, the actual content frame the extension's HTML renders
        // into. Both frames' URLs start with `vscode-webview://`, so matching on the URL prefix
        // alone picks whichever one `Array.prototype.find` hits first — observed in practice to
        // be the empty outer wrapper, which then makes every subsequent `page.locator(...)`
        // call in the caller time out even though `connectToWebview` itself "succeeded". Filter
        // to the *leaf* frame (no children of its own) among the `vscode-webview://` matches —
        // that's structurally the innermost, content-holding one, and doesn't depend on VSCode's
        // internal frame naming/URL details staying stable across versions.
        const webviewFrames = page
          .frames()
          .filter((f) => f.url().startsWith("vscode-webview://"));
        const frame = webviewFrames.find((f) => f.childFrames().length === 0);
        if (frame) return frame;
      }
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`No vscode-webview:// frame found within ${timeoutMs}ms`);
}

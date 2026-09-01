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
        const frame = page.frames().find((f) => f.url().startsWith("vscode-webview://"));
        if (frame) return frame;
      }
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`No vscode-webview:// frame found within ${timeoutMs}ms`);
}

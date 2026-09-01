import { chromium, type Page } from "playwright-core";

export async function connectToWebview(
  cdpHttpUrl: string,
  timeoutMs = 15000,
): Promise<Page> {
  const browser = await chromium.connectOverCDP(cdpHttpUrl);
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    for (const context of browser.contexts()) {
      for (const page of context.pages()) {
        if (page.url().startsWith("vscode-webview://")) return page;
      }
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`No vscode-webview:// page found within ${timeoutMs}ms`);
}

import { describe, expect, it, vi } from "vitest";
import type { Uri, Webview } from "vscode";
import { renderWebviewHtml } from "./webviewHtml";

function fakeUri(value: string) {
  return { toString: () => value } as Uri;
}

function fakeWebview() {
  const asWebviewUri = vi.fn((uri: Uri) => fakeUri(`vscode-resource:${uri.toString()}`));
  return {
    cspSource: "vscode-webview://unit-test",
    asWebviewUri,
  } as unknown as Webview;
}

describe("renderWebviewHtml", () => {
  it("uses webview resource URIs and a nonce-restricted CSP without inline executable code", () => {
    const webview = fakeWebview();
    const script = fakeUri("/workspace/frontend/dist-vscode/assets/vscode-main.js");
    const style = fakeUri("/workspace/frontend/dist-vscode/assets/vscode-main.css");

    const html = renderWebviewHtml(webview, { script, style });

    expect(webview.asWebviewUri).toHaveBeenNthCalledWith(1, script);
    expect(webview.asWebviewUri).toHaveBeenNthCalledWith(2, style);
    expect(html).toContain(
      "default-src 'none'; img-src vscode-webview://unit-test data:; " +
        "font-src vscode-webview://unit-test; style-src vscode-webview://unit-test; " +
        "style-src-attr 'unsafe-inline'; script-src 'nonce-",
    );
    expect(html).toContain(
      'href="vscode-resource:/workspace/frontend/dist-vscode/assets/vscode-main.css"',
    );
    expect(html).toContain(
      'src="vscode-resource:/workspace/frontend/dist-vscode/assets/vscode-main.js"',
    );
    expect(html).not.toContain("https: data:");
    // `data:` only for images: never for scripts, styles, fonts or a default source.
    expect(html.match(/data:/g)).toHaveLength(1);
    expect(html).not.toMatch(/(?:default-src|script-src|style-src|font-src|connect-src|frame-src|object-src)[^;]*data:/);
    expect(html).not.toMatch(/img-src[^;]*(?:https:|http:|\*|blob:)/);
    expect(html).not.toMatch(/(?:script-src|style-src) [^;]*'unsafe-inline'/);
    expect(html).toMatch(/script-src 'nonce-[a-f0-9]{32}';/);
    expect(html).not.toContain("'unsafe-eval'");
    expect(html).not.toMatch(/<script[^>]*>\s*[^<\s]/);
  });

  it("generates a fresh nonce for each document", () => {
    const webview = fakeWebview();
    const assets = { script: fakeUri("script.js"), style: fakeUri("style.css") };

    const first = renderWebviewHtml(webview, assets);
    const second = renderWebviewHtml(webview, assets);
    const firstNonce = first.match(/nonce-([a-f0-9]+)'/)?.[1];
    const secondNonce = second.match(/nonce-([a-f0-9]+)'/)?.[1];

    expect(firstNonce).toMatch(/^[a-f0-9]{32}$/);
    expect(secondNonce).toMatch(/^[a-f0-9]{32}$/);
    expect(secondNonce).not.toBe(firstNonce);
    expect(first).toContain(`nonce="${firstNonce}"`);
    expect(second).toContain(`nonce="${secondNonce}"`);
  });
});

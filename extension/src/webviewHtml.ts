import { randomBytes } from "node:crypto";
import type { Uri, Webview } from "vscode";

export interface WebviewAssets {
  readonly script: Uri;
  readonly style: Uri;
}

export function renderWebviewHtml(webview: Webview, assets: WebviewAssets): string {
  const nonce = randomBytes(16).toString("hex");
  const scriptUri = escapeAttribute(webview.asWebviewUri(assets.script).toString());
  const styleUri = escapeAttribute(webview.asWebviewUri(assets.style).toString());
  const csp =
    "default-src 'none'; " +
    // `data:` for the image diff preview (base64 bytes from the backend; no network access).
    "img-src " +
    webview.cspSource +
    " data:; " +
    "font-src " +
    webview.cspSource +
    "; " +
    "style-src " +
    webview.cspSource +
    "; " +
    "style-src-attr 'unsafe-inline'; " +
    // The "Third-party licences" dialog fetches third-party-notices.md from the webview's own
    // files (localResourceRoots), nothing else.
    "connect-src " +
    webview.cspSource +
    "; " +
    "script-src 'nonce-" +
    nonce +
    "'; " +
    // Syntax highlighting runs in a Web Worker (#843). A webview can start one only from a
    // blob: URL: the frontend fetches its worker file (connect-src) and wraps it in a blob.
    "worker-src blob:;";

  return [
    "<!doctype html>",
    // Turns on the frontend's `styles/vscode-tokens.css` (#676) before the bundle runs.
    '<html lang="en" data-host="vscode">',
    "<head>",
    '<meta charset="UTF-8">',
    '<meta name="viewport" content="width=device-width, initial-scale=1.0">',
    '<meta http-equiv="Content-Security-Policy" content="' + escapeAttribute(csp) + '">',
    "<title>Browsitory</title>",
    '<link rel="stylesheet" href="' + styleUri + '">',
    "</head>",
    "<body>",
    '<div id="root"></div>',
    '<script nonce="' + nonce + '" src="' + scriptUri + '"></script>',
    "</body>",
    "</html>",
  ].join("\n");
}

function escapeAttribute(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll('"', "&quot;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;");
}

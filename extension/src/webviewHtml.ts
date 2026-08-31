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
    "img-src " +
    webview.cspSource +
    " https: data:; " +
    "font-src " +
    webview.cspSource +
    "; " +
    "style-src " +
    webview.cspSource +
    "; " +
    "script-src " +
    webview.cspSource +
    " 'nonce-" +
    nonce +
    "';";

  return [
    "<!doctype html>",
    '<html lang="en">',
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

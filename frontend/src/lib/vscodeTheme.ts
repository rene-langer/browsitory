import type { Theme } from "./theme";

/**
 * The app palette for the VSCode theme on `body` (#676). VSCode marks the webview's body with
 * the theme kind (`data-vscode-theme-kind` and a class of the same name): `vscode-light`,
 * `vscode-dark`, `vscode-high-contrast` or `vscode-high-contrast-light`.
 */
export function vscodeThemeOf(body: HTMLElement): Theme {
  const kind =
    body.dataset.vscodeThemeKind ??
    ["vscode-high-contrast-light", "vscode-high-contrast", "vscode-dark", "vscode-light"].find((name) =>
      body.classList.contains(name),
    );
  return kind === "vscode-dark" || kind === "vscode-high-contrast" ? "dark" : "light";
}

/**
 * Marks `root` as the VSCode webview (`data-host="vscode"`, which turns on
 * `styles/vscode-tokens.css`) and keeps its `data-theme` on the editor's theme kind as the user
 * switches themes. Returns a function that stops following.
 */
export function followVscodeTheme(root: HTMLElement = document.documentElement, body: HTMLElement = document.body): () => void {
  root.dataset.host = "vscode";
  const apply = () => {
    root.dataset.theme = vscodeThemeOf(body);
  };
  apply();
  const observer = new MutationObserver(apply);
  observer.observe(body, { attributes: true, attributeFilter: ["class", "data-vscode-theme-kind"] });
  return () => observer.disconnect();
}

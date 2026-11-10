import { afterEach, describe, expect, it } from "vitest";
import { followVscodeTheme, vscodeThemeOf } from "./vscodeTheme";

function setKind(body: HTMLElement, kind: string) {
  body.className = kind === "vscode-high-contrast-light" ? `vscode-high-contrast ${kind}` : kind;
  body.dataset.vscodeThemeKind = kind;
}

describe("vscodeThemeOf", () => {
  it("maps each VSCode theme kind to the app's light or dark palette", () => {
    const body = document.createElement("body");
    const cases: [string, string][] = [
      ["vscode-light", "light"],
      ["vscode-dark", "dark"],
      ["vscode-high-contrast", "dark"],
      ["vscode-high-contrast-light", "light"],
    ];
    for (const [kind, theme] of cases) {
      setKind(body, kind);
      expect(vscodeThemeOf(body)).toBe(theme);
    }
  });

  it("reads the body classes when the kind attribute is missing", () => {
    const body = document.createElement("body");
    body.className = "vscode-dark";
    expect(vscodeThemeOf(body)).toBe("dark");
  });

  it("is light when VSCode has set nothing yet", () => {
    expect(vscodeThemeOf(document.createElement("body"))).toBe("light");
  });
});

describe("followVscodeTheme", () => {
  const root = document.createElement("html");
  const body = document.createElement("body");
  let stop = () => {};

  afterEach(() => {
    stop();
    delete root.dataset.host;
    delete root.dataset.theme;
  });

  it("marks the root as the VSCode host and copies the theme kind", () => {
    setKind(body, "vscode-dark");

    stop = followVscodeTheme(root, body);

    expect(root.dataset.host).toBe("vscode");
    expect(root.dataset.theme).toBe("dark");
  });

  it("follows a theme change in the editor", async () => {
    setKind(body, "vscode-dark");
    stop = followVscodeTheme(root, body);

    setKind(body, "vscode-high-contrast-light");
    await Promise.resolve();

    expect(root.dataset.theme).toBe("light");
  });

  it("stops following when stopped", async () => {
    setKind(body, "vscode-light");
    stop = followVscodeTheme(root, body);
    stop();

    setKind(body, "vscode-dark");
    await Promise.resolve();

    expect(root.dataset.theme).toBe("light");
  });
});

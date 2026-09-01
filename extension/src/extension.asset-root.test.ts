import { describe, expect, it, vi } from "vitest";
import type { ExtensionMode, Uri } from "vscode";

vi.mock("vscode", () => ({
  Uri: {
    joinPath: (base: { fsPath: string }, ...parts: string[]) => ({
      fsPath: [base.fsPath, ...parts].join("/"),
    }),
  },
  ExtensionMode: { Development: 1, Test: 3, Production: 2 },
}));

type ExtensionModule = typeof import("./extension.js") & {
  resolveWebviewAssetRoot: (
    extensionUri: Uri,
    mode: ExtensionMode,
  ) => Uri;
};

describe("resolveWebviewAssetRoot", () => {
  it("uses the sibling frontend in development and package-local files in production", async () => {
    const extension = await import("./extension.js") as ExtensionModule;
    const uri = { fsPath: "/workspace/extension" } as unknown as Uri;

    expect(extension.resolveWebviewAssetRoot(uri, 1 as ExtensionMode).fsPath).toBe(
      "/workspace/extension/../frontend/dist-vscode",
    );
    expect(extension.resolveWebviewAssetRoot(uri, 2 as ExtensionMode).fsPath).toBe(
      "/workspace/extension/webview",
    );
    expect(extension.resolveWebviewAssetRoot(uri, 3 as ExtensionMode).fsPath).toBe(
      "/workspace/extension/../frontend/dist-vscode",
    );
  });
});

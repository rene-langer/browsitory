import { afterEach, describe, expect, it, vi } from "vitest";

const rootRender = vi.hoisted(() => vi.fn());

vi.mock("react-dom/client", () => ({
  createRoot: vi.fn(() => ({ render: rootRender })),
}));
vi.mock("@tauri-apps/plugin-log", () => {
  throw new Error("VSCode entry imported the Tauri logger");
});
vi.mock("@tauri-apps/plugin-updater", () => {
  throw new Error("VSCode entry imported the Tauri updater");
});
vi.mock("@tauri-apps/plugin-process", () => {
  throw new Error("VSCode entry imported the Tauri process plugin");
});
vi.mock("./ipc/tauriRepoClient", () => {
  throw new Error("VSCode entry imported the Tauri RepoClient");
});

describe("VSCode webview bootstrap", () => {
  afterEach(() => {
    rootRender.mockReset();
    vi.resetModules();
    document.body.replaceChildren();
  });

  it("loads and mounts without evaluating any Tauri-only module", async () => {
    const root = document.createElement("div");
    root.id = "root";
    document.body.append(root);

    await expect(import("./vscode-main")).resolves.toEqual(expect.any(Object));

    expect(rootRender).toHaveBeenCalledOnce();
    // The dynamic import loads and transforms the whole App module graph from cold inside the
    // test, so it gets more than the 5 s default: about 0.3 s in CI, but over 5 s on a busy
    // machine (#217).
  }, 20_000);
});

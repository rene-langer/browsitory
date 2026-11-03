import * as fs from "node:fs";
import * as path from "node:path";
import { describe, expect, it, vi } from "vitest";
import type { ExtensionContext } from "vscode";
import { HOST_SIDECAR_METHODS, NATIVE_METHODS, SidecarBridge } from "./sidecarBridge";

// The frontend's transport parity test (#306, `frontend/src/ipc/transportParity.test.ts`) reads
// these lists. Regenerate with
// `UPDATE_TRANSPORT_METHODS=1 pnpm exec vitest --run src/nativeMethods.test.ts`.
const GENERATED_DIR = path.resolve(__dirname, "../../frontend/src/ipc/generated");

function expectGeneratedList(file: string, names: Iterable<string>) {
  const generated = path.join(GENERATED_DIR, file);
  const expected = JSON.stringify([...names].sort(), null, 2) + "\n";
  if (process.env["UPDATE_TRANSPORT_METHODS"] === "1") {
    if (process.env["CI"]) throw new Error("UPDATE_TRANSPORT_METHODS must not be set in CI");
    fs.writeFileSync(generated, expected);
    return;
  }
  const actual = fs.existsSync(generated) ? fs.readFileSync(generated, "utf8") : "";
  expect(
    actual.replace(/\r\n/g, "\n"),
    `${generated} is out of date; rerun with UPDATE_TRANSPORT_METHODS=1`,
  ).toBe(expected);
}

/** A bridge whose sidecar never starts, so host methods that need it fail fast with a reply. */
function bridgeWithoutSidecar() {
  const postToWebview = vi.fn();
  const spawn = vi.fn((): never => {
    throw new Error("no sidecar in this test");
  });
  const bridge = new SidecarBridge({
    spawn,
    executablePath: "/nowhere/vscode-sidecar",
    context: {
      extension: { packageJSON: { version: "1.2.3" } },
      globalState: { get: () => undefined, update: async () => {} },
    } as unknown as ExtensionContext,
    postToWebview,
    showOpenDialog: async () => undefined,
    openExternal: async () => true,
    appendLine: () => {},
    openFile: async () => undefined,
    revealInOS: async () => undefined,
    createTerminal: () => {},
    openDiff: async () => undefined,
    openMergeEditor: async () => true,
  });
  return { bridge, postToWebview, spawn };
}

function replyTo(postToWebview: ReturnType<typeof vi.fn>, id: number) {
  return postToWebview.mock.calls
    .map(([message]) => message as { id?: number; error?: { code: number; message: string } })
    .find((message) => message.id === id);
}

describe("NATIVE_METHODS", () => {
  it("matches the checked-in list the frontend parity test reads", () => {
    expectGeneratedList("extension-native-methods.json", NATIVE_METHODS);
  });

  it.each([...NATIVE_METHODS])("%s is answered by the extension host", async (method) => {
    const { bridge, postToWebview } = bridgeWithoutSidecar();
    await bridge.handleWebviewMessage({ jsonrpc: "2.0", id: 7, method, params: {} });
    await vi.waitFor(() => expect(replyTo(postToWebview, 7)).toBeDefined());
    // Invalid params or a missing sidecar may fail the call; what matters is that it has a case.
    expect(replyTo(postToWebview, 7)?.error?.message ?? "").not.toMatch(/unknown native method/);
    await bridge.dispose();
  });
});

describe("HOST_SIDECAR_METHODS", () => {
  it("matches the checked-in list the frontend parity test reads", () => {
    expectGeneratedList("extension-host-sidecar-methods.json", Object.values(HOST_SIDECAR_METHODS));
  });

  it.each(Object.values(HOST_SIDECAR_METHODS))(
    "%s is refused for the webview without reaching the sidecar",
    async (method) => {
      const { bridge, postToWebview, spawn } = bridgeWithoutSidecar();
      await bridge.handleWebviewMessage({ jsonrpc: "2.0", id: 9, method, params: {} });
      expect(replyTo(postToWebview, 9)).toEqual({
        jsonrpc: "2.0",
        id: 9,
        error: { code: -32601, message: `${method} is not available to the webview` },
      });
      expect(spawn).not.toHaveBeenCalled();
      await bridge.dispose();
    },
  );
});

import { execFileSync, spawn as spawnProcess, type ChildProcessWithoutNullStreams } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import * as path from "node:path";
import { describe, expect, it, vi } from "vitest";
import type { ExtensionContext } from "vscode";
import {
  resolveDevelopmentSidecarPath,
  SidecarBridge,
} from "./sidecarBridge";

type JsonRpcMessage = Record<string, unknown>;

async function waitForMessage(
  messages: readonly JsonRpcMessage[],
  predicate: (message: JsonRpcMessage) => boolean,
) {
  await vi.waitFor(
    () => expect(messages.some(predicate)).toBe(true),
    { timeout: 10_000 },
  );
  const message = messages.find(predicate);
  if (!message) throw new Error("matching webview message disappeared");
  return message;
}

describe("SidecarBridge with the real vscode-sidecar", () => {
  it("reopens persisted repositories before serving a retry after a crash", async () => {
    const extensionRoot = path.resolve(__dirname, "..");
    const repoRoot = path.resolve(extensionRoot, "..");
    const executablePath = resolveDevelopmentSidecarPath(extensionRoot);
    if (!existsSync(executablePath)) {
      execFileSync(
        "cargo",
        ["build", "--manifest-path", path.join(repoRoot, "Cargo.toml"), "-p", "vscode-sidecar"],
        { cwd: repoRoot, stdio: "inherit" },
      );
    }

    const testRoot = mkdtempSync(path.join(tmpdir(), "browsitory-sidecar-bridge-"));
    const repoPath = path.join(testRoot, "repo");
    const configDir = path.join(testRoot, "config");
    mkdirSync(configDir);
    execFileSync("git", ["init", "--quiet", repoPath]);

    const children: ChildProcessWithoutNullStreams[] = [];
    const messages: JsonRpcMessage[] = [];
    const diagnostics: string[] = [];
    const context = {
      extension: { packageJSON: { version: "1.2.3" } },
      globalState: {
        get: () => undefined,
        update: async () => undefined,
      },
    } as unknown as ExtensionContext;
    const bridge = new SidecarBridge({
      spawn: (sidecarPath) => {
        const child = spawnProcess(sidecarPath, [], {
          stdio: ["pipe", "pipe", "pipe"],
          env: {
            ...process.env,
            BROWSITORY_CONFIG_DIR: configDir,
          },
        });
        children.push(child);
        return child;
      },
      executablePath,
      context,
      postToWebview: (message) => {
        messages.push(message);
      },
      showOpenDialog: async () => undefined,
      openExternal: async () => true,
      appendLine: (message) => diagnostics.push(message),
    });

    async function request(id: number, method: string, params: Record<string, unknown>) {
      const response = waitForMessage(messages, (message) => message["id"] === id);
      await bridge.handleWebviewMessage({ jsonrpc: "2.0", id, method, params });
      return response;
    }

    try {
      await expect(request(1, "open_repo", { path: repoPath })).resolves.toEqual({
        jsonrpc: "2.0",
        id: 1,
        result: null,
      });
      await expect(
        request(2, "persist_open_repos", {
          entries: [{ path: repoPath, workspaceId: null }],
          activePath: repoPath,
        }),
      ).resolves.toEqual({ jsonrpc: "2.0", id: 2, result: null });

      const transportLoss = waitForMessage(
        messages,
        (message) =>
          message["method"] === "transportStatus" &&
          (message["params"] as { state?: unknown } | undefined)?.state === "reconnecting",
      );
      expect(children[0]?.kill("SIGKILL")).toBe(true);
      await transportLoss;

      const retry = await request(3, "get_status", { repoPath });

      expect(retry).toEqual({ jsonrpc: "2.0", id: 3, result: [] });
      expect(children).toHaveLength(2);
      expect(diagnostics).not.toContainEqual(expect.stringContaining("repo not open"));
    } finally {
      await bridge.dispose();
      rmSync(testRoot, { recursive: true, force: true });
    }
  }, 120_000);
});

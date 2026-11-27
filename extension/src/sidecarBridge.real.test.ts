import { execFileSync, spawn as spawnProcess, type ChildProcessWithoutNullStreams } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
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

const executablePath = resolveDevelopmentSidecarPath(path.resolve(__dirname, ".."));
const sidecarMissing = !existsSync(executablePath);
const sidecarMissingMessage =
  `vscode-sidecar binary not found at ${executablePath}; ` +
  "build it first with `cargo build -p vscode-sidecar` (from the repo root)";

// The test never builds the binary itself (#552): a cold `cargo build` inside a JS test run
// is slow, unexpected and blows the timeout. Locally a missing binary skips these tests with
// a note; in CI (which builds the sidecar before `pnpm test`) it fails, so the coverage can't
// quietly disappear.
if (sidecarMissing) {
  if (process.env["CI"]) {
    throw new Error(`${sidecarMissingMessage}. CI must build it before the extension tests.`);
  }
  process.stderr.write(`Skipping the real-sidecar tests: ${sidecarMissingMessage}.\n`);
}

/**
 * This process's environment with a throwaway HOME, XDG config dir, git global config and GnuPG
 * home under `root`, `GIT_CONFIG_NOSYSTEM=1`, and without the SSH agent or any inherited `GIT_*`
 * variable (#1112): the sidecar's libgit2 reads `$HOME/.gitconfig`, so it and the `git init`
 * below never see the developer's config. The E2E runs use `extension/e2e`'s `isolatedHome.ts`
 * for the same, which this package can't import (`rootDir: "src"`).
 */
function isolatedEnv(root: string): NodeJS.ProcessEnv {
  const home = path.join(root, "home");
  mkdirSync(path.join(home, ".config"), { recursive: true });
  mkdirSync(path.join(home, ".gnupg"), { mode: 0o700 });
  writeFileSync(path.join(home, ".gitconfig"), "");
  const env: NodeJS.ProcessEnv = {};
  for (const [name, value] of Object.entries(process.env)) {
    if (name.toUpperCase().startsWith("GIT_") || name === "SSH_AUTH_SOCK" || name === "SSH_AGENT_PID") continue;
    env[name] = value;
  }
  return {
    ...env,
    HOME: home,
    XDG_CONFIG_HOME: path.join(home, ".config"),
    GIT_CONFIG_GLOBAL: path.join(home, ".gitconfig"),
    GIT_CONFIG_NOSYSTEM: "1",
    GNUPGHOME: path.join(home, ".gnupg"),
  };
}

describe.skipIf(sidecarMissing)("SidecarBridge with the real vscode-sidecar", () => {
  it("reopens persisted repositories before serving a retry after a crash", async () => {
    const testRoot = mkdtempSync(path.join(tmpdir(), "browsitory-sidecar-bridge-"));
    const repoPath = path.join(testRoot, "repo");
    const configDir = path.join(testRoot, "config");
    mkdirSync(configDir);
    const env = isolatedEnv(testRoot);
    execFileSync("git", ["init", "--quiet", repoPath], { env });

    const children: ChildProcessWithoutNullStreams[] = [];
    const messages: JsonRpcMessage[] = [];
    const diagnostics: string[] = [];
    // The window's state, where `persist_open_repos` keeps the open tabs (#1128).
    const windowState = new Map<string, unknown>();
    const context = {
      extension: { packageJSON: { version: "1.2.3" } },
      globalState: {
        get: () => undefined,
        update: async () => undefined,
      },
      workspaceState: {
        get: (key: string) => windowState.get(key),
        update: async (key: string, value: unknown) => {
          windowState.set(key, value);
        },
      },
    } as unknown as ExtensionContext;
    const bridge = new SidecarBridge({
      spawn: (sidecarPath) => {
        const child = spawnProcess(sidecarPath, [], {
          stdio: ["pipe", "pipe", "pipe"],
          env: {
            ...env,
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
    showSaveDialog: async () => undefined,
      readFile: async () => new Uint8Array(),
      writeFile: async () => {},
      openExternal: async () => true,
      appendLine: (message) => diagnostics.push(message),
      openFile: async () => undefined,
      revealInOS: async () => undefined,
      createTerminal: () => {},
      openDiff: async () => undefined,
      openMergeEditor: async () => true,
      executeCommand: async () => undefined,
      isWorkspaceTrusted: () => true,
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

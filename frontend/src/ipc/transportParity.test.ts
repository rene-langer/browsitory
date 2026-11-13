// Transport parity (#306): both `RepoClient` implementations must reach a backend that answers
// every method they call, and the two transports may differ only where an allow-list below says
// why.
//
// The backend method lists are generated, not copied by hand. Each is checked in under
// `./generated/` and kept current by the test suite of the code it describes (run it with
// `UPDATE_TRANSPORT_METHODS=1` to rewrite the file):
//
// - `tauri-commands.json`: the desktop app's `generate_handler!` list
//   (`crates/tauri-app/src/main.rs`, test `command_list_is_current`).
// - `sidecar-methods.json`: the VSCode sidecar's dispatch table
//   (`crates/vscode-sidecar/src/dispatch.rs`; test `method_list_is_current` in `dispatch/tests.rs`).
// - `extension-native-methods.json`: the methods the VSCode extension host answers itself
//   (`extension/src/sidecarBridge.ts`'s `NATIVE_METHODS`, test in `nativeMethods.test.ts`).
// - `extension-host-sidecar-methods.json`: the sidecar methods the extension host calls itself
//   and refuses to forward for the webview (`HOST_SIDECAR_METHODS`, same test).
//
// The client side is not parsed from source either: this test calls every method of each client
// with a mocked transport and records the backend name and the params it sends. Most methods come
// from the shared `createRepoClient` (#504), so they can't differ; the params check guards the
// methods a transport implements itself.
import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { RepoClient } from "./RepoClient";
import tauriCommands from "./generated/tauri-commands.json";
import sidecarMethods from "./generated/sidecar-methods.json";
import extensionNativeMethods from "./generated/extension-native-methods.json";
import extensionHostSidecarMethods from "./generated/extension-host-sidecar-methods.json";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/plugin-log", () => ({ error: vi.fn(async () => {}) }));

type Method = keyof RepoClient;

/** One backend call as a transport sent it. */
interface Sent {
  name: string;
  params: unknown;
}

/**
 * The arguments every method is called with: a distinct object per position, so the recorded
 * params show which argument went where (a spread argument such as `HunkOptions` adds its key).
 */
const ARGS = Array.from({ length: 8 }, (_, index) => ({ [`arg${index}`]: index }));

/** Methods that only register an event listener; they call no backend method on either side. */
const SUBSCRIPTIONS = [
  "subscribeTransferProgress",
  "subscribeHookProgress",
  "subscribeRepoWatch",
  "subscribeOperationLog",
  "subscribeFileDrops",
  "subscribeHostRequests",
] as const satisfies readonly Method[];

/**
 * Desktop only. VSCode has its own Git: Clone and Git: Initialize Repository commands, so the
 * extension reports `supportsCloneAndInit: false` and these reject without calling anything.
 * `readPatchFile` reads a file dropped on the window (#649); VSCode handles drops itself.
 */
const VSCODE_UNSUPPORTED = [
  "cloneRepo",
  "cancelClone",
  "initRepo",
  "readPatchFile",
] as const satisfies readonly Method[];

/**
 * No Tauri command: the desktop app writes frontend errors to its log file through
 * `@tauri-apps/plugin-log` directly. The extension sends `log_frontend_error` to the extension
 * host, which writes it to the "Browsitory" output channel.
 */
const TAURI_NO_COMMAND = ["logFrontendError"] as const satisfies readonly Method[];

/**
 * VSCode only (#680): the picker's Clone… and New repository… buttons run VSCode's own Git
 * commands there; the desktop app clones and initializes itself, so this rejects without a call.
 */
const TAURI_UNSUPPORTED = ["runHostGitCommand"] as const satisfies readonly Method[];

/**
 * Sidecar methods the webview never calls. The extension host itself calls them, to read file
 * versions for VSCode's diff and merge editors, to check a merge target before it opens the
 * merge editor, and to write a file version to the path picked in its save dialog.
 */
const SIDECAR_HOST_ONLY = ["check_merge_target", "read_file_version", "repo_workdir", "write_file_version"];

const sorted = (names: Iterable<string>) => [...names].sort();

function methodsOf(client: RepoClient): Method[] {
  return (Object.keys(client) as Method[]).filter((name) => typeof client[name] === "function");
}

/** Calls `method` with `ARGS` and returns the backend calls it sent through `sent`. */
async function backendCallsFor(client: RepoClient, method: Method, sent: () => Sent[]): Promise<Sent[]> {
  const before = sent().length;
  try {
    const result: unknown = (client[method] as (...args: unknown[]) => unknown)(...ARGS);
    // A vscode call stays pending until a reply arrives; the message is already posted by then.
    if (result instanceof Promise) {
      await Promise.race([result.catch(() => {}), new Promise((resolve) => setTimeout(resolve, 0))]);
    }
  } catch {
    // Argument validation may throw; only what was sent matters here.
  }
  return sent().slice(before);
}

async function backendCalls(client: RepoClient, sent: () => Sent[]): Promise<Map<Method, Sent[]>> {
  const calls = new Map<Method, Sent[]>();
  for (const method of methodsOf(client)) {
    calls.set(method, await backendCallsFor(client, method, sent));
  }
  return calls;
}

/** Each method sends exactly one backend call, except the listed methods, which send none. */
function singleCalls(calls: Map<Method, Sent[]>, sendNothing: readonly Method[]): Map<Method, Sent> {
  const single = new Map<Method, Sent>();
  for (const [method, sent] of calls) {
    if (sendNothing.includes(method)) {
      expect(sent, `${method} should call no backend method`).toEqual([]);
    } else {
      expect(sent, `${method} should call exactly one backend method`).toHaveLength(1);
      single.set(method, sent[0]!);
    }
  }
  return single;
}

const namesOf = (calls: Map<Method, Sent>) => new Map([...calls].map(([method, { name }]) => [method, name]));

describe("transport parity", () => {
  let tauriCalls: Map<Method, Sent>;
  let vscodeCalls: Map<Method, Sent>;
  let tauriNames: Map<Method, string>;
  let vscodeNames: Map<Method, string>;
  let resetVscode: (() => void) | undefined;

  beforeEach(async () => {
    vi.resetModules();
    const invoked: Sent[] = [];
    vi.mocked(invoke).mockImplementation(async (command: string, args?: unknown) => {
      // A command without arguments is invoked with none; the VSCode client sends `{}` for it.
      invoked.push({ name: command, params: args ?? {} });
      return [];
    });
    const { tauriRepoClient } = await import("./tauriRepoClient");
    tauriCalls = singleCalls(
      await backendCalls(tauriRepoClient, () => invoked),
      [...SUBSCRIPTIONS, ...TAURI_NO_COMMAND, ...TAURI_UNSUPPORTED],
    );
    tauriNames = namesOf(tauriCalls);

    const posted: Sent[] = [];
    (globalThis as unknown as { acquireVsCodeApi: () => unknown }).acquireVsCodeApi = () => ({
      // A notification to the extension host itself (`hostReady`, `webviewFocus`; no `id`) is
      // no backend call.
      postMessage: (message: { id?: number; method: string; params: unknown }) => {
        if (message.id !== undefined) posted.push({ name: message.method, params: message.params });
      },
    });
    const vscodeModule = await import("./vscodeRepoClient");
    resetVscode = vscodeModule.__resetVscodeRepoClientForTests;
    vscodeCalls = singleCalls(
      await backendCalls(vscodeModule.vscodeRepoClient, () => posted),
      [...SUBSCRIPTIONS, ...VSCODE_UNSUPPORTED],
    );
    vscodeNames = namesOf(vscodeCalls);
  });

  afterEach(() => {
    resetVscode?.();
  });

  it("the desktop client calls every registered Tauri command, and only those", () => {
    expect(sorted(new Set(tauriNames.values()))).toEqual(sorted(tauriCommands));
  });

  it("every method the VSCode client calls is answered by the sidecar or the extension host", () => {
    const answered = new Set([...sidecarMethods, ...extensionNativeMethods]);
    const unanswered = [...vscodeNames.values()].filter((name) => !answered.has(name));
    expect(unanswered).toEqual([]);
  });

  it("the extension host and the sidecar never both answer a method", () => {
    const sidecar = new Set(sidecarMethods);
    expect(extensionNativeMethods.filter((name) => sidecar.has(name))).toEqual([]);
  });

  it("every sidecar and extension host method is called, except the host-only ones", () => {
    const called = new Set(vscodeNames.values());
    const uncalled = [...sidecarMethods, ...extensionNativeMethods].filter((name) => !called.has(name));
    expect(sorted(uncalled)).toEqual(sorted(SIDECAR_HOST_ONLY));
  });

  it("the host-only sidecar methods are the ones the extension host calls, and exist", () => {
    expect(sorted(SIDECAR_HOST_ONLY)).toEqual(sorted(extensionHostSidecarMethods));
    const sidecar = new Set(sidecarMethods);
    expect(extensionHostSidecarMethods.filter((name) => !sidecar.has(name))).toEqual([]);
  });

  it("both clients use the same backend name for each method they share", () => {
    const differing = [...tauriNames]
      .filter(([method, name]) => vscodeNames.has(method) && vscodeNames.get(method) !== name)
      .map(([method]) => method);
    expect(differing).toEqual([]);
  });

  it("both clients send the same params for each method they share", () => {
    const shared = [...tauriCalls.keys()].filter((method) => vscodeCalls.has(method));
    const paramsOf = (calls: Map<Method, Sent>) =>
      Object.fromEntries(shared.map((method) => [method, calls.get(method)!.params]));
    expect(paramsOf(vscodeCalls)).toStrictEqual(paramsOf(tauriCalls));
  });

  it("records the arguments each method sends, not just their count", () => {
    // Guards the check above: with arguments that all looked alike it would pass vacuously.
    expect(tauriCalls.get("getStatus")?.params).toEqual({ repoPath: ARGS[0] });
    expect(vscodeCalls.get("stageHunk")?.params).toEqual({
      repoPath: ARGS[0],
      path: ARGS[1],
      oldStart: ARGS[2],
      newStart: ARGS[3],
      ...ARGS[4],
    });
  });

  it("the transports differ only in the allow-listed methods", () => {
    const tauriOnly = [...tauriNames.keys()].filter((method) => !vscodeNames.has(method));
    const vscodeOnly = [...vscodeNames.keys()].filter((method) => !tauriNames.has(method));
    expect(sorted(tauriOnly)).toEqual(sorted(VSCODE_UNSUPPORTED));
    expect(sorted(vscodeOnly)).toEqual(sorted([...TAURI_NO_COMMAND, ...TAURI_UNSUPPORTED]));
  });
});

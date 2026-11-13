import type { HookProgress, HostRequest, OpenRepoEntry, OperationLogChange, RepoClient, RepoWatch, TransferProgress } from "./RepoClient";
import { createRepoClient } from "./createRepoClient";
import { RepoError, toRepoError } from "./RepoClient";
import { __resetHostVisibilityForTests, publishHostVisibility } from "./hostVisibility";
import {
  __resetTransportStatusForTests,
  publishTransportStatus,
  type TransportStatus,
} from "./transportStatus";

// No `vscode` npm package dependency here on purpose — the real extension host's webview API
// typings arrive with sub-phase (c)'s `extension/` package. This ambient declaration is the
// minimal shape this file actually calls.
interface VsCodeWebviewApi {
  postMessage(message: unknown): void;
}

declare function acquireVsCodeApi(): VsCodeWebviewApi;

interface JsonRpcSuccess {
  jsonrpc: "2.0";
  id: number;
  result: unknown;
}

interface JsonRpcFailure {
  jsonrpc: "2.0";
  id: number;
  /** `data` is the sidecar's `RepoErrorPayload` (#496); the extension host's own errors have none. */
  error: { code: number; message: string; data?: unknown };
}

// The JSON-RPC code the extension host answers a request with when it lost the sidecar
// (`TRANSPORT_ERROR_CODE` in `extension/src/sidecarBridge.ts`).
const TRANSPORT_LOST_CODE = -32001;

function rejectionOf(error: JsonRpcFailure["error"]): RepoError {
  if (error.code === TRANSPORT_LOST_CODE) return new RepoError("TransportLost", error.message);
  const kind = toRepoError(error.data).kind;
  return new RepoError(kind, error.message);
}

type JsonRpcResponse = JsonRpcSuccess | JsonRpcFailure;

function isJsonRpcResponse(value: unknown): value is JsonRpcResponse {
  return (
    typeof value === "object" &&
    value !== null &&
    "jsonrpc" in value &&
    "id" in value &&
    ("result" in value || "error" in value)
  );
}

// A JSON-RPC *notification* is the sidecar pushing something unprompted: a `method` and `params`,
// and — the load-bearing part — no `id`. That absence is the entire wire distinction from a
// response, and `isJsonRpcResponse` above already requires `"id" in value`, so the two guards can
// never both match the same message.
interface JsonRpcNotification {
  jsonrpc: "2.0";
  method: string;
  params: unknown;
}

function isJsonRpcNotification(value: unknown): value is JsonRpcNotification {
  return (
    typeof value === "object" &&
    value !== null &&
    "jsonrpc" in value &&
    "method" in value &&
    !("id" in value)
  );
}


function isTransportStatus(value: unknown): value is TransportStatus {
  if (typeof value !== "object" || value === null || !("state" in value) || !("message" in value)) {
    return false;
  }
  return (
    (value.state === "reconnecting" || value.state === "failed") &&
    typeof value.message === "string"
  );
}

let vscode: VsCodeWebviewApi | undefined;
let listenerRegistered = false;
let nextRequestId = 1;
const pending = new Map<
  number,
  { resolve: (value: unknown) => void; reject: (error: Error) => void }
>();
const transferProgressListeners = new Set<(progress: TransferProgress) => void>();
const hookProgressListeners = new Set<(event: HookProgress) => void>();
const hostRequestListeners = new Set<(request: HostRequest) => void>();
const repoWatchListeners = new Set<(event: RepoWatch) => void>();
const operationLogListeners = new Set<(change: OperationLogChange) => void>();
let focusTracked = false;

function handleMessage(event: MessageEvent) {
  const message = event.data;
  if (isJsonRpcResponse(message)) {
    const waiting = pending.get(message.id);
    if (!waiting) return;
    pending.delete(message.id);
    if ("error" in message) {
      waiting.reject(rejectionOf(message.error));
    } else {
      waiting.resolve(message.result);
    }
    return;
  }
  if (!isJsonRpcNotification(message)) return;
  if (message.method === "transferProgress") {
    const progress = message.params as TransferProgress;
    for (const listener of transferProgressListeners) listener(progress);
    return;
  }
  if (message.method === "hookProgress") {
    const hookEvent = message.params as HookProgress;
    for (const listener of hookProgressListeners) listener(hookEvent);
    return;
  }
  if (message.method === "repoWatch") {
    // A repository watcher's report (#756), relayed from the sidecar.
    const watchEvent = message.params as RepoWatch;
    for (const listener of repoWatchListeners) listener(watchEvent);
    return;
  }
  if (message.method === "operationLog") {
    // An operation log entry added or ended (#888), relayed from the sidecar.
    const change = message.params as OperationLogChange;
    for (const listener of operationLogListeners) listener(change);
    return;
  }
  if (message.method === "hostRequest") {
    // "Browsitory: Open", the menus' history commands and the contributed shortcut keys (#677).
    const request = message.params as HostRequest;
    for (const listener of hostRequestListeners) listener(request);
    return;
  }
  if (message.method === "panelVisibility") {
    // The extension host's `onDidChangeViewState`: a panel behind another editor tab keeps its
    // page "visible", so the page alone can't tell.
    const params = message.params as { visible?: unknown } | null;
    if (typeof params?.visible === "boolean") publishHostVisibility(params.visible);
    return;
  }
  if (message.method !== "transportStatus" || !isTransportStatus(message.params)) return;
  const status = message.params;
  const error = new RepoError("TransportLost", status.message);
  const waitingCalls = [...pending.values()];
  pending.clear();
  for (const waiting of waitingCalls) waiting.reject(error);
  publishTransportStatus(status);
}

function ensureMessageListener() {
  if (listenerRegistered) return;
  listenerRegistered = true;
  window.addEventListener("message", handleMessage);
}

// Importing the VS Code adapter activates its inbound notification bridge.
ensureMessageListener();

// Lazily acquire the VSCode API so importing this module remains safe in the Tauri bundle.
function ensureInitialized(): VsCodeWebviewApi {
  if (!vscode) vscode = acquireVsCodeApi();
  ensureMessageListener();
  return vscode;
}

/** A notification to the extension host itself: no `id`, no reply. */
function notifyHost(method: string, params?: Record<string, unknown>): void {
  ensureInitialized().postMessage(params === undefined ? { jsonrpc: "2.0", method } : { jsonrpc: "2.0", method, params });
}

function reportFocus(): void {
  notifyHost("webviewFocus", { focused: document.hasFocus() });
}

/**
 * Tells the host whether the webview has the focus: its contributed shortcut keybindings apply
 * only then (#679), so F6 in the terminal stays VSCode's while the panel is the active editor.
 */
function trackFocus(): void {
  if (focusTracked) return;
  focusTracked = true;
  window.addEventListener("focus", reportFocus);
  window.addEventListener("blur", reportFocus);
  reportFocus();
}

// A method without arguments sends empty `params`.
function call<T>(method: string, params: Record<string, unknown> = {}): Promise<T> {
  const api = ensureInitialized();
  const id = nextRequestId++;
  return new Promise<T>((resolve, reject) => {
    pending.set(id, { resolve: resolve as (value: unknown) => void, reject });
    api.postMessage({ jsonrpc: "2.0", id, method, params });
  });
}


/** Test-only lifecycle cleanup for Vitest's shared window. */
export function __resetVscodeRepoClientForTests() {
  if (listenerRegistered) {
    window.removeEventListener("message", handleMessage);
    listenerRegistered = false;
  }
  if (focusTracked) {
    window.removeEventListener("focus", reportFocus);
    window.removeEventListener("blur", reportFocus);
    focusTracked = false;
  }
  hostRequestListeners.clear();
  vscode = undefined;
  nextRequestId = 1;
  pending.clear();
  transferProgressListeners.clear();
  hookProgressListeners.clear();
  repoWatchListeners.clear();
  operationLogListeners.clear();
  __resetTransportStatusForTests();
  __resetHostVisibilityForTests();
}

export const vscodeRepoClient: RepoClient = {
  ...createRepoClient(call),
  listOpenRepos: () =>
    call<{ entries: OpenRepoEntry[]; activePath: string | null }>("list_open_repos"),
  supportsCloneAndInit: false,
  cloneRepo: () => Promise.reject(new Error("Not supported in the VSCode extension")),
  cancelClone: () => Promise.reject(new Error("Not supported in the VSCode extension")),
  initRepo: () => Promise.reject(new Error("Not supported in the VSCode extension")),
  supportsHostGitCommands: true,
  runHostGitCommand: (command: "clone" | "init") => call<void>("run_vscode_git_command", { command }),
  openHostKeybindings: () => call<void>("open_vscode_keybindings"),
  userCommandsBlocked: () => call<string | null>("user_commands_blocked"),
  subscribeTransferProgress: (listener: (progress: TransferProgress) => void) => {
    // Initialize eagerly: a caller may subscribe before it ever issues a request, and the
    // `message` listener registered here is what delivers notifications.
    ensureInitialized();
    transferProgressListeners.add(listener);
    return () => {
      transferProgressListeners.delete(listener);
    };
  },
  subscribeHookProgress: (listener: (event: HookProgress) => void) => {
    ensureInitialized();
    hookProgressListeners.add(listener);
    return () => {
      hookProgressListeners.delete(listener);
    };
  },
  subscribeRepoWatch: (listener: (event: RepoWatch) => void) => {
    ensureInitialized();
    repoWatchListeners.add(listener);
    return () => {
      repoWatchListeners.delete(listener);
    };
  },
  subscribeOperationLog: (listener: (change: OperationLogChange) => void) => {
    ensureInitialized();
    operationLogListeners.add(listener);
    return () => {
      operationLogListeners.delete(listener);
    };
  },
  subscribeHostRequests: (listener: (request: HostRequest) => void) => {
    hostRequestListeners.add(listener);
    trackFocus();
    // The host holds its requests until the app listens (the panel may still be loading).
    notifyHost("hostReady");
    return () => {
      hostRequestListeners.delete(listener);
    };
  },
  // VSCode handles what is dropped on its windows (#649): no drops reach the webview's app.
  subscribeFileDrops: () => () => {},
  readPatchFile: () => Promise.reject(new Error("Not supported in the VSCode extension")),
  // Handled natively by the extension host (see `sidecarBridge.ts`'s `NATIVE_METHODS`), which
  // writes into the "Browsitory" output channel rather than forwarding to the sidecar process.
  logFrontendError: (context: string, error: unknown) =>
    call<void>("log_frontend_error", { context, message: String(error) }),
};

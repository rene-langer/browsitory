import * as path from "node:path";
import { StringDecoder } from "node:string_decoder";
import type { ExtensionContext, OpenDialogOptions, Uri } from "vscode";
import {
  diffSides,
  isSafeRelativePath,
  mergeSides,
  resolveRepoFile,
  type DiffSource,
  type FileVersionRef,
} from "./toolUris";

const LAST_SEEN_VERSION_KEY = "lastSeenVersion";
const NATIVE_METHODS = new Set([
  "pick_repo_folder",
  "open_external_url",
  "get_app_version",
  "get_last_seen_version",
  "set_last_seen_version",
  "log_frontend_error",
  // External tools open in VSCode itself; the sidecar is only asked for the work tree root.
  "open_in_editor",
  "reveal_in_file_manager",
  "open_terminal",
  "open_diff_tool",
  "open_merge_tool",
]);

interface SidecarReadable {
  on(event: "data", listener: (chunk: Buffer | string) => void): unknown;
  off(event: "data", listener: (chunk: Buffer | string) => void): unknown;
}

interface SidecarWritable {
  write(chunk: string): boolean;
  on(event: "error", listener: (error: Error) => void): unknown;
  off(event: "error", listener: (error: Error) => void): unknown;
}

export interface SidecarProcess {
  readonly stdin: SidecarWritable;
  readonly stdout: SidecarReadable;
  readonly stderr: SidecarReadable;
  kill(): boolean;
  on(event: "error", listener: (error: Error) => void): unknown;
  on(
    event: "exit",
    listener: (code: number | null, signal: NodeJS.Signals | null) => void,
  ): unknown;
  off(event: "error", listener: (error: Error) => void): unknown;
  off(
    event: "exit",
    listener: (code: number | null, signal: NodeJS.Signals | null) => void,
  ): unknown;
}

export interface JsonRpcRequest {
  readonly jsonrpc: "2.0";
  readonly id: number;
  readonly method: string;
  readonly params: Record<string, unknown>;
}

type JsonRpcMessage = Record<string, unknown>;

interface InternalRequestWaiter {
  readonly resolve: (value: unknown) => void;
  readonly reject: (error: Error) => void;
}

export interface SidecarBridgeDependencies {
  readonly spawn: (executablePath: string) => SidecarProcess;
  readonly executablePath: string;
  readonly context: ExtensionContext;
  readonly postToWebview: (message: JsonRpcMessage) => unknown;
  readonly showOpenDialog: (
    options: OpenDialogOptions,
  ) => PromiseLike<readonly Uri[] | undefined>;
  readonly openExternal: (url: string) => PromiseLike<boolean>;
  readonly appendLine: (message: string) => void;
  /** Opens a work tree file in a VSCode editor, at the 1-based `line` when given. */
  readonly openFile: (path: string, line: number | null) => PromiseLike<unknown>;
  readonly revealInOS: (path: string) => PromiseLike<unknown>;
  readonly createTerminal: (cwd: string, name: string) => void;
  /** `left`/`right` are URI strings (`browsitory-file:` or `file:`). */
  readonly openDiff: (left: string, right: string, title: string) => PromiseLike<unknown>;
  /** Resolves `false` when the merge editor wasn't available and the file opened plainly. */
  readonly openMergeEditor: (sides: {
    base: string;
    ours: string;
    theirs: string;
    output: string;
  }) => PromiseLike<boolean>;
}

class InvalidParamsError extends Error {}

type SidecarBridgeState = "idle" | "running" | "reconnecting" | "failed";

const TRANSPORT_ERROR_CODE = -32001;
const DISPOSED_MESSAGE = "Browsitory sidecar bridge disposed";

/**
 * Splits a text stream into `\n`-terminated lines. Only each new chunk is scanned for a newline,
 * and a partial line is kept as a list of pieces joined once its newline arrives, so a single
 * multi-megabyte line delivered in many small chunks costs linear rather than quadratic time.
 */
export class LineAccumulator {
  private pending: string[] = [];

  constructor(private readonly onLine: (line: string) => void) {}

  push(text: string): void {
    let start = 0;
    let newline = text.indexOf("\n");
    while (newline !== -1) {
      const tail = text.slice(start, newline);
      const line = this.pending.length === 0 ? tail : this.pending.join("") + tail;
      this.pending = [];
      start = newline + 1;
      this.onLine(line);
      newline = text.indexOf("\n", start);
    }
    if (start < text.length) this.pending.push(text.slice(start));
  }
}

export class SidecarBridge {
  private sidecar: SidecarProcess | undefined;
  private state: SidecarBridgeState = "idle";
  private detachSidecarListeners: (() => void) | undefined;
  private readonly pendingRequestIds = new Set<number>();
  private readonly internalRequestWaiters = new Map<number, InternalRequestWaiter>();
  private nextInternalRequestId = Number.MAX_SAFE_INTEGER;
  private sidecarStartup: Promise<SidecarProcess | undefined> | undefined;
  private restoreRepositoriesOnNextSpawn = false;
  private disposed = false;

  constructor(private readonly dependencies: SidecarBridgeDependencies) {}

  async handleWebviewMessage(message: unknown): Promise<void> {
    if (!isJsonRpcRequest(message)) {
      this.dependencies.appendLine("Browsitory: ignoring malformed webview JSON-RPC request");
      return;
    }

    if (NATIVE_METHODS.has(message.method)) {
      await this.handleNativeRequest(message);
      return;
    }

    this.pendingRequestIds.add(message.id);
    if (this.disposed) {
      this.rejectPendingAndNotify("failed", DISPOSED_MESSAGE);
      return;
    }

    const sidecar = await this.ensureSidecar();
    if (!sidecar) return;
    try {
      sidecar.stdin.write(JSON.stringify(message) + "\n");
    } catch (error) {
      this.handleProcessLoss(
        sidecar,
        "Browsitory sidecar stdin write failed: " + errorMessage(error),
        true,
      );
    }
  }

  dispose(): Promise<void> {
    if (this.disposed) return Promise.resolve();
    this.disposed = true;
    const sidecar = this.sidecar;
    this.sidecarStartup = undefined;
    this.detachCurrentSidecar();
    this.sidecar = undefined;
    this.state = "idle";
    this.rejectInternalRequests(DISPOSED_MESSAGE);
    if (sidecar) this.killSidecar(sidecar);
    if (this.pendingRequestIds.size > 0) {
      this.rejectPendingAndNotify("failed", DISPOSED_MESSAGE);
    }
    return Promise.resolve();
  }

  private async ensureSidecar(): Promise<SidecarProcess | undefined> {
    if (this.state === "running" && this.sidecar) return this.sidecar;
    if (this.sidecarStartup) return this.sidecarStartup;

    const startup = this.spawnSidecar();
    this.sidecarStartup = startup;
    try {
      return await startup;
    } finally {
      if (this.sidecarStartup === startup) this.sidecarStartup = undefined;
    }
  }

  private async spawnSidecar(): Promise<SidecarProcess | undefined> {
    let sidecar: SidecarProcess;
    try {
      sidecar = this.dependencies.spawn(this.dependencies.executablePath);
    } catch (error) {
      const message = "Browsitory sidecar failed to start: " + errorMessage(error);
      this.state = "failed";
      this.dependencies.appendLine(message);
      this.rejectPendingAndNotify("failed", message);
      return undefined;
    }

    this.sidecar = sidecar;
    this.attachSidecar(sidecar);
    if (this.restoreRepositoriesOnNextSpawn) {
      try {
        await this.restorePersistedRepositories(sidecar);
      } catch (error) {
        if (sidecar !== this.sidecar || this.disposed) return undefined;
        const message =
          "Browsitory sidecar failed to restore persisted repositories: " +
          errorMessage(error);
        this.detachCurrentSidecar();
        this.sidecar = undefined;
        this.state = "failed";
        this.killSidecar(sidecar);
        this.dependencies.appendLine(message);
        this.rejectInternalRequests(message);
        this.rejectPendingAndNotify("failed", message);
        return undefined;
      }
    }

    if (sidecar !== this.sidecar || this.disposed) return undefined;
    this.restoreRepositoriesOnNextSpawn = false;
    this.state = "running";
    return sidecar;
  }

  private async restorePersistedRepositories(sidecar: SidecarProcess): Promise<void> {
    const listed = await this.callSidecar(sidecar, "list_open_repos", {});
    for (const repoPath of persistedRepositoryPaths(listed)) {
      try {
        await this.callSidecar(sidecar, "open_repo", { path: repoPath });
      } catch (error) {
        if (sidecar !== this.sidecar || this.disposed) throw error;
        this.dependencies.appendLine(
          `Browsitory: failed to reopen persisted repository ${repoPath}: ${errorMessage(error)}`,
        );
      }
    }
  }

  private callSidecar(
    sidecar: SidecarProcess,
    method: string,
    params: Record<string, unknown>,
  ): Promise<unknown> {
    const id = this.allocateInternalRequestId();
    return new Promise((resolve, reject) => {
      this.internalRequestWaiters.set(id, { resolve, reject });
      try {
        sidecar.stdin.write(JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n");
      } catch (error) {
        this.internalRequestWaiters.delete(id);
        reject(error instanceof Error ? error : new Error(errorMessage(error)));
        this.handleProcessLoss(
          sidecar,
          "Browsitory sidecar stdin write failed: " + errorMessage(error),
          true,
        );
      }
    });
  }

  private allocateInternalRequestId(): number {
    while (
      this.pendingRequestIds.has(this.nextInternalRequestId) ||
      this.internalRequestWaiters.has(this.nextInternalRequestId)
    ) {
      this.nextInternalRequestId -= 1;
    }
    return this.nextInternalRequestId--;
  }

  private attachSidecar(sidecar: SidecarProcess): void {
    const onProcessError = (error: Error) => {
      this.handleProcessLoss(
        sidecar,
        "Browsitory sidecar process error: " + errorMessage(error),
        true,
      );
    };
    const onExit = (code: number | null, signal: NodeJS.Signals | null) => {
      this.handleProcessLoss(sidecar, sidecarExitMessage(code, signal), false);
    };
    const onStdinError = (error: Error) => {
      this.handleProcessLoss(
        sidecar,
        "Browsitory sidecar stdin write failed: " + errorMessage(error),
        true,
      );
    };
    const onStderrData = (chunk: Buffer | string) => {
      const text = chunkToBuffer(chunk).toString("utf8").trimEnd();
      if (text.length > 0) this.dependencies.appendLine("vscode-sidecar stderr: " + text);
    };
    const decoder = new StringDecoder("utf8");
    const stdoutLines = new LineAccumulator((rawLine) => {
      const line = rawLine.endsWith("\r") ? rawLine.slice(0, -1) : rawLine;
      if (line.trim().length === 0) return;
      this.relaySidecarLine(line);
    });
    const onStdoutData = (chunk: Buffer | string) => {
      stdoutLines.push(decoder.write(chunkToBuffer(chunk)));
    };

    sidecar.on("error", onProcessError);
    sidecar.on("exit", onExit);
    sidecar.stdin.on("error", onStdinError);
    sidecar.stdout.on("data", onStdoutData);
    sidecar.stderr.on("data", onStderrData);
    this.detachSidecarListeners = () => {
      sidecar.off("error", onProcessError);
      sidecar.off("exit", onExit);
      sidecar.stdin.off("error", onStdinError);
      sidecar.stdout.off("data", onStdoutData);
      sidecar.stderr.off("data", onStderrData);
    };
  }

  private handleProcessLoss(
    sidecar: SidecarProcess,
    message: string,
    kill: boolean,
  ): void {
    if (sidecar !== this.sidecar || this.disposed) return;
    this.detachCurrentSidecar();
    this.sidecar = undefined;
    this.sidecarStartup = undefined;
    this.state = "reconnecting";
    this.restoreRepositoriesOnNextSpawn = true;
    if (kill) this.killSidecar(sidecar);
    this.dependencies.appendLine(message);
    this.rejectInternalRequests(message);
    this.rejectPendingAndNotify("reconnecting", message);
  }

  private detachCurrentSidecar(): void {
    const detach = this.detachSidecarListeners;
    this.detachSidecarListeners = undefined;
    detach?.();
  }

  private killSidecar(sidecar: SidecarProcess): void {
    try {
      sidecar.kill();
    } catch (error) {
      this.dependencies.appendLine(
        "Browsitory: failed to stop vscode-sidecar: " + errorMessage(error),
      );
    }
  }

  private rejectInternalRequests(message: string): void {
    const waiters = [...this.internalRequestWaiters.values()];
    this.internalRequestWaiters.clear();
    const error = new Error(message);
    for (const waiter of waiters) {
      waiter.reject(error);
    }
  }

  private rejectPendingAndNotify(
    state: "reconnecting" | "failed",
    message: string,
  ): void {
    const pendingIds = [...this.pendingRequestIds];
    this.pendingRequestIds.clear();
    for (const id of pendingIds) {
      this.postLifecycleMessage({
        jsonrpc: "2.0",
        id,
        error: { code: TRANSPORT_ERROR_CODE, message },
      });
    }
    this.postLifecycleMessage({
      jsonrpc: "2.0",
      method: "transportStatus",
      params: { state, message },
    });
  }

  private postLifecycleMessage(message: JsonRpcMessage): void {
    try {
      const posted = this.dependencies.postToWebview(message);
      if (isPromiseLike(posted)) {
        void Promise.resolve(posted).catch((error: unknown) => {
          this.dependencies.appendLine(
            "Browsitory: failed to post lifecycle message: " + errorMessage(error),
          );
        });
      }
    } catch (error) {
      this.dependencies.appendLine(
        "Browsitory: failed to post lifecycle message: " + errorMessage(error),
      );
    }
  }

  private relaySidecarLine(line: string): void {
    try {
      const message: unknown = JSON.parse(line);
      if (!isJsonRpcMessage(message)) {
        throw new Error("not a JSON-RPC response or notification");
      }
      if (typeof message["id"] === "number") {
        const internalWaiter = this.internalRequestWaiters.get(message["id"]);
        if (internalWaiter) {
          this.internalRequestWaiters.delete(message["id"]);
          const rpcError = message["error"];
          if (isRecord(rpcError) && typeof rpcError["message"] === "string") {
            internalWaiter.reject(new Error(rpcError["message"]));
          } else if (Object.prototype.hasOwnProperty.call(message, "result")) {
            internalWaiter.resolve(message["result"]);
          } else {
            internalWaiter.reject(new Error("malformed internal JSON-RPC response"));
          }
          return;
        }
        if (!this.pendingRequestIds.has(message["id"])) {
          // No internal waiter and no recorded pending webview request for this id — a stray or
          // duplicate response from the sidecar (or one that arrived after `handleProcessLoss`
          // already cleared `pendingRequestIds`). Drop it rather than forwarding it to the
          // webview as if it answered a request the webview actually made.
          return;
        }
        this.pendingRequestIds.delete(message["id"]);
      }
      this.dependencies.postToWebview(message);
    } catch (error) {
      this.dependencies.appendLine(
        "Browsitory: malformed sidecar stdout: " + errorMessage(error),
      );
    }
  }

  /** Contents of a repository file version for the `browsitory-file:` content provider. */
  async readFileVersion(ref: FileVersionRef): Promise<string> {
    const result = await this.callRunningSidecar("read_file_version", {
      repoPath: ref.repoPath,
      path: ref.path,
      version: ref.version,
    });
    if (!isRecord(result) || (typeof result["text"] !== "string" && result["text"] !== null)) {
      throw new Error("read_file_version returned an invalid result");
    }
    return result["text"] ?? "";
  }

  /** The work tree root of an open repository (its path may be a subdirectory). */
  private async workdir(repoPath: string): Promise<string> {
    const result = await this.callRunningSidecar("repo_workdir", { repoPath });
    if (typeof result !== "string" || result === "") {
      throw new Error("repo_workdir returned an invalid result");
    }
    return result;
  }

  private async callRunningSidecar(method: string, params: Record<string, unknown>): Promise<unknown> {
    if (this.disposed) throw new Error(DISPOSED_MESSAGE);
    const sidecar = await this.ensureSidecar();
    if (!sidecar) throw new Error("Browsitory sidecar is not running");
    return this.callSidecar(sidecar, method, params);
  }

  private async handleNativeRequest(request: JsonRpcRequest): Promise<void> {
    try {
      let result: unknown;
      switch (request.method) {
        case "pick_repo_folder": {
          requireEmptyParams(request);
          const selected = await this.dependencies.showOpenDialog({
            canSelectFiles: false,
            canSelectFolders: true,
            canSelectMany: false,
            openLabel: "Open Repository",
          });
          result = selected?.[0]?.fsPath ?? null;
          break;
        }
        case "open_external_url": {
          const url = requireStringParam(request, "url");
          result = await this.dependencies.openExternal(url);
          break;
        }
        case "get_app_version": {
          requireEmptyParams(request);
          const version: unknown = this.dependencies.context.extension.packageJSON["version"];
          if (typeof version !== "string") {
            throw new Error("extension manifest version is missing");
          }
          result = version;
          break;
        }
        case "get_last_seen_version": {
          requireEmptyParams(request);
          result =
            this.dependencies.context.globalState.get<string>(LAST_SEEN_VERSION_KEY) ?? null;
          break;
        }
        case "set_last_seen_version": {
          const version = requireStringParam(request, "version");
          await this.dependencies.context.globalState.update(LAST_SEEN_VERSION_KEY, version);
          result = null;
          break;
        }
        case "log_frontend_error": {
          const { context, message } = requireLogFrontendErrorParams(request);
          this.dependencies.appendLine(`${context}: ${message}`);
          result = null;
          break;
        }
        case "open_in_editor": {
          const params = requireParams(request, { repoPath: "string", path: "string", line: "number|null" });
          const rel = requireRepoPath(params["path"]);
          const line = params["line"];
          if (line !== null && (!Number.isSafeInteger(line) || (line as number) < 1)) {
            throw new InvalidParamsError("open_in_editor requires a positive line");
          }
          const file = repoFile(await this.workdir(params["repoPath"] as string), rel);
          await this.dependencies.openFile(file, line as number | null);
          result = null;
          break;
        }
        case "reveal_in_file_manager": {
          const params = requireParams(request, { repoPath: "string", path: "string|null" });
          const rel = params["path"] === null ? null : requireRepoPath(params["path"]);
          const workdir = await this.workdir(params["repoPath"] as string);
          await this.dependencies.revealInOS(rel === null ? workdir : repoFile(workdir, rel));
          result = null;
          break;
        }
        case "open_terminal": {
          const params = requireParams(request, { repoPath: "string" });
          const workdir = await this.workdir(params["repoPath"] as string);
          const name = path.basename(workdir.replace(/[\\/]+$/, "")) || workdir;
          this.dependencies.createTerminal(workdir, `Browsitory: ${name}`);
          result = null;
          break;
        }
        case "open_diff_tool": {
          const params = requireParams(request, { repoPath: "string", path: "string", source: "object" });
          const rel = requireRepoPath(params["path"]);
          const source = requireDiffSource(params["source"]);
          const repoPath = params["repoPath"] as string;
          const workdir = await this.workdir(repoPath);
          repoFile(workdir, rel);
          const sides = diffSides(repoPath, workdir, rel, source);
          await this.dependencies.openDiff(sides.left, sides.right, sides.title);
          result = null;
          break;
        }
        case "open_merge_tool": {
          const params = requireParams(request, { repoPath: "string", path: "string" });
          const rel = requireRepoPath(params["path"]);
          const repoPath = params["repoPath"] as string;
          const workdir = await this.workdir(repoPath);
          repoFile(workdir, rel);
          // The desktop merge tool's guards (a text conflict, no symlink or submodule, a work
          // tree file inside the tree): the merge editor writes its result to that file.
          await this.callRunningSidecar("check_merge_target", { repoPath, path: rel });
          await this.dependencies.openMergeEditor(mergeSides(repoPath, workdir, rel));
          // VSCode owns the merge from here; Browsitory can't see when it's done.
          result = "HandedOff";
          break;
        }
        default:
          return;
      }

      this.dependencies.postToWebview({
        jsonrpc: "2.0",
        id: request.id,
        result,
      });
    } catch (error) {
      this.dependencies.postToWebview({
        jsonrpc: "2.0",
        id: request.id,
        error: {
          code: error instanceof InvalidParamsError ? -32602 : -32000,
          message: errorMessage(error),
        },
      });
    }
  }
}

export function resolveDevelopmentSidecarPath(
  extensionRoot: string,
  platform: NodeJS.Platform = process.platform,
): string {
  return path.resolve(
    extensionRoot,
    "..",
    "target",
    "debug",
    sidecarExecutableName(platform),
  );
}

export function resolvePackagedSidecarPath(
  extensionRoot: string,
  platform: NodeJS.Platform = process.platform,
): string {
  return path.join(extensionRoot, "bin", sidecarExecutableName(platform));
}

function sidecarExecutableName(platform: NodeJS.Platform): string {
  return platform === "win32" ? "vscode-sidecar.exe" : "vscode-sidecar";
}

function isJsonRpcRequest(value: unknown): value is JsonRpcRequest {
  if (!isRecord(value) || value["jsonrpc"] !== "2.0") return false;
  return (
    typeof value["id"] === "number" &&
    Number.isSafeInteger(value["id"]) &&
    value["id"] >= 0 &&
    typeof value["method"] === "string" &&
    value["method"].length > 0 &&
    isRecord(value["params"])
  );
}

function isJsonRpcMessage(value: unknown): value is JsonRpcMessage {
  if (!isRecord(value) || value["jsonrpc"] !== "2.0") return false;
  if (
    typeof value["method"] === "string" &&
    !Object.prototype.hasOwnProperty.call(value, "id")
  ) {
    return true;
  }
  return (
    typeof value["id"] === "number" &&
    (Object.prototype.hasOwnProperty.call(value, "result") ||
      Object.prototype.hasOwnProperty.call(value, "error"))
  );
}

function requireEmptyParams(request: JsonRpcRequest): void {
  if (Object.keys(request.params).length !== 0) {
    throw new InvalidParamsError(request.method + " does not accept parameters");
  }
}

function requireStringParam(request: JsonRpcRequest, name: string): string {
  const value = request.params[name];
  const keys = Object.keys(request.params);
  if (typeof value !== "string" || keys.length !== 1 || keys[0] !== name) {
    throw new InvalidParamsError(request.method + " requires a string " + name);
  }
  return value;
}

function requireLogFrontendErrorParams(
  request: JsonRpcRequest,
): { context: string; message: string } {
  const { context, message } = request.params;
  const keys = Object.keys(request.params);
  if (
    typeof context !== "string" ||
    typeof message !== "string" ||
    keys.length !== 2 ||
    !keys.includes("context") ||
    !keys.includes("message")
  ) {
    throw new InvalidParamsError(
      request.method + " requires string context and message params",
    );
  }
  return { context, message };
}

type ParamType = "string" | "number|null" | "string|null" | "object";

/** Checks that `request.params` has exactly the keys of `shape`, each of the given type. */
function requireParams(
  request: JsonRpcRequest,
  shape: Record<string, ParamType>,
): Record<string, unknown> {
  const keys = Object.keys(request.params);
  const names = Object.keys(shape);
  const matches =
    keys.length === names.length &&
    names.every((name) => {
      const value = request.params[name];
      switch (shape[name]) {
        case "string":
          return typeof value === "string";
        case "number|null":
          return value === null || typeof value === "number";
        case "string|null":
          return value === null || typeof value === "string";
        case "object":
          return isRecord(value);
        default:
          return false;
      }
    });
  if (!matches) {
    const expected = names.map((name) => `${name}: ${shape[name]}`).join(", ");
    throw new InvalidParamsError(`${request.method} requires { ${expected} }`);
  }
  return request.params;
}

/** A work-tree-relative path that cannot escape the work tree. */
function requireRepoPath(value: unknown): string {
  if (typeof value !== "string" || !isSafeRelativePath(value)) {
    throw new InvalidParamsError("Invalid path.");
  }
  return value;
}

function repoFile(workdir: string, rel: string): string {
  try {
    return resolveRepoFile(workdir, rel);
  } catch {
    throw new InvalidParamsError("Invalid path.");
  }
}

const COMMIT_ID = /^[0-9a-fA-F]{4,64}$/;

function requireDiffSource(value: unknown): DiffSource {
  const invalid = () => new InvalidParamsError("open_diff_tool requires a valid source");
  if (!isRecord(value)) throw invalid();
  const keys = Object.keys(value).sort().join(",");
  const commitId = (v: unknown): v is string => typeof v === "string" && COMMIT_ID.test(v);
  const oldPath = (v: unknown): v is string | null =>
    v === null || (typeof v === "string" && isSafeRelativePath(v));
  switch (value["kind"]) {
    case "Unstaged":
    case "Staged":
      if (keys === "kind") return { kind: value["kind"] };
      break;
    case "Commit":
      if (keys === "commitId,kind,oldPath" && commitId(value["commitId"]) && oldPath(value["oldPath"])) {
        return { kind: "Commit", commitId: value["commitId"], oldPath: value["oldPath"] };
      }
      break;
    case "Range": {
      const toId = value["toId"];
      if (
        keys === "fromId,kind,oldPath,toId" &&
        commitId(value["fromId"]) &&
        (toId === null || commitId(toId)) &&
        oldPath(value["oldPath"])
      ) {
        return { kind: "Range", fromId: value["fromId"], toId, oldPath: value["oldPath"] };
      }
      break;
    }
  }
  throw invalid();
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function persistedRepositoryPaths(value: unknown): string[] {
  if (!isRecord(value) || !Array.isArray(value["entries"])) {
    throw new Error("list_open_repos returned an invalid result");
  }
  return value["entries"].map((entry) => {
    if (!isRecord(entry) || typeof entry["path"] !== "string") {
      throw new Error("list_open_repos returned an invalid repository entry");
    }
    return entry["path"];
  });
}

function isPromiseLike(value: unknown): value is PromiseLike<unknown> {
  return isRecord(value) && typeof value["then"] === "function";
}

function sidecarExitMessage(
  code: number | null,
  signal: NodeJS.Signals | null,
): string {
  if (code !== null) return "Browsitory sidecar exited with code " + code;
  if (signal !== null) return "Browsitory sidecar exited after signal " + signal;
  return "Browsitory sidecar exited unexpectedly";
}

function chunkToBuffer(chunk: Buffer | string): Buffer {
  return typeof chunk === "string" ? Buffer.from(chunk) : chunk;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

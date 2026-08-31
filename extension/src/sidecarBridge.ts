import * as path from "node:path";
import { StringDecoder } from "node:string_decoder";
import type { ExtensionContext, OpenDialogOptions, Uri } from "vscode";

const LAST_SEEN_VERSION_KEY = "lastSeenVersion";
const NATIVE_METHODS = new Set([
  "pick_repo_folder",
  "open_external_url",
  "get_app_version",
  "get_last_seen_version",
  "set_last_seen_version",
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
}

class InvalidParamsError extends Error {}

type SidecarBridgeState = "idle" | "running" | "reconnecting" | "failed";

const TRANSPORT_ERROR_CODE = -32001;
const DISPOSED_MESSAGE = "Browsitory sidecar bridge disposed";

export class SidecarBridge {
  private sidecar: SidecarProcess | undefined;
  private state: SidecarBridgeState = "idle";
  private detachSidecarListeners: (() => void) | undefined;
  private readonly pendingRequestIds = new Set<number>();
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

    const sidecar = this.ensureSidecar();
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
    this.detachCurrentSidecar();
    this.sidecar = undefined;
    this.state = "idle";
    if (sidecar) this.killSidecar(sidecar);
    if (this.pendingRequestIds.size > 0) {
      this.rejectPendingAndNotify("failed", DISPOSED_MESSAGE);
    }
    return Promise.resolve();
  }

  private ensureSidecar(): SidecarProcess | undefined {
    if (this.state === "running" && this.sidecar) return this.sidecar;

    try {
      const sidecar = this.dependencies.spawn(this.dependencies.executablePath);
      this.sidecar = sidecar;
      this.state = "running";
      this.attachSidecar(sidecar);
      return sidecar;
    } catch (error) {
      const message = "Browsitory sidecar failed to start: " + errorMessage(error);
      this.state = "failed";
      this.dependencies.appendLine(message);
      this.rejectPendingAndNotify("failed", message);
      return undefined;
    }
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
    let buffered = "";
    const onStdoutData = (chunk: Buffer | string) => {
      buffered += decoder.write(chunkToBuffer(chunk));
      const lines = buffered.split("\n");
      buffered = lines.pop() ?? "";
      for (const rawLine of lines) {
        const line = rawLine.endsWith("\r") ? rawLine.slice(0, -1) : rawLine;
        if (line.trim().length === 0) continue;
        this.relaySidecarLine(line);
      }
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
    this.state = "reconnecting";
    if (kill) this.killSidecar(sidecar);
    this.dependencies.appendLine(message);
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
        this.pendingRequestIds.delete(message["id"]);
      }
      this.dependencies.postToWebview(message);
    } catch (error) {
      this.dependencies.appendLine(
        "Browsitory: malformed sidecar stdout: " + errorMessage(error),
      );
    }
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

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
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

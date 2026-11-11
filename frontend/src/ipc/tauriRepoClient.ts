import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { error as logError } from "@tauri-apps/plugin-log";
import { toCloneError, toRepoError } from "./RepoClient";
import type {
  CloneOutcome,
  CloneRequest,
  FileDropEvent,
  HookProgress,
  OpenRepoEntry,
  PatchContent,
  RepoClient,
  TransferProgress,
} from "./RepoClient";
import { createRepoClient } from "./createRepoClient";

let transferListenersReady: Promise<void> = Promise.resolve();
let hookListenersReady: Promise<void> = Promise.resolve();

// Single choke point for frontend failure logging: writes into the same rotated log file as
// the Rust backend (see crates/tauri-app/src/main.rs's log plugin setup), so a bug report
// doesn't need console access to a running dev session to diagnose. Backs both this module's
// own `call` below and the `RepoClient.logFrontendError` method exposed to
// `frontend/src/lib/logger.ts`'s global `error`/`unhandledrejection` handlers.
function logFrontendError(context: string, error: unknown): Promise<void> {
  return logError(`${context}: ${String(error)}`);
}

// Every backend command failure surfaces to the UI as a rejected `invoke` promise, so this
// is the single point that can log all of them (including ones a component only shows in its
// own error state, never in the browser console) without instrumenting each RepoClient method.
// Commands answer a `RepoErrorPayload` (#496), read here into the `RepoError` every method
// rejects with. Clone and init answer their own payload and go through `loggedRawInvoke`.
function loggedInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return loggedRawInvoke<T>(command, args).catch((error: unknown) => {
    throw toRepoError(error);
  });
}

function loggedRawInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(command, args).catch((error: unknown) => {
    // A command's error payload is a plain object (`{ kind, message }`): log it as JSON.
    const logged = typeof error === "object" && error !== null && !(error instanceof Error) ? JSON.stringify(error) : error;
    void logFrontendError(`IPC ${command} failed`, logged);
    throw error;
  });
}

// The commands whose backend emits `transfer-progress`/`transfer-complete` events
// (`emit_transfer_events` in `crates/tauri-app/src/commands/remote.rs`). They wait for
// `subscribeTransferProgress`'s listeners, so a fast transfer's events are not emitted before
// anyone listens. `cancel_transfer` starts no transfer and doesn't wait.
const TRANSFER_COMMANDS: ReadonlySet<string> = new Set([
  "fetch_remote",
  "fetch_all_remotes",
  "background_fetch",
  "push_branch",
  "push_current_branch",
  "force_push_current_branch",
  "push_tags",
  "delete_remote_branch",
  "delete_remote_tag",
  "pull_current_upstream",
]);

// The commands whose backend emits `hook-progress` events (`emit_hook_events` and
// `hook_event_channel` in `crates/tauri-app/src/commands/`). They wait for
// `subscribeHookProgress`'s listener for the same reason.
const HOOK_COMMANDS: ReadonlySet<string> = new Set([
  "commit",
  "amend_commit",
  "switch_branch",
  "checkout_commit",
  "start_merge",
  "cherry_pick",
  "revert_commit",
  "cherry_pick_many",
  "revert_many",
  "continue_sequence",
]);

// Commands whose failures are expected and shown in the UI, so they are not logged: query errors
// and cancellations of a commit search happen while the user types.
const QUIET_COMMANDS: ReadonlySet<string> = new Set(["search_commits"]);

function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const send = (): Promise<T> =>
    QUIET_COMMANDS.has(command)
      ? invoke<T>(command, args).catch((error: unknown) => {
          throw toRepoError(error);
        })
      : loggedInvoke<T>(command, args);
  if (TRANSFER_COMMANDS.has(command)) return transferListenersReady.then(send);
  if (HOOK_COMMANDS.has(command)) return hookListenersReady.then(send);
  return send();
}

export const tauriRepoClient: RepoClient = {
  ...createRepoClient(call),
  listOpenRepos: () =>
    call<[OpenRepoEntry[], string | null]>("list_open_repos").then(([entries, activePath]) => ({
      entries,
      activePath,
    })),
  supportsCloneAndInit: true,
  // `loggedRawInvoke` logs only the command name and the rejection payload (`{ kind, message }`),
  // never the arguments, so the `request.https.token` is not logged.
  cloneRepo: (operationId: string, request: CloneRequest) =>
    loggedRawInvoke<CloneOutcome>("clone_repo", { operationId, request }).catch((error: unknown) => {
      throw toCloneError(error);
    }),
  cancelClone: (operationId: string) => call<void>("cancel_clone", { operationId }),
  initRepo: (path: string) =>
    loggedRawInvoke<void>("init_repo", { path }).catch((error: unknown) => {
      throw toCloneError(error);
    }),
  subscribeTransferProgress: (listener: (progress: TransferProgress) => void) => {
    let disposed = false;
    const unlisten: Array<() => void> = [];
    const registrations = ["transfer-progress", "transfer-complete"].map((event) =>
      listen<TransferProgress>(event, ({ payload }) => listener(payload)),
    );
    transferListenersReady = Promise.all(registrations).then((stops) => {
      for (const stop of stops) {
        if (disposed) stop();
        else unlisten.push(stop);
      }
    });
    void transferListenersReady.catch(() => {});
    return () => {
      if (disposed) return;
      disposed = true;
      for (const stop of unlisten) stop();
    };
  },
  subscribeHookProgress: (listener: (event: HookProgress) => void) => {
    let disposed = false;
    let stop: (() => void) | null = null;
    const registration = listen<HookProgress>("hook-progress", ({ payload }) => listener(payload));
    // The `HOOK_COMMANDS` wait for this so a fast hook's events are not emitted before
    // anyone listens (same reason as `transferListenersReady`).
    hookListenersReady = registration.then((unlisten) => {
      if (disposed) unlisten();
      else stop = unlisten;
    });
    void hookListenersReady.catch(() => {});
    return () => {
      if (disposed) return;
      disposed = true;
      stop?.();
    };
  },
  // The webview's native drag and drop events (#649): Tauri hands over the dropped paths, which
  // the page's own drop events never see. Imported when first used, so loading this module needs
  // no window.
  subscribeFileDrops: (listener: (event: FileDropEvent) => void) => {
    let disposed = false;
    let stop: (() => void) | null = null;
    import("@tauri-apps/api/webview")
      .then(({ getCurrentWebview }) =>
        getCurrentWebview().onDragDropEvent(({ payload }) => {
          if (payload.type === "drop") listener({ kind: "drop", paths: payload.paths });
          else if (payload.type === "leave") listener({ kind: "leave" });
          else listener({ kind: "over" });
        }),
      )
      .then(
        (unlisten) => {
          if (disposed) unlisten();
          else stop = unlisten;
        },
        (error: unknown) => void logFrontendError("Listening for file drops failed", error),
      );
    return () => {
      if (disposed) return;
      disposed = true;
      stop?.();
    };
  },
  // Requests from the VSCode extension host (#677); the desktop app has none.
  subscribeHostRequests: () => () => {},
  readPatchFile: (path: string) => call<PatchContent>("read_patch_file", { path }),
  logFrontendError: (context: string, error: unknown) => logFrontendError(context, error),
};

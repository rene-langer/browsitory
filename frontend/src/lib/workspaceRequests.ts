/**
 * Requests from the app to the active repository workspace (#647): the header's Commands button
 * and Help menu open the palette and the shortcut sheet, whose open state each `RepoWorkspace`
 * owns (`useWorkspaceShortcuts`), and patch files dropped on the window are applied to its
 * working tree (#649). Only the active workspace listens.
 */
export type WorkspaceRequest =
  | "palette"
  | "shortcuts"
  /** The operation log (#889), from the Help menu. */
  | "operationLog"
  | { kind: "applyPatches"; paths: string[] }
  /** A global shortcut the VSCode extension handed over (#679); `index` is Ctrl/Cmd+1…9's tab. */
  | { kind: "shortcut"; id: string; index?: number };

type Listener = (request: WorkspaceRequest) => void;

const listeners = new Set<Listener>();

/** Asks the active workspace to open `request`; does nothing while none is listening. */
export function requestWorkspace(request: WorkspaceRequest): void {
  for (const listener of [...listeners]) listener(request);
}

/** Calls `listener` for each request until the returned function is called. */
export function onWorkspaceRequest(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/**
 * A file or line history the VSCode extension asked for (#678), for the workspace of `repoPath`.
 * Kept until that workspace takes it: its tab may still be opening, and only the active
 * workspace is mounted.
 */
export interface HistoryRequest {
  repoPath: string;
  path: string;
  lines?: { startLine: number; endLine: number };
}

let pendingHistory: HistoryRequest | null = null;
const historyListeners = new Set<() => void>();

export function requestHistory(request: HistoryRequest): void {
  pendingHistory = request;
  for (const listener of [...historyListeners]) listener();
}

/** The pending history request for `repoPath`, removed; `null` when there is none. */
export function takeHistoryRequest(repoPath: string): HistoryRequest | null {
  if (pendingHistory === null || pendingHistory.repoPath !== repoPath) return null;
  const request = pendingHistory;
  pendingHistory = null;
  return request;
}

/** Calls `listener` whenever a history is requested, until the returned function is called. */
export function onHistoryRequest(listener: () => void): () => void {
  historyListeners.add(listener);
  return () => {
    historyListeners.delete(listener);
  };
}

/**
 * A commit the VSCode extension asked to select (#759), from the Timeline view, for the
 * workspace of `repoPath`. Kept until that workspace takes it, as a history request is.
 */
export interface CommitRequest {
  repoPath: string;
  commitId: string;
}

let pendingCommit: CommitRequest | null = null;
const commitListeners = new Set<() => void>();

export function requestCommit(request: CommitRequest): void {
  pendingCommit = request;
  for (const listener of [...commitListeners]) listener();
}

/** The pending commit request for `repoPath`, removed; `null` when there is none. */
export function takeCommitRequest(repoPath: string): CommitRequest | null {
  if (pendingCommit === null || pendingCommit.repoPath !== repoPath) return null;
  const request = pendingCommit;
  pendingCommit = null;
  return request;
}

/** Calls `listener` whenever a commit is requested, until the returned function is called. */
export function onCommitRequest(listener: () => void): () => void {
  commitListeners.add(listener);
  return () => {
    commitListeners.delete(listener);
  };
}

/**
 * A user command picked from a repository tab's menu (#895), for the workspace of `repoPath`:
 * the tab menu lives in `App`, outside any workspace. Kept until that workspace takes it, as a
 * history request is (the menu may have switched to its tab).
 */
export interface UserCommandRequest {
  repoPath: string;
  commandId: string;
}

let pendingUserCommand: UserCommandRequest | null = null;
const userCommandListeners = new Set<() => void>();

export function requestUserCommand(request: UserCommandRequest): void {
  pendingUserCommand = request;
  for (const listener of [...userCommandListeners]) listener();
}

/** The pending user command request for `repoPath`, removed; `null` when there is none. */
export function takeUserCommandRequest(repoPath: string): UserCommandRequest | null {
  if (pendingUserCommand === null || pendingUserCommand.repoPath !== repoPath) return null;
  const request = pendingUserCommand;
  pendingUserCommand = null;
  return request;
}

/** Calls `listener` whenever a user command is requested, until the returned function is called. */
export function onUserCommandRequest(listener: () => void): () => void {
  userCommandListeners.add(listener);
  return () => {
    userCommandListeners.delete(listener);
  };
}

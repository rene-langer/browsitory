export type StatusKind = "New" | "Modified" | "Deleted" | "Renamed" | "TypeChange" | "Conflicted";

export interface StatusEntry {
  path: string;
  staged: boolean;
  kind: StatusKind;
}

export type DiffLineOrigin = "Add" | "Remove" | "Context";

export interface DiffLine {
  origin: DiffLineOrigin;
  content: string;
}

export interface DiffOptions {
  ignoreWhitespace: boolean;
  contextLines: number;
}

export interface DiffSettings extends DiffOptions {
  syntaxHighlight: boolean;
}

export const DEFAULT_DIFF_SETTINGS: DiffSettings = { ignoreWhitespace: false, contextLines: 3, syntaxHighlight: true };
export const DEFAULT_DIFF_OPTIONS: DiffOptions = { ignoreWhitespace: false, contextLines: 3 };
export const MAX_CONTEXT_LINES = 20;

export interface DiffHunk {
  oldStart: number;
  oldLines: number;
  newStart: number;
  newLines: number;
  lines: DiffLine[];
}

export interface BranchInfo {
  name: string;
  isCurrent: boolean;
  // The backend always sends these three; optional so older fixtures stay valid.
  /** `<remote>/<branch>` from branch config, even when never fetched; null without upstream. */
  upstream?: string | null;
  /** Commits to push / to pull against the tracking ref; null when unknown (no upstream or never fetched). */
  ahead?: number | null;
  behind?: number | null;
}

export interface WorktreeInfo {
  name: string;
  path: string;
  head: string | null;
  isMain: boolean;
  isLocked: boolean;
  isPrunable: boolean;
}

export interface SubmoduleInfo {
  path: string;
  url: string | null;
  gitlinkId: string | null;
  initialized: boolean;
  headId: string | null;
}

export interface ReflogEntry {
  reference: string;
  oldId: string;
  newId: string;
  committerName: string;
  committerEmail: string;
  timestamp: number;
  message: string;
  summary: string | null;
}

export interface RemoteInfo {
  name: string;
  fetchUrl: string;
  pushUrl: string | null;
  authMode: RemoteAuthMode | null;
  authUsername: string | null;
  /** Effective git-config prune default (`remote.<name>.prune`, else `fetch.prune`). Always sent by the backend. */
  prune?: boolean;
}

export type RemoteAuthMode = "HttpsToken" | "SshAgent";

export interface UpstreamInfo {
  localBranch: string;
  remoteName: string;
  remoteBranch: string;
  // Commits ahead of / behind the remote-tracking ref; null when that ref was never fetched.
  ahead?: number | null;
  behind?: number | null;
  /**
   * Where Push lands on `remoteName` under `push.default`; null when it does not choose (e.g.
   * `simple` with a differently named upstream) and the UI must ask. Always sent by the backend;
   * see `lib/pushTarget.ts` for how a missing value is read.
   */
  pushBranch?: string | null;
}

export type OperationKind = "Merge" | "CherryPick" | "Revert";
export interface HeadInfo {
  headId: string | null;
  detachedAt: string | null;
  headPushed: boolean;
  operation: OperationKind | null;
  /** A pending squash merge's message (`SQUASH_MSG`); pre-fills the commit box. */
  squashMessage: string | null;
}
export interface UndoState {
  undo: string | null;
  redo: string | null;
}
export type UndoOutcome =
  | { kind: "Done"; label: string }
  | { kind: "Nothing" }
  | { kind: "RepositoryChanged"; label: string }
  | { kind: "OperationInProgress" };
export type PickOutcome = { kind: "Committed"; commitId: string } | { kind: "Conflicted"; files: string[] };
export type ResetMode = "soft" | "mixed" | "hard";

export type TransferOperation =
  | "Fetch"
  | "FetchAll"
  | "Pull"
  | "PushBranch"
  | "PushTags"
  | "ForcePushBranch"
  | "DeleteRemoteBranch"
  | "Clone";
/** Mirrors `git_core::remote::TransferErrorKind`; the wire value is its Rust variant name. */
export type TransferErrorKind = "NonFastForward" | "RejectedRemoteRef" | "MissingCredential" | "CredentialStoreFailure" | "SshAgentFailure" | "TransferFailed" | "Cancelled" | "StaleLease";

export interface CloneRequest {
  url: string;
  dest: string;
  branch: string | null;
  recurseSubmodules: boolean;
  https: { username: string; token: string } | null;
}

export interface CloneOutcome {
  path: string;
  submoduleFailures: string[];
  credentialSaveFailed: boolean;
}

export type CloneErrorKind =
  | "InvalidUrl"
  | "CredentialBearingUrl"
  | "DestNotEmpty"
  | "BranchNotFound"
  | "Cancelled"
  | "AlreadyRepository"
  | "Io"
  | "InitFailed"
  | TransferErrorKind;

/** Clone/init failure; the backend sends `{ kind, message }` so callers can branch on `kind`. */
export class CloneError extends Error {
  readonly kind: CloneErrorKind;
  constructor(kind: CloneErrorKind, message: string) {
    super(message);
    this.name = "CloneError";
    this.kind = kind;
  }
}

export function toCloneError(raw: unknown): CloneError {
  if (typeof raw === "object" && raw !== null && "kind" in raw && "message" in raw) {
    const { kind, message } = raw as { kind: unknown; message: unknown };
    if (typeof kind === "string" && typeof message === "string") {
      return new CloneError(kind as CloneErrorKind, message);
    }
  }
  return new CloneError("TransferFailed", String(raw));
}

export interface TransferProgress {
  operationId: string;
  operation: TransferOperation;
  phase: "Starting" | "Receiving" | "Updating" | "RemoteFailed" | "Completed" | "Failed";
  errorKind: TransferErrorKind | null;
  current: number;
  total: number;
  receivedBytes: number;
  message: string | null;
  /** Only on `RemoteFailed`: the remote that failed during a Fetch all. */
  remoteName?: string;
}

export interface TagInfo {
  name: string;
  targetId: string;
  annotated: boolean;
  message: string | null;
  taggerName: string | null;
  timestamp: number | null;
}

export interface StashEntry {
  index: number;
  message: string;
  commitId: string;
}

export interface GraphCommit {
  id: string;
  shortId: string;
  summary: string;
  authorName: string;
  authorEmail: string;
  timestamp: number;
  parentIds: string[];
  branchRefs: string[];
  remoteBranchRefs: string[];
  /** Tag names pointing (peeled) at this commit, sorted. Always sent by the backend. */
  tags?: string[];
}

/** One commit in a file's history. `path` is the file's name in that commit; `oldPath` is set
 *  when the commit renamed `oldPath` to `path`. */
export interface FileHistoryEntry {
  commit: GraphCommit;
  path: string;
  oldPath: string | null;
}

/** One page of a commit search. */
export interface SearchPage {
  commits: GraphCommit[];
  /** Resume point for the next page, or `null` once every commit has been scanned. */
  nextCursor: string | null;
  /** Commits scanned so far across all pages of this search (cumulative). */
  scanned: number;
}

export interface BlameLine {
  lineNumber: number;
  content: string;
  commitId: string;
  shortId: string;
  authorName: string;
  timestamp: number;
}

export type MergeOutcome =
  | { kind: "UpToDate" }
  | { kind: "FastForwarded" }
  | { kind: "Merged" }
  /** Squash merge: the combined changes are staged and `HeadInfo.squashMessage` is set. */
  | { kind: "Squashed" }
  | { kind: "Conflicted"; files: string[] };

/** `Allow` = fast-forward if possible, `Never` = always a merge commit, `Only` = ff or refuse. */
export type FastForwardMode = "Allow" | "Never" | "Only";
export interface MergeOptions {
  fastForward: FastForwardMode;
  /** Stage the combined changes without committing; ignores `fastForward`. */
  squash: boolean;
}

export interface StashOptions {
  /** `null` or blank → git's default "WIP on …" message. */
  message: string | null;
  keepIndex: boolean;
  includeUntracked: boolean;
  /** File paths limiting the stash; empty stashes everything. */
  paths: string[];
}
export type StashApplyOutcome = { kind: "Applied" } | { kind: "Conflicted"; files: string[] };
export interface StashFiles {
  /** Changed against the HEAD the stash was made on. */
  tracked: string[];
  /** Untracked files the stash took; diff them with `getCommitDiff(untrackedCommitId, path)`. */
  untracked: string[];
  untrackedCommitId: string | null;
}

export type PullOutcome =
  | { kind: "UpToDate" }
  | { kind: "FastForwarded"; upstreamRef: string }
  | { kind: "Diverged"; upstreamRef: string };

export type ConflictSegment =
  | { kind: "Clean"; content: string }
  | { kind: "Conflict"; ours: string; theirs: string };

export type FileConflictChoice = "Ours" | "Theirs" | "Delete";

export interface RebasePlanCommit {
  id: string;
  shortId: string;
  summary: string;
  authorName: string;
  timestamp: number;
}

export type RebaseAction =
  | { kind: "Pick" }
  | { kind: "Reword"; message: string }
  | { kind: "Edit" }
  | { kind: "Squash" }
  | { kind: "Fixup" }
  | { kind: "Drop" };

export type SignatureStatus =
  | "unsigned"
  | "good"
  | "goodUntrusted"
  | "bad"
  | "unknownKey"
  | "expired"
  | "expiredKey"
  | "revokedKey"
  | "error";

export interface CommitSignature {
  status: SignatureStatus;
  format: "openpgp" | "ssh" | null;
  signer: string | null;
  key: string | null;
  detail: string | null;
}

export interface RebasePlanEntry {
  commitId: string;
  action: RebaseAction;
  combinedMessage: string | null;
}

export type RebaseStepResult =
  | { kind: "Conflicted"; files: string[] }
  | { kind: "PausedForEdit" }
  | { kind: "Advanced" }
  | { kind: "Done" };

export type ForgeProvider = "GitHub" | "Bitbucket";

export interface ForgeRepository {
  provider: ForgeProvider;
  host: string;
  owner: string;
  name: string;
  remoteName: string;
}

export interface PullRequest {
  id: string;
  number: number;
  title: string;
  url: string;
  author: string;
  sourceBranch: string;
  targetBranch: string;
  state: string;
}

export interface CreatePullRequest {
  title: string;
  description: string | null;
  sourceBranch: string;
  targetBranch: string;
}

// GitHub defaults to 30 PRs/page, Bitbucket to 10 — `listPullRequests` asks for a larger
// explicit page (100), but a repository can still have more open PRs than that. `truncated`
// tells the UI whether the provider indicated more results exist beyond this page (a GitHub
// `Link: rel="next"` header, or a non-null Bitbucket `next` field), so it can show an explicit
// "more available" notice instead of silently displaying a partial list.
export interface PullRequestList {
  pullRequests: PullRequest[];
  truncated: boolean;
}

export interface OpenRepoEntry {
  path: string;
  workspaceId: string | null;
}

export interface Workspace {
  id: string;
  name: string;
  rootPath: string;
  memberPaths: string[];
}

export interface RepoClient {
  pickRepoFolder(): Promise<string | null>;
  listRecentRepos(): Promise<string[]>;
  getAppVersion(): Promise<string>;
  getLastSeenVersion(): Promise<string | null>;
  setLastSeenVersion(version: string): Promise<void>;
  openRepo(path: string): Promise<void>;
  closeRepo(repoPath: string): Promise<void>;
  listOpenRepos(): Promise<{ entries: OpenRepoEntry[]; activePath: string | null }>;
  persistOpenRepos(entries: OpenRepoEntry[], activePath: string | null): Promise<void>;
  scanReposInRoot(root: string): Promise<string[]>;
  /** False in the VSCode extension, where VSCode's own Git: Clone / Initialize cover this. */
  readonly supportsCloneAndInit: boolean;
  /**
   * Clones and resolves when done. `operationId` is chosen by the caller so it can
   * `cancelClone` before this resolves; progress arrives on `subscribeTransferProgress` with
   * `operation: "Clone"` (Progress phases only, no Starting/Completed). Rejects with `CloneError`.
   */
  cloneRepo(operationId: string, request: CloneRequest): Promise<CloneOutcome>;
  cancelClone(operationId: string): Promise<void>;
  /** Rejects with `CloneError` (`AlreadyRepository` when `path/.git` exists). */
  initRepo(path: string): Promise<void>;
  listWorkspaces(): Promise<Workspace[]>;
  saveWorkspace(name: string, root: string, members: string[]): Promise<string>;
  updateWorkspace(id: string, name: string, members: string[]): Promise<void>;
  deleteWorkspace(id: string): Promise<void>;
  getStatus(repoPath: string): Promise<StatusEntry[]>;
  getCommitGraph(repoPath: string, limit: number, selectedBranches: string[] | null): Promise<GraphCommit[]>;
  getGraphBranchSelection(repoPath: string): Promise<string[] | null>;
  setGraphBranchSelection(repoPath: string, selectedBranches: string[]): Promise<void>;
  getGraphRemoteBranchSelection(repoPath: string): Promise<string[] | null>;
  setGraphRemoteBranchSelection(repoPath: string, selectedBranches: string[]): Promise<void>;
  getWorkingDiff(repoPath: string, path: string, staged: boolean, opts: DiffOptions): Promise<DiffHunk[]>;
  getCommitDiff(repoPath: string, commitId: string, path: string, opts: DiffOptions): Promise<DiffHunk[]>;
  getFileHistory(repoPath: string, path: string, limit: number): Promise<FileHistoryEntry[]>;
  /**
   * One page of commits matching `query` (see the search syntax help). `refs` has the same
   * meaning as `getCommitGraph`'s `selectedBranches` (`null` = every local branch); a
   * `branch:` term overrides it. `cursor` is a previous page's `nextCursor`, or `null` for the
   * first page. Rejects with the query error text, or "Search cancelled." when a newer search
   * (or `cancelSearch`) superseded this one.
   */
  searchCommits(repoPath: string, query: string, refs: string[] | null, cursor: string | null): Promise<SearchPage>;
  /** Stops the repo's in-flight search, if any. */
  cancelSearch(repoPath: string): Promise<void>;
  getCommitRenameDiff(repoPath: string, commitId: string, oldPath: string, newPath: string, opts: DiffOptions): Promise<DiffHunk[]>;
  getCommitFiles(repoPath: string, commitId: string): Promise<string[]>;
  // Full raw commit message (subject plus body).
  getCommitMessage(repoPath: string, commitId: string): Promise<string>;
  getCommitSignature(repoPath: string, commitId: string): Promise<CommitSignature>;
  stageFile(repoPath: string, path: string): Promise<void>;
  unstageFile(repoPath: string, path: string): Promise<void>;
  stageHunk(repoPath: string, path: string, oldStart: number, newStart: number, contextLines: number): Promise<void>;
  stageLines(repoPath: string, path: string, oldStart: number, newStart: number, contextLines: number, lineIndices: number[], expectedLines: string[]): Promise<void>;
  unstageHunk(repoPath: string, path: string, oldStart: number, newStart: number, contextLines: number): Promise<void>;
  unstageLines(repoPath: string, path: string, oldStart: number, newStart: number, contextLines: number, lineIndices: number[], expectedLines: string[]): Promise<void>;
  discardHunk(repoPath: string, path: string, oldStart: number, newStart: number, contextLines: number): Promise<void>;
  discardLines(repoPath: string, path: string, oldStart: number, newStart: number, contextLines: number, lineIndices: number[], expectedLines: string[]): Promise<void>;
  discardFile(repoPath: string, path: string): Promise<void>;
  // Discards exactly `paths` (the list the user confirmed); folders are skipped and reported.
  discardAllUnstaged(repoPath: string, paths: string[]): Promise<void>;
  ignorePath(repoPath: string, pattern: string): Promise<void>;
  getDiffSettings(): Promise<DiffSettings>;
  setDiffSettings(settings: DiffSettings): Promise<void>;
  commit(repoPath: string, message: string): Promise<void>;
  listBranches(repoPath: string): Promise<BranchInfo[]>;
  createBranch(repoPath: string, name: string, startPoint: string): Promise<void>;
  switchBranch(repoPath: string, name: string): Promise<void>;
  deleteBranch(repoPath: string, name: string, force: boolean): Promise<void>;
  renameBranch(repoPath: string, oldName: string, newName: string): Promise<void>;
  listWorktrees(repoPath: string): Promise<WorktreeInfo[]>;
  createWorktree(repoPath: string, name: string, path: string, branch: string, startPoint: string | null): Promise<void>;
  removeWorktree(repoPath: string, name: string): Promise<void>;
  pruneWorktrees(repoPath: string): Promise<void>;
  listSubmodules(repoPath: string): Promise<SubmoduleInfo[]>;
  initSubmodule(repoPath: string, path: string): Promise<void>;
  updateSubmodule(repoPath: string, path: string, recursive: boolean): Promise<void>;
  listReflogRefs(repoPath: string): Promise<string[]>;
  getReflog(repoPath: string, reference: string): Promise<ReflogEntry[]>;
  restoreReflogEntry(repoPath: string, reference: string, newId: string): Promise<void>;
  listRemotes(repoPath: string): Promise<RemoteInfo[]>;
  listRemoteBranches(repoPath: string, remoteName: string): Promise<string[]>;
  getCurrentUpstream(repoPath: string): Promise<UpstreamInfo | null>;
  getRemoteUpstreams(repoPath: string, name: string): Promise<UpstreamInfo[]>;
  addRemote(repoPath: string, name: string, fetchUrl: string, pushUrl: string | null): Promise<void>;
  renameRemote(repoPath: string, oldName: string, newName: string): Promise<void>;
  updateRemoteUrls(repoPath: string, name: string, fetchUrl: string, pushUrl: string | null): Promise<void>;
  removeRemote(repoPath: string, name: string, clearUpstreams: boolean): Promise<void>;
  saveHttpsCredential(repoPath: string, remoteName: string, username: string, token: string): Promise<void>;
  forgetHttpsCredential(repoPath: string, remoteName: string): Promise<void>;
  setRemoteAuthMode(repoPath: string, remoteName: string, mode: RemoteAuthMode, username: string | null): Promise<void>;
  setCurrentUpstream(repoPath: string, remoteName: string, remoteBranch: string): Promise<void>;
  clearCurrentUpstream(repoPath: string): Promise<void>;
  listTags(repoPath: string): Promise<TagInfo[]>;
  createTag(repoPath: string, name: string, message: string | null): Promise<void>;
  deleteTag(repoPath: string, name: string): Promise<void>;
  /** `prune` `null` = git config decides (`remote.<name>.prune`, then `fetch.prune`). */
  fetchRemote(repoPath: string, remoteName: string, prune: boolean | null): Promise<string>;
  pushCurrentBranch(repoPath: string, remoteName: string): Promise<string>;
  pushTags(repoPath: string, remoteName: string, names: string[]): Promise<string>;
  /**
   * Fetches every remote in turn under one operation id. A failed remote arrives as a
   * `RemoteFailed` progress event; the operation completes unless every remote failed.
   */
  fetchAllRemotes(repoPath: string, prune: boolean | null): Promise<string>;
  /**
   * Pushes `localBranch` to `remoteBranch` on `remoteName`, and when `setUpstream` is set and the
   * push lands, records it as the branch's upstream.
   */
  pushBranch(repoPath: string, remoteName: string, localBranch: string, remoteBranch: string, setUpstream: boolean): Promise<string>;
  /** Deletes the branch on the remote and its remote-tracking ref. */
  deleteRemoteBranch(repoPath: string, remoteName: string, branchName: string): Promise<string>;
  pullCurrentUpstream(repoPath: string): Promise<PullOutcome>;
  /**
   * Asks the backend to abort the in-flight transfer `operationId`. Resolves as soon as the
   * request is recorded, not when the transfer stops: the cancellation itself arrives as that
   * operation's terminal `subscribeTransferProgress` event with `errorKind: "Cancelled"`.
   * Unknown or already-finished ids are a no-op.
   */
  cancelTransfer(repoPath: string, operationId: string): Promise<void>;
  subscribeTransferProgress(listener: (progress: TransferProgress) => void): () => void;
  listStashes(repoPath: string): Promise<StashEntry[]>;
  saveStash(repoPath: string, options: StashOptions): Promise<void>;
  /**
   * Applies `stash@{index}`; `restoreIndex` also restages what was staged. Resolves `Conflicted`
   * (instead of rejecting) when the stash applied with conflicts. `commitId` is the stash's id as
   * listed: the backend refuses ("The stash list changed; refresh and try again.") when
   * `stash@{index}` is now another stash. Same for `popStash` and `dropStash`.
   */
  applyStash(repoPath: string, index: number, commitId: string, restoreIndex: boolean): Promise<StashApplyOutcome>;
  /** Like `applyStash`, then drops the stash, but only when it applied without conflicts. */
  popStash(repoPath: string, index: number, commitId: string, restoreIndex: boolean): Promise<StashApplyOutcome>;
  /** Files of the stash whose commit is `commitId`; rejects for an id that is not a stash. */
  getStashFiles(repoPath: string, commitId: string): Promise<StashFiles>;
  dropStash(repoPath: string, index: number, commitId: string): Promise<void>;
  getBlame(repoPath: string, commitId: string, path: string): Promise<BlameLine[]>;
  /** Merges a local branch name, or a full ref (`refs/remotes/...`), into the current branch. */
  mergeBranch(repoPath: string, branchName: string, options: MergeOptions): Promise<MergeOutcome>;
  /** The merge dialog's preselection: the choice remembered for this repo, else `merge.ff`. */
  getMergeOptions(repoPath: string): Promise<MergeOptions>;
  setMergeOptions(repoPath: string, options: MergeOptions): Promise<void>;
  getConflictHunks(repoPath: string, path: string): Promise<ConflictSegment[]>;
  resolveConflict(repoPath: string, path: string, resolvedContent: string): Promise<void>;
  abortMerge(repoPath: string): Promise<void>;
  getMergeMessage(repoPath: string): Promise<string | null>;
  getHeadInfo(repoPath: string): Promise<HeadInfo>;
  amendCommit(repoPath: string, message: string): Promise<void>;
  /** Labels of the next undo/redo step for this repo; `null` when the stack is empty. */
  getUndoState(repoPath: string): Promise<UndoState>;
  /** Refusals (`Nothing`, `RepositoryChanged`, `OperationInProgress`) resolve; only real failures reject. */
  undo(repoPath: string): Promise<UndoOutcome>;
  redo(repoPath: string): Promise<UndoOutcome>;
  cherryPick(repoPath: string, commitId: string): Promise<PickOutcome>;
  revertCommit(repoPath: string, commitId: string): Promise<PickOutcome>;
  resetToCommit(repoPath: string, commitId: string, mode: ResetMode): Promise<void>;
  checkoutCommit(repoPath: string, commitId: string): Promise<void>;
  /** Resolves with the transfer operation id; progress arrives via `subscribeTransferProgress`. */
  forcePushCurrentBranch(repoPath: string, remoteName: string): Promise<string>;
  resolveAddDeleteConflict(repoPath: string, path: string, choice: FileConflictChoice): Promise<void>;
  commitsSince(repoPath: string, onto: string): Promise<RebasePlanCommit[]>;
  startRebase(repoPath: string, onto: string, plan: RebasePlanEntry[]): Promise<RebaseStepResult>;
  rebaseContinue(repoPath: string): Promise<RebaseStepResult>;
  abortRebase(repoPath: string): Promise<void>;
  getRebaseProgress(repoPath: string): Promise<{ currentStep: number; totalSteps: number } | null>;
  detectForgeRepository(repoPath: string): Promise<ForgeRepository[]>;
  saveForgeToken(repoPath: string, provider: ForgeProvider, account: string, token: string): Promise<void>;
  forgetForgeToken(repoPath: string, provider: ForgeProvider, account: string): Promise<void>;
  listPullRequests(repoPath: string, remoteName: string, account: string): Promise<PullRequestList>;
  createPullRequest(repoPath: string, remoteName: string, account: string, pullRequest: CreatePullRequest): Promise<PullRequest>;
  // Opens `url` in the user's default external browser/handler rather than navigating this
  // app's own window away from the app entirely (see `tauriRepoClient.ts` and
  // `PullRequestPanel.tsx`, the only caller).
  openExternalUrl(url: string): Promise<void>;
  // Single choke point for frontend failure logging (see `frontend/src/lib/logger.ts`'s global
  // `error`/`unhandledrejection` handlers, the only caller): each transport routes it into its
  // own durable log sink — `tauriRepoClient.ts` writes to the same rotated log file as the Rust
  // backend via `@tauri-apps/plugin-log`, `vscodeRepoClient.ts` forwards it to the extension
  // host's "Browsitory" output channel — so a bug report doesn't need console access to a
  // running session to diagnose, regardless of transport.
  logFrontendError(context: string, error: unknown): Promise<void>;
}

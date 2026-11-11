import { DEFAULT_DIFF_SETTINGS } from "./generated/wire-constants";

export type StatusKind = "New" | "Modified" | "Deleted" | "Renamed" | "TypeChange" | "Conflicted";

export interface StatusEntry {
  /** For a rename, the new path. */
  path: string;
  /** A staged rename's old path; `null` for every other entry. Unstaged moves show as a
   * `Deleted` plus a `New` entry, like `git status`. */
  oldPath: string | null;
  staged: boolean;
  kind: StatusKind;
}

export type DiffLineOrigin = "Add" | "Remove" | "Context";

export interface DiffLine {
  origin: DiffLineOrigin;
  content: string;
}

/**
 * How the two sides are lined up (#469): Myers (`Default`), `Patience` or `Minimal`. libgit2 has
 * no histogram.
 */
export type DiffAlgorithm = "Default" | "Patience" | "Minimal";
export const DIFF_ALGORITHM_VALUES: readonly DiffAlgorithm[] = ["Default", "Patience", "Minimal"];

export interface DiffOptions {
  ignoreWhitespace: boolean;
  contextLines: number;
  algorithm: DiffAlgorithm;
}

/**
 * The options a displayed hunk was read with. Hunk and line actions re-diff with exactly these to
 * find the hunk again (another context or algorithm can split, merge or move hunks), so they must
 * be the displayed diff's, never the current settings.
 */
export interface HunkOptions {
  contextLines: number;
  algorithm: DiffAlgorithm;
}

/** Diff line wrapping: always, never, or `Auto` (see `lib/wordWrap.ts`). */
export type WordWrap = "Auto" | "On" | "Off";
export const WORD_WRAP_VALUES: readonly WordWrap[] = ["Auto", "On", "Off"];

export interface DiffSettings extends DiffOptions {
  syntaxHighlight: boolean;
  wordWrap: WordWrap;
  /** Faint markers for spaces, tabs and carriage returns in diff lines (#616); display only. */
  showWhitespace: boolean;
  /** Untracked files load at once in the uncommitted changes (#623); off, only when asked for (#639). */
  expandUntracked: boolean;
}

// The backend's limits and defaults (#497), generated from the Rust constants:
// `DEFAULT_DIFF_SETTINGS`, `MAX_CONTEXT_LINES` and `WHOLE_FILE_CONTEXT_LINES`, which is
// `DiffOptions.contextLines` for "Show whole file" (#444): the backend then returns every line of
// the file in one hunk. Any other value above `MAX_CONTEXT_LINES` is clamped to it. Hunk and line
// actions on such a diff pass the same value, as they do for any other context.
export {
  CODE_FONT_SIZES,
  COMMIT_MESSAGE_COLUMNS,
  DEFAULT_APPEARANCE_SETTINGS,
  DEFAULT_COMMIT_MESSAGE_SETTINGS,
  DEFAULT_DIFF_SETTINGS,
  DEFAULT_FETCH_SETTINGS,
  DEFAULT_UPDATE_SETTINGS,
  MAX_CONTEXT_LINES,
  SEARCH_KEYS,
  TAB_SIZES,
  WHOLE_FILE_CONTEXT_LINES,
} from "./generated/wire-constants";
export const DEFAULT_DIFF_OPTIONS: DiffOptions = {
  ignoreWhitespace: DEFAULT_DIFF_SETTINGS.ignoreWhitespace,
  contextLines: DEFAULT_DIFF_SETTINGS.contextLines,
  algorithm: DEFAULT_DIFF_SETTINGS.algorithm,
};
/** A whole-file diff longer than this many lines falls back to the normal context. */
export const WHOLE_FILE_LINE_LIMIT = 10000;

/** Commit message editor settings (`[commit_message]`); columns are clamped to `COMMIT_MESSAGE_COLUMNS`. */
export interface CommitMessageSettings {
  summaryLimit: number;
  bodyRuler: number;
  showRuler: boolean;
}

/** Fetch settings (`[fetch]`): the app-wide "Fetch automatically" interval, 0 for off. */
export interface FetchSettings {
  autoFetchMinutes: number;
}

/** The interface text scale (`--text-*`, see `styles/tokens.css`). */
export type UiSize = "Small" | "Normal" | "Large";
export const UI_SIZE_VALUES: readonly UiSize[] = ["Small", "Normal", "Large"];

/**
 * Appearance settings (`[appearance]`, #615): `codeFont` and `codeFontSize` are `null` for the
 * built-in look; sizes are clamped to `CODE_FONT_SIZES` and `TAB_SIZES`. Applied by
 * `lib/appearance.ts`.
 */
export interface AppearanceSettings {
  codeFont: string | null;
  codeFontSize: number | null;
  uiSize: UiSize;
  tabSize: number;
}

/**
 * Update settings (`[updates]`, desktop app only, #630): `checkAutomatically` checks for a new
 * version at start and every few hours; off, only "Check for updates" checks.
 */
export interface UpdateSettings {
  checkAutomatically: boolean;
}

/** A `Key: value` line for a commit message's trailer block, e.g. `Co-authored-by`. */
export interface Trailer {
  key: string;
  value: string;
}
/** A person: an amend author override, or one of the recent authors. */
export interface SignatureInput {
  name: string;
  email: string;
}
/** Amend extras: `author: null` keeps HEAD's author. */
export interface AmendOptions {
  author: SignatureInput | null;
  trailers: Trailer[];
}

/**
 * One side of a file for the image preview. `Loaded.base64` is the bytes; `mime` is the image
 * type sniffed from them by the backend (`null`: not a PNG, JPEG, GIF, WebP or SVG).
 */
export type FileSide =
  | { kind: "Missing" }
  | { kind: "TooLarge"; size: number }
  | { kind: "Loaded"; size: number; base64: string; mime: string | null };
export interface FileSides {
  old: FileSide;
  new: FileSide;
}

/** What a commit's tree entry is (#446). `Executable`: a file with the executable bit. */
export type TreeEntryKind = "Folder" | "File" | "Executable" | "Symlink" | "Submodule";
/** One entry of a commit's tree; `path` is repository-relative, `size` `null` for folders and submodules. */
export interface TreeEntry {
  path: string;
  name: string;
  kind: TreeEntryKind;
  size: number | null;
}
/** One folder of a commit's tree, or a filter's matches (`truncated`: there were more). */
export interface TreeListing {
  entries: TreeEntry[];
  truncated: boolean;
}
/**
 * A file's content at a commit for the read-only viewer (#446). Files over 2 MB are `TooLarge`;
 * a binary file's bytes come only when they are a previewable image (`Image`).
 */
export type FileContent =
  | { kind: "Text"; size: number; text: string }
  | { kind: "Image"; size: number; base64: string; mime: string }
  | { kind: "Binary"; size: number }
  | { kind: "TooLarge"; size: number };

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
  /** The checked-out submodule's folder, which "Open in new tab" opens; `null` until it is checked out (#445). */
  workdir: string | null;
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
  /**
   * `operation`'s changes are already in HEAD; only git's state files are left (#240). Abort, the
   * next commit or an amend clears them. The commit box isn't pre-filled with the old message,
   * and amend is available.
   */
  operationLeftover: boolean;
  /** A pending squash merge's message (`SQUASH_MSG`); pre-fills the commit box. */
  squashMessage: string | null;
  /** Installed commit hooks that `noVerify` skips (`pre-commit`, `commit-msg`). */
  commitHooks: string[];
  /** A stopped multi-commit cherry-pick/revert (stored, so it survives a restart). */
  sequence: SequenceProgress | null;
  /** A paused interactive rebase (stored, so it survives a restart). */
  rebase: RebaseInfo | null;
  /**
   * A `git bisect` started in a terminal. Nothing is blocked during it (git allows picks,
   * rebases, reset and commits while bisecting); independent of `operation`, so a merge paused
   * during a bisect has both.
   */
  bisect: BisectInfo | null;
  /**
   * A `git cherry-pick A B C` / `git revert A B` started in a terminal and not finished (#258):
   * paused on a step's conflict (`operation` is then set too) or between steps. Cherry-pick,
   * revert and rebase are refused until it ends; `abortSequence` aborts it.
   */
  terminalSequence: TerminalSequence | null;
  /**
   * A `git rebase` or `git am` started in a terminal and not finished (#263). It is continued or
   * aborted there: Abort merge, picks, reverts, a new rebase, reset and amend are refused until it
   * ends. `operation` stays `null` even when git wrote `MERGE_MSG` for a conflicted step.
   */
  terminalRebase: TerminalRebase | null;
}
/**
 * A terminal rebase (`kind: "Rebase"`) or `git am` (`"Am"`). `branch` is the rebased branch's short
 * name and `onto` the full id it is rebased onto, both empty when unknown (always for `git am`);
 * `currentStep` is 1-based, both steps 0 when unknown.
 */
export interface TerminalRebase {
  kind: "Rebase" | "Am";
  branch: string;
  onto: string;
  currentStep: number;
  totalSteps: number;
}
/**
 * A terminal sequence. `startedAt` is where it started (full id, `startedAtSummary` its summary).
 * `rewinds`: Abort puts the branch back at `startedAt`, removing `commitsToRemove` commits (as
 * `git cherry-pick --abort`); otherwise HEAD moved since and Abort leaves it.
 */
export interface TerminalSequence {
  kind: SequenceKind;
  pausedOnConflict: boolean;
  startedAt: string;
  startedAtSummary: string;
  rewinds: boolean;
  commitsToRemove: number;
}
/**
 * What the user confirmed before aborting a terminal sequence: `HeadInfo.headId` and the
 * `terminalSequence` fields shown (#281 review).
 */
export interface ConfirmedAbort {
  headId: string;
  startedAt: string;
  rewinds: boolean;
  commitsToRemove: number;
  /** A step was paused on a conflict: the dialog warned that its changes and resolutions go. */
  pausedOnConflict: boolean;
}
/** `startedFrom` is what `git bisect reset` returns to: a branch name, or a commit id. */
export interface BisectInfo {
  startedFrom: string;
}
/** What changed outside Browsitory while a rebase was paused; the rebase can only be aborted. */
export type RebaseDivergence = "HeadMoved" | "BranchMoved" | "BranchMissing" | "OtherOperation";
export type RebaseHealth = { kind: "Paused" } | { kind: "Diverged"; reason: RebaseDivergence };
/** `branch` is the short name; `onto` the full id; `currentStep` is 1-based. */
export interface RebaseInfo {
  branch: string;
  onto: string;
  currentStep: number;
  totalSteps: number;
  health: RebaseHealth;
}
/** `restored: false`: the rebase no longer owned HEAD, so HEAD was left where it is. */
export interface RebaseAbort {
  restored: boolean;
}
export type SequenceKind = "CherryPick" | "Revert";
/** `currentStep` is 1-based. */
export interface SequenceProgress {
  kind: SequenceKind;
  currentStep: number;
  totalSteps: number;
  currentCommit: string;
}
/** `skipped`: commits whose changes were already on the branch (their step came out empty). */
export type SequenceOutcome =
  | { kind: "Completed"; applied: number; skipped: string[] }
  | { kind: "Conflicted"; files: string[]; progress: SequenceProgress }
  /**
   * A commit hook failed (Continue's, or a clean step's `prepare-commit-msg`): nothing was
   * committed for that step, which is paused.
   */
  | { kind: "HookFailed"; hook: string; output: string; progress: SequenceProgress }
  /**
   * Cancel was pressed while a step's hooks ran after its commit: that commit (`kept`, its
   * summary) stands and the sequence stopped before `progress.currentCommit`; Continue resumes.
   */
  | { kind: "Stopped"; kept: string; progress: SequenceProgress };
/** `rewound: false`: HEAD had moved since the sequence stopped, so it was left where it is. */
export interface SequenceAbort {
  rewound: boolean;
}
/** Which side's whole file to keep: "Ours" = HEAD (stage 2), "Theirs" = the incoming commit. */
export type ConflictSide = "Ours" | "Theirs";
export interface UndoState {
  undo: string | null;
  redo: string | null;
}
export type UndoOutcome =
  | { kind: "Done"; label: string }
  | { kind: "Nothing" }
  | { kind: "RepositoryChanged"; label: string }
  /** A branch to move, delete or check out is held by a worktree (#291); `message` says how. */
  | { kind: "BranchHeld"; label: string; message: string }
  | { kind: "OperationInProgress" };
/**
 * A single cherry-pick/revert. `HookFailed`: its `prepare-commit-msg` failed (#183); nothing was
 * committed and the pick stays staged and in progress, for the commit box or Abort.
 */
export type PickOutcome =
  | { kind: "Committed"; commitId: string }
  | { kind: "Conflicted"; files: string[] }
  | { kind: "HookFailed"; hook: string; output: string };
/** A commit/amend result: a failing gate hook resolves `HookFailed` with the hook's output. */
export type CommitOutcome =
  | { kind: "Committed"; commitId: string }
  | { kind: "HookFailed"; hook: string; output: string };
/**
 * The kind of command a hook runs for (#185): `Commit` (commit, amend, a sequence's Continue)
 * shows in the commit box and sequence panel; the others (`post-checkout` after a switch or
 * checkout, `post-merge` after a fast-forward or squash, a cherry-pick's or revert's hooks) in
 * the "Hook status" strip.
 */
export type HookSource = "Commit" | "Checkout" | "Merge" | "Pick";
/** A hook starting or finishing; `operationId` is accepted by `cancelTransfer`. */
export interface HookProgress {
  repoPath: string;
  operationId: string;
  hook: string;
  phase: "Started" | "Finished";
  source: HookSource;
}
export type ResetMode = "soft" | "mixed" | "hard";

export type TransferOperation =
  | "Fetch"
  | "FetchAll"
  | "Pull"
  | "PushBranch"
  | "PushTags"
  | "ForcePushBranch"
  | "DeleteRemoteBranch"
  | "DeleteRemoteTag"
  | "Clone"
  /** The quiet Fetch all of the "Fetch automatically" setting (`backgroundFetch`). */
  | "BackgroundFetch";
/** Mirrors `git_core::remote::TransferErrorKind`; the wire value is its Rust variant name. */
export type TransferErrorKind = "NonFastForward" | "RejectedRemoteRef" | "MissingCredential" | "CredentialStoreFailure" | "SshAgentFailure" | "TransferFailed" | "Cancelled" | "StaleLease" | "HookFailed";

/**
 * Mirrors `repo_service::wire::ErrorKind` (#496): what kind of failure a rejected request was,
 * for the few the UI tells apart. Code switches on it, never on the message text.
 */
export type ErrorKind =
  | "Other"
  | "WorkerStopped"
  | "CredentialStoreFailure"
  | "DirtyWorktree"
  | "PullFailed"
  | "Cancelled"
  | "AddDeleteConflict"
  | "InvalidSearchQuery"
  | "PermissionDenied"
  | "LocalChanges"
  | "MissingIdentity";

/** A rejected request as the backend sends it (`repo_service::wire::ErrorDto`). */
export interface RepoErrorPayload {
  kind: ErrorKind;
  message: string;
}

/**
 * `ErrorKind`, plus `TransportLost`: the VSCode extension host lost its sidecar process, so the
 * request never got an answer (JSON-RPC code `-32001`, or a `transportStatus` notification).
 */
export type RepoErrorKind = ErrorKind | "TransportLost";

/**
 * What every `RepoClient` method rejects with when the backend refuses a request (#496).
 * `String(error)` is the backend's message alone, as when Tauri rejected with a bare string.
 */
export class RepoError extends Error {
  readonly kind: RepoErrorKind;
  constructor(kind: RepoErrorKind, message: string) {
    super(message);
    this.name = "RepoError";
    this.kind = kind;
  }
  override toString(): string {
    return this.message;
  }
}

/** The kind of a caught value: a `RepoError`'s, else `"Other"`. */
export function errorKind(error: unknown): RepoErrorKind {
  return error instanceof RepoError ? error.kind : "Other";
}

const ERROR_KINDS: ReadonlySet<string> = new Set<ErrorKind>([
  "Other",
  "WorkerStopped",
  "CredentialStoreFailure",
  "DirtyWorktree",
  "PullFailed",
  "Cancelled",
  "AddDeleteConflict",
  "InvalidSearchQuery",
  "PermissionDenied",
  "LocalChanges",
  "MissingIdentity",
]);

/**
 * Reads a backend rejection: a `RepoErrorPayload`, or a bare string (an error Tauri raises
 * itself, such as a bad argument). A kind this frontend doesn't know reads as `"Other"`.
 *
 * The bare strings stay `"Other"` here (#527): Tauri raises them before any command runs (an
 * argument its generated wrapper can't deserialise, an unknown command, an ACL refusal) and
 * sends them through its own `InvokeResolver`, whose responder an app can't wrap, so the
 * backend can't put them in the envelope. They mean the frontend and backend disagree about a
 * command, which no kind-specific remediation fixes.
 */
export function toRepoError(raw: unknown): RepoError {
  if (raw instanceof RepoError) return raw;
  if (typeof raw === "object" && raw !== null && "kind" in raw && "message" in raw) {
    const { kind, message } = raw as { kind: unknown; message: unknown };
    if (typeof message === "string") {
      return new RepoError(typeof kind === "string" && ERROR_KINDS.has(kind) ? (kind as ErrorKind) : "Other", message);
    }
  }
  return new RepoError("Other", raw instanceof Error ? raw.message : String(raw));
}

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
  | "CaseCollision"
  | "CasePair"
  | "CaseConfig"
  | "FileInUse"
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
  /**
   * `RunningHook`: a push's `pre-push` hook is running (no transfer progress yet), or a pull's
   * `post-merge` after it fast-forwarded (#185).
   */
  phase: "Starting" | "Receiving" | "Updating" | "RunningHook" | "RemoteFailed" | "Completed" | "Failed";
  errorKind: TransferErrorKind | null;
  current: number;
  total: number;
  receivedBytes: number;
  message: string | null;
  /** Only on `RemoteFailed`: the remote that failed during a Fetch all. */
  remoteName?: string;
  /**
   * The failed `pre-push` hook's output when `errorKind` is `HookFailed`; `null` otherwise.
   * Unlike `message` this comes from the user's own script, so it is forwarded.
   */
  hookOutput: string | null;
}

export interface TagInfo {
  name: string;
  /** What the tag points at directly: another tag object for a tag of a tag. */
  targetId: string;
  /** The commit the tag peels to; `null` for a tag of a tree or blob. */
  commitId: string | null;
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
  /**
   * Only on a merge row of a graph that follows first parents ("Collapse merges" or one selected
   * branch, #617): how many commits its other parents brought in, which `getMergedCommits` lists.
   */
  mergedCount?: number;
}

/** One commit in a file's history. `path` is the file's name in that commit; `oldPath` is set
 *  when the commit renamed `oldPath` to `path`. */
export interface FileHistoryEntry {
  commit: GraphCommit;
  path: string;
  oldPath: string | null;
}

/** One commit that changed a range of lines (#447). `path`/`oldPath` as in `FileHistoryEntry`;
 *  `startLine`..`endLine` (1-based, inclusive) are the followed lines in this commit's version. */
export interface LineHistoryEntry {
  commit: GraphCommit;
  path: string;
  oldPath: string | null;
  startLine: number;
  endLine: number;
}

/** `getLineHistory`'s answer. `truncated`: the walk stopped at its commit cap, so older changes
 *  may be missing. */
export interface LineHistory {
  entries: LineHistoryEntry[];
  truncated: boolean;
}

/** One changed path in a compare; `oldPath` is set when the file was renamed (then `path` is the new name). */
export interface ChangedFile {
  path: string;
  oldPath: string | null;
}

/**
 * The branches the commit graph walks, passed as one `selection` to `getCommitGraph`,
 * `locateCommit` and `searchCommits` (#498). `local`: `null` = every local branch, `[]` = none.
 * `remote`: `<remote>/<branch>` names walked on top; `[]` = none (the default graph).
 */
export interface GraphSelection {
  local: string[] | null;
  remote: string[];
  /**
   * "Collapse merges" (#617): the graph and `locateCommit` follow first parents from every
   * selected tip. `searchCommits` ignores it. Absent is `false`.
   */
  firstParent?: boolean;
}

/** The collapsed merge that holds a "Go to" target (#617). */
export interface CollapsedMerge {
  mergeId: string;
  /** The merge's row in the graph, as `CommitLocation.position`. */
  mergePosition: number;
  /** The target's index in `getMergedCommits` of that merge: a limit above it lists it. */
  offset: number;
}

/** Where a revision's commit sits in the graph ("Go to commit", #441). */
export interface CommitLocation {
  commitId: string;
  /** Its row in the graph of the selection (0 = the first commit), or `null` when that graph
   *  never reaches it however much history is loaded. */
  position: number | null;
  /** Only when `position` is `null`: local branches that contain the commit, sorted. A branch
   *  already shown appears when the graph follows its first parents only. */
  branches: string[];
  /** Like `branches`, remote-tracking branches as `<remote>/<branch>`. */
  remoteBranches: string[];
  /** Only when `position` is `null` in a graph that follows first parents: the merge row that
   *  holds the commit, which expanding shows it (#617). */
  merge?: CollapsedMerge;
}

/** One file's line counts in a commit (#620). */
export interface FileStats {
  path: string;
  insertions: number;
  deletions: number;
  /** Binary content: counted as a changed file, with no lines. */
  binary: boolean;
}

/**
 * A commit's line counts against its first parent (a root commit against the empty tree), with
 * the diff the commit's file list uses: no rename detection, so a rename is a removed and an
 * added file (#620).
 */
export interface CommitStats {
  filesChanged: number;
  insertions: number;
  deletions: number;
  /** Each changed file, in diff order; empty unless `withFiles` asked for them. */
  files: FileStats[];
}

/** One answered commit of `getCommitStats`. */
export interface CommitStatsEntry {
  commitId: string;
  /** `null`: the commit is over the size or time cap (or couldn't be read); it gets no numbers. */
  stats: CommitStats | null;
}

/** A commit's author or committer, with the Unix-seconds time git recorded for them (#641). */
export interface CommitPerson {
  name: string;
  email: string;
  timestamp: number;
}

/**
 * What the commit details show beyond the graph row (#641): the author and the committer with
 * their own times, and the branches and tags whose tip is or contains the commit, sorted.
 */
export interface CommitDetails {
  author: CommitPerson;
  committer: CommitPerson;
  branches: string[];
  /** As `<remote>/<branch>`; symbolic refs such as `origin/HEAD` are left out. */
  remoteBranches: string[];
  tags: string[];
  /** `false`: the backend ran out of time before checking every ref, so the lists may be short. */
  complete: boolean;
}

/** Resolved ends of a branch compare: the merge base with HEAD, and the branch tip. */
export interface CompareRange {
  fromId: string;
  toId: string;
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

/** How a diverged pull reconciles (#652). */
export type PullStrategy = "Merge" | "Rebase" | "FastForwardOnly";

/** What "Stash and switch" did with the stashed changes on the new branch (#653). */
export type StashSwitchOutcome =
  | { kind: "Stashed" }
  | { kind: "Reapplied" }
  | { kind: "Conflicted"; files: string[] }
  | { kind: "NotReapplied"; reason: string };

export type PullOutcome =
  | { kind: "UpToDate" }
  | { kind: "FastForwarded"; upstreamRef: string }
  | { kind: "Diverged"; upstreamRef: string };

export type ConflictSegment =
  | { kind: "Clean"; content: string }
  | { kind: "Conflict"; ours: string; base: string; theirs: string };

export type FileConflictChoice = "Ours" | "Theirs" | "Delete";

/**
 * Which two versions of a file a diff tool compares. `Range.toId === null` compares `fromId`
 * with the working tree (the compare view's "with the working tree" mode).
 */
export type DiffSource =
  | { kind: "Unstaged" }
  | { kind: "Staged"; oldPath: string | null }
  | { kind: "Commit"; commitId: string; oldPath: string | null }
  | { kind: "Range"; fromId: string; toId: string | null; oldPath: string | null };
/** A hunk of a commit's diff as the commit view showed it (#448), whitespace significant. */
export interface CommitHunk extends HunkOptions {
  oldStart: number;
  newStart: number;
}
/** A commit's file, or one hunk of it, to revert or apply in the working tree (#448). */
export interface CommitChangeRequest {
  commitId: string;
  path: string;
  /** The rename's old path, when the commit view paired one. */
  oldPath: string | null;
  /** `null`: the whole file. */
  hunk: CommitHunk | null;
  /** Revert the change rather than apply it. */
  reverse: boolean;
  /** Also stage the result (the touched files must have no unstaged changes). */
  stage: boolean;
}
/** What "Save as patch…" writes (#448). `commitIds` oldest first. */
export type PatchSource =
  | { kind: "Commits"; commitIds: string[] }
  | { kind: "CommitFile"; commitId: string; path: string; oldPath: string | null }
  | { kind: "Staged" };
/** A patch's bytes, base64 (a patch may hold file content in any encoding). */
export interface PatchContent {
  base64: string;
}
/**
 * How "Apply patch…" applies a patch file (#658): to the working tree, to the working tree and
 * the index, or each mail as a commit with its author, date and message.
 */
export type PatchApply = "WorkTree" | "Stage" | "Commits";
/** What a patch file holds and whether it applies (#658). */
export interface PatchPreview {
  /** One per mail, in order (one for a plain diff). */
  parts: PatchPreviewPart[];
  /** Why it doesn't apply to the working tree; only the first mail is checked against it. */
  problem: string | null;
}
export interface PatchPreviewPart {
  /** `null` for a plain diff. */
  mail: PatchMail | null;
  files: PatchPreviewFile[];
}
export interface PatchMail {
  subject: string;
  body: string;
  authorName: string;
  authorEmail: string;
  /** Seconds since the epoch; `null` when the mail's date couldn't be read. */
  timestamp: number | null;
}
export type PatchFileStatus = "Added" | "Deleted" | "Modified" | "Renamed" | "Copied";
export interface PatchPreviewFile {
  path: string;
  oldPath: string | null;
  status: PatchFileStatus;
  additions: number;
  deletions: number;
  binary: boolean;
}
/** `HandedOff`: VSCode's merge editor took over, and Browsitory can't see when it's done. */
export type MergeToolOutcome = "Resolved" | "Unresolved" | "HandedOff";
export type MarkResolvedOutcome = "Resolved" | "MarkersRemain";
/** Editor/terminal command lines; `null` means the system default. */
export interface ExternalToolSettings {
  editor: string | null;
  terminal: string | null;
}
export type GitConfigSource = "System" | "Xdg" | "Global" | "Local" | "Worktree" | "Other";
export interface GitToolInfo {
  name: string;
  command: string | null;
  source: GitConfigSource;
  viaMergeTool: boolean;
  /** Why the tool can't be used (e.g. a terminal tool, or set only in repository config). */
  problem: string | null;
}
export interface GitTools {
  diffTool: GitToolInfo | null;
  mergeTool: GitToolInfo | null;
}

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

/** A one-click history edit from the commit menu (#442). */
export type HistoryEditKind = "Reword" | "Squash" | "Fixup" | "MoveUp" | "MoveDown" | "Drop";

/**
 * `startRebase`'s `edit`: the one-click history edit the rebase runs, so its Undo entry names it
 * ("Drop 1a2b3c4") instead of "Rebase onto <id>" (#473). It only changes the label.
 */
export interface HistoryEditTarget {
  kind: HistoryEditKind;
  commitId: string;
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

/** A repository's custom tab name (#625), kept by path in the app config. */
export interface RepoAlias {
  repoPath: string;
  alias: string;
}

export interface Workspace {
  id: string;
  name: string;
  rootPath: string;
  memberPaths: string[];
}

/** The work tree root files the tab menu and palette open in the editor (#624), in menu order. */
export const ROOT_FILE_NAMES = [".gitignore", ".gitattributes"] as const;
export type RootFileName = (typeof ROOT_FILE_NAMES)[number];

/**
 * Files or folders dragged from the file manager onto the window (#649): `over` while they hover
 * (repeated), `leave` when they go without a drop, `drop` with their absolute paths.
 */
export type FileDropEvent = { kind: "over" } | { kind: "leave" } | { kind: "drop"; paths: string[] };

/**
 * What the VSCode extension host asks the app to do (#677, #678, #679); `extension/src/
 * hostRequests.ts` holds the same type.
 */
export type HostRequest =
  /** Opens each repository as a tab (or keeps its tab) and shows the first one. */
  | { kind: "openRepositories"; paths: string[] }
  /**
   * Opens `repoPath`'s tab and shows the history of `path` (relative to the work tree, with "/"),
   * or of its lines `startLine`..`endLine` as they are in HEAD.
   */
  | { kind: "showHistory"; repoPath: string; path: string; lines?: { startLine: number; endLine: number } }
  /** A global shortcut (`ShortcutId`) VSCode would otherwise take; `index` is Ctrl/Cmd+1…9's tab. */
  | { kind: "shortcut"; id: string; index?: number };

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
  /** Every stored tab name (#625), also of repositories that aren't open. */
  listRepoAliases(): Promise<RepoAlias[]>;
  /** Stores `alias` (trimmed) as the tab name of `repoPath`; `null` or a blank one removes it. */
  setRepoAlias(repoPath: string, alias: string | null): Promise<void>;
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
  /**
   * The commits of `selection.local` plus those of `selection.remote`. Exactly one branch in
   * total, or `selection.firstParent`, walks first parents only; its merge rows then carry
   * `mergedCount`. Missing branches are skipped.
   */
  getCommitGraph(repoPath: string, limit: number, selection: GraphSelection): Promise<GraphCommit[]>;
  /**
   * The commits merge `mergeId` brought in (reachable from its other parents, not from its first
   * parent), children first, at most `limit` (#617): what expanding a collapsed merge shows.
   */
  getMergedCommits(repoPath: string, mergeId: string, limit: number): Promise<GraphCommit[]>;
  /** Whether this repository's graph collapses merges (#617); `false` until set. */
  getGraphCollapseMerges(repoPath: string): Promise<boolean>;
  setGraphCollapseMerges(repoPath: string, collapse: boolean): Promise<void>;
  /**
   * Resolves `revision` (SHA or prefix, branch, tag, any git revision) and finds its commit in
   * the graph `getCommitGraph` returns for the same selection. `null` when it names no commit;
   * rejects for an ambiguous SHA prefix.
   */
  locateCommit(repoPath: string, revision: string, selection: GraphSelection): Promise<CommitLocation | null>;
  getGraphBranchSelection(repoPath: string): Promise<string[] | null>;
  setGraphBranchSelection(repoPath: string, local: string[]): Promise<void>;
  /** Forgets the saved selection: back to every local branch, also ones created later (#402). */
  clearGraphBranchSelection(repoPath: string): Promise<void>;
  getGraphRemoteBranchSelection(repoPath: string): Promise<string[] | null>;
  setGraphRemoteBranchSelection(repoPath: string, remote: string[]): Promise<void>;
  /** Forgets the saved remote selection: no remote commits, every remote badge (#402). */
  clearGraphRemoteBranchSelection(repoPath: string): Promise<void>;
  /** `oldPath` (with `staged`): a staged rename's old path, so only its edits show. */
  getWorkingDiff(repoPath: string, path: string, staged: boolean, opts: DiffOptions, oldPath?: string | null): Promise<DiffHunk[]>;
  getCommitDiff(repoPath: string, commitId: string, path: string, opts: DiffOptions): Promise<DiffHunk[]>;
  getFileHistory(repoPath: string, path: string, limit: number): Promise<FileHistoryEntry[]>;
  /**
   * The commits that changed lines `startLine`..`endLine` (1-based, inclusive) of `path` as it is
   * in `revision` (a commit id or "HEAD"), newest first, at most `limit`, following the lines
   * back through earlier changes (#447). Rejects when the range isn't in the file.
   */
  getLineHistory(
    repoPath: string,
    revision: string,
    path: string,
    startLine: number,
    endLine: number,
    limit: number,
  ): Promise<LineHistory>;
  /**
   * One page of commits matching `query` (see the search syntax help). `selection` is the one
   * `getCommitGraph` takes, so search walks the graph's commits; a `branch:` term overrides it.
   * `cursor` is a previous page's `nextCursor`, or `null` for the first page. Rejects with the query error text, or "Search cancelled." when a newer search
   * (or `cancelSearch`) superseded this one.
   */
  searchCommits(
    repoPath: string,
    query: string,
    selection: GraphSelection,
    cursor: string | null,
  ): Promise<SearchPage>;
  /** Stops the repo's in-flight search, if any. */
  cancelSearch(repoPath: string): Promise<void>;
  getCommitRenameDiff(repoPath: string, commitId: string, oldPath: string, newPath: string, opts: DiffOptions): Promise<DiffHunk[]>;
  getCommitFiles(repoPath: string, commitId: string): Promise<string[]>;
  /** Author, committer and the branches and tags that contain `commitId` (#641). */
  getCommitDetails(repoPath: string, commitId: string): Promise<CommitDetails>;
  /**
   * Line counts of `commitIds` (#620), in order. The backend stops starting commits after a short
   * time budget, so the answer can leave out the commits at the end: ask for those again.
   */
  getCommitStats(repoPath: string, commitIds: string[], withFiles: boolean): Promise<CommitStatsEntry[]>;
  /** Files that differ between two commits' trees (`git diff from to`), renames detected. */
  getRangeFiles(repoPath: string, fromId: string, toId: string): Promise<ChangedFile[]>;
  /** One file's diff between two commits; pass `oldPath` from `getRangeFiles` for a rename. */
  getRangeDiff(repoPath: string, fromId: string, toId: string, path: string, oldPath: string | null, opts: DiffOptions): Promise<DiffHunk[]>;
  /** Files that differ between a commit and the working tree (staged, unstaged and untracked). */
  getWorkingTreeCompareFiles(repoPath: string, commitId: string): Promise<ChangedFile[]>;
  getWorkingTreeCompareDiff(repoPath: string, commitId: string, path: string, oldPath: string | null, opts: DiffOptions): Promise<DiffHunk[]>;
  /** `merge-base(HEAD, refName)..refName` for a full ref name (`refs/heads/…`, `refs/remotes/…`). */
  getBranchCompareRange(repoPath: string, refName: string): Promise<CompareRange>;
  // Full raw commit message (subject plus body).
  getCommitMessage(repoPath: string, commitId: string): Promise<string>;
  getCommitSignature(repoPath: string, commitId: string): Promise<CommitSignature>;
  stageFile(repoPath: string, path: string): Promise<void>;
  unstageFile(repoPath: string, path: string): Promise<void>;
  stageHunk(repoPath: string, path: string, oldStart: number, newStart: number, hunk: HunkOptions): Promise<void>;
  stageLines(repoPath: string, path: string, oldStart: number, newStart: number, hunk: HunkOptions, lineIndices: number[], expectedLines: string[]): Promise<void>;
  unstageHunk(repoPath: string, path: string, oldStart: number, newStart: number, hunk: HunkOptions): Promise<void>;
  unstageLines(repoPath: string, path: string, oldStart: number, newStart: number, hunk: HunkOptions, lineIndices: number[], expectedLines: string[]): Promise<void>;
  discardHunk(repoPath: string, path: string, oldStart: number, newStart: number, hunk: HunkOptions): Promise<void>;
  discardLines(repoPath: string, path: string, oldStart: number, newStart: number, hunk: HunkOptions, lineIndices: number[], expectedLines: string[]): Promise<void>;
  discardFile(repoPath: string, path: string): Promise<void>;
  // Discards exactly `paths` (the list the user confirmed); folders are skipped and reported.
  discardAllUnstaged(repoPath: string, paths: string[]): Promise<void>;
  /**
   * Reverts (`reverse`) or applies a commit's file or hunk in the working tree (#448); never
   * commits. Rejects, changing nothing, when it doesn't apply. Recorded for Undo.
   */
  applyCommitChange(repoPath: string, change: CommitChangeRequest): Promise<void>;
  /** The patch text of `source` (#448). */
  getPatch(repoPath: string, source: PatchSource): Promise<PatchContent>;
  /**
   * Applies a patch file's content (a diff, or `git format-patch` mails in order) to the working
   * tree, all or nothing (#448); `how` also stages it, or commits each mail with its authorship
   * (#658). Recorded for Undo.
   */
  applyPatch(repoPath: string, content: PatchContent, how: PatchApply): Promise<void>;
  /** What a patch file's content holds and whether it applies (#658); changes nothing. */
  previewPatch(repoPath: string, content: PatchContent): Promise<PatchPreview>;
  /** Asks the user where to save `content` and writes it: the path, or `null` when cancelled. */
  savePatchFile(suggestedName: string, content: PatchContent): Promise<string | null>;
  /** Asks the user for a patch file: its content, or `null` when cancelled. */
  openPatchFile(): Promise<PatchContent | null>;
  /**
   * Reads the patch file at `path`, one dropped on the window (#649). Rejects for a file not
   * named like a patch (`.patch`, `.diff`, `.mbox`, `.txt`) or larger than 64 MB. Desktop only:
   * the VSCode extension gets no file drops and rejects without calling anything.
   */
  readPatchFile(path: string): Promise<PatchContent>;
  ignorePath(repoPath: string, pattern: string): Promise<void>;
  getDiffSettings(): Promise<DiffSettings>;
  setDiffSettings(settings: DiffSettings): Promise<void>;
  /**
   * Commits the index, running the commit hooks unless `noVerify` (which skips `pre-commit` and
   * `commit-msg`). A failing gate hook resolves `HookFailed`; only real failures reject. Hook
   * progress arrives on `subscribeHookProgress`.
   */
  commit(repoPath: string, message: string, noVerify: boolean, trailers?: Trailer[]): Promise<CommitOutcome>;
  listBranches(repoPath: string): Promise<BranchInfo[]>;
  /** Creates `name` at `startPoint`; with `checkout` it switches to it too (#656). */
  createBranch(repoPath: string, name: string, startPoint: string, checkout: boolean): Promise<void>;
  /** Rejects with kind `LocalChanges` when local changes are in the way (#653). */
  switchBranch(repoPath: string, name: string): Promise<void>;
  /** The files whose local changes keep a switch to `name` from running (#653). */
  getSwitchConflicts(repoPath: string, name: string): Promise<string[]>;
  /** Stashes every local change, switches to `name`, and with `reapply` pops them there (#653). */
  stashAndSwitch(repoPath: string, name: string, reapply: boolean): Promise<StashSwitchOutcome>;
  /** Sets `user.name`/`user.email` for this repository, or with `global` for every one (#689). */
  setIdentity(repoPath: string, name: string, email: string, global: boolean): Promise<void>;
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
  /** Tags `target` (a full commit id), or HEAD's commit when `null`; `message` makes it annotated. */
  createTag(repoPath: string, name: string, message: string | null, target: string | null): Promise<void>;
  deleteTag(repoPath: string, name: string): Promise<void>;
  /** `prune` `null` = git config decides (`remote.<name>.prune`, then `fetch.prune`). */
  fetchRemote(repoPath: string, remoteName: string, prune: boolean | null): Promise<string>;
  /** Every push runs the `pre-push` hook unless `noVerify`; a failure completes `HookFailed`. */
  pushCurrentBranch(repoPath: string, remoteName: string, noVerify: boolean): Promise<string>;
  pushTags(repoPath: string, remoteName: string, names: string[], noVerify: boolean): Promise<string>;
  /**
   * Fetches every remote in turn under one operation id. A failed remote arrives as a
   * `RemoteFailed` progress event; the operation completes unless every remote failed.
   */
  fetchAllRemotes(repoPath: string, prune: boolean | null): Promise<string>;
  /**
   * The "Fetch automatically" tick: a Fetch all reported as `BackgroundFetch` (with
   * `RemoteFailed` events like `fetchAllRemotes`). Resolves `null`, starting nothing, while the
   * repository has another operation queued or running (a fetch, pull, push, commit, …).
   * `intervalMinutes` is the setting's interval: a remote that can't be reached is skipped for
   * twice the interval, doubling with each failure in a row up to an hour (#462).
   */
  backgroundFetch(repoPath: string, prune: boolean | null, intervalMinutes: number): Promise<string | null>;
  /**
   * Pushes `localBranch` to `remoteBranch` on `remoteName`, and when `setUpstream` is set and the
   * push lands, records it as the branch's upstream.
   */
  pushBranch(repoPath: string, remoteName: string, localBranch: string, remoteBranch: string, setUpstream: boolean, noVerify: boolean): Promise<string>;
  /** Deletes the branch on the remote and its remote-tracking ref (runs `pre-push` unless `noVerify`). */
  deleteRemoteBranch(repoPath: string, remoteName: string, branchName: string, noVerify: boolean): Promise<string>;
  /** Deletes the tag on the remote only; the local tag stays (runs `pre-push` unless `noVerify`). */
  deleteRemoteTag(repoPath: string, remoteName: string, tagName: string, noVerify: boolean): Promise<string>;
  pullCurrentUpstream(repoPath: string): Promise<PullOutcome>;
  /**
   * How a diverged pull reconciles (#652): the strategy remembered for this repository, else the
   * one its git config sets (`pull.rebase`, `pull.ff=only`), else `null` (ask).
   */
  getPullStrategy(repoPath: string): Promise<PullStrategy | null>;
  setPullStrategy(repoPath: string, strategy: PullStrategy): Promise<void>;
  /**
   * Asks the backend to abort the in-flight transfer or hook run `operationId` (commit/amend/
   * Continue ids arrive on `subscribeHookProgress`). Resolves as soon as the
   * request is recorded, not when the transfer stops: the cancellation itself arrives as that
   * operation's terminal `subscribeTransferProgress` event with `errorKind: "Cancelled"`.
   * Unknown or already-finished ids are a no-op.
   */
  cancelTransfer(repoPath: string, operationId: string): Promise<void>;
  subscribeTransferProgress(listener: (progress: TransferProgress) => void): () => void;
  subscribeHookProgress(listener: (event: HookProgress) => void): () => void;
  /**
   * File manager drags onto the window (#649), until the returned function is called. Desktop
   * only: in the VSCode extension, VSCode handles drops itself and this never calls `listener`.
   */
  subscribeFileDrops(listener: (event: FileDropEvent) => void): () => void;
  /**
   * The VSCode extension host's requests (#677), until the returned function is called; the host
   * holds them until the first subscription. Desktop: never calls `listener`.
   */
  subscribeHostRequests(listener: (request: HostRequest) => void): () => void;
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
  /** Amends HEAD; hooks and `noVerify` as for `commit`. `options.author` replaces HEAD's author. */
  amendCommit(repoPath: string, message: string, noVerify: boolean, options?: AmendOptions): Promise<CommitOutcome>;
  /** Both sides of `path` in `source`, for the image preview (each capped at 20 MB). */
  getFileSides(repoPath: string, path: string, source: DiffSource): Promise<FileSides>;
  /**
   * One folder (`dir`, `""` for the root) of `commitId`'s tree, folders first. With a non-blank
   * `filter`, every file whose path contains it instead (case-insensitive, at most 500) (#446).
   */
  getCommitTree(repoPath: string, commitId: string, dir: string, filter: string | null): Promise<TreeListing>;
  /** `path` as it is in `commitId`, for the read-only viewer. */
  getFileAtCommit(repoPath: string, commitId: string, path: string): Promise<FileContent>;
  /**
   * Asks where to save `path` as it is in `commitId`, then writes it there byte for byte.
   * Resolves the path written, or `null` when the user cancelled.
   */
  saveFileVersion(repoPath: string, commitId: string, path: string): Promise<string | null>;
  /** Writes `path` as it is in `commitId` to the work tree (not the index); one undo step. */
  restoreFileVersion(repoPath: string, commitId: string, path: string): Promise<void>;
  /** Up to 20 recent authors (other than the current user), newest first. */
  listRecentAuthors(repoPath: string): Promise<SignatureInput[]>;
  getCommitMessageSettings(): Promise<CommitMessageSettings>;
  setCommitMessageSettings(settings: CommitMessageSettings): Promise<void>;
  getFetchSettings(): Promise<FetchSettings>;
  setFetchSettings(settings: FetchSettings): Promise<void>;
  getAppearanceSettings(): Promise<AppearanceSettings>;
  setAppearanceSettings(settings: AppearanceSettings): Promise<void>;
  /** Only the desktop app uses these; the VS Code extension updates through its marketplace. */
  getUpdateSettings(): Promise<UpdateSettings>;
  setUpdateSettings(settings: UpdateSettings): Promise<void>;
  /** Labels of the next undo/redo step for this repo; `null` when the stack is empty. */
  getUndoState(repoPath: string): Promise<UndoState>;
  /** Refusals (`Nothing`, `RepositoryChanged`, `BranchHeld`, `OperationInProgress`) resolve; only real failures reject. */
  undo(repoPath: string): Promise<UndoOutcome>;
  redo(repoPath: string): Promise<UndoOutcome>;
  /** `mainline` (1-based parent) is required for a merge commit and refused for any other. */
  cherryPick(repoPath: string, commitId: string, mainline?: number | null): Promise<PickOutcome>;
  revertCommit(repoPath: string, commitId: string, mainline?: number | null): Promise<PickOutcome>;
  /**
   * Picks several commits as one sequence. Ids are oldest-first; cherry-pick applies them in that
   * order. A conflict pauses it (see `HeadInfo.sequence`) until `continueSequence`/`abortSequence`.
   */
  cherryPickMany(repoPath: string, commitIdsOldestFirst: string[]): Promise<SequenceOutcome>;
  /** Like `cherryPickMany`, but reverts, newest-first (as `git revert A..B`). */
  revertMany(repoPath: string, commitIdsOldestFirst: string[]): Promise<SequenceOutcome>;
  /**
   * Continues a paused sequence. The step it commits runs the commit hooks unless `noVerify`
   * (default false); a failing one resolves `HookFailed`. Hook progress arrives on
   * `subscribeHookProgress`.
   */
  continueSequence(repoPath: string, noVerify?: boolean): Promise<SequenceOutcome>;
  /**
   * Aborts the stored sequence; without one, a terminal `git cherry-pick A B C` (see
   * `HeadInfo.terminalSequence`) like `git cherry-pick --abort` (#253). Confirm first when it
   * `rewinds`: its commits are removed. A terminal sequence is aborted only with `confirmed`
   * (what the user saw); if it no longer matches, the call rejects and nothing changes.
   */
  abortSequence(repoPath: string, confirmed?: ConfirmedAbort | null): Promise<SequenceAbort>;
  resetToCommit(repoPath: string, commitId: string, mode: ResetMode): Promise<void>;
  checkoutCommit(repoPath: string, commitId: string): Promise<void>;
  /** Resolves with the transfer operation id; progress arrives via `subscribeTransferProgress`. */
  forcePushCurrentBranch(repoPath: string, remoteName: string, noVerify: boolean): Promise<string>;
  resolveAddDeleteConflict(repoPath: string, path: string, choice: FileConflictChoice): Promise<void>;
  /** Takes one side's whole file for any conflict shape (`git checkout --ours/--theirs` + add). */
  resolveWholeFile(repoPath: string, path: string, side: ConflictSide): Promise<void>;
  commitsSince(repoPath: string, onto: string): Promise<RebasePlanCommit[]>;
  /** `edit` names a one-click history edit's Undo entry (#473); omitted for a planned rebase. */
  startRebase(
    repoPath: string,
    onto: string,
    plan: RebasePlanEntry[],
    edit?: HistoryEditTarget | null,
  ): Promise<RebaseStepResult>;
  rebaseContinue(repoPath: string): Promise<RebaseStepResult>;
  abortRebase(repoPath: string): Promise<RebaseAbort>;
  detectForgeRepository(repoPath: string): Promise<ForgeRepository[]>;
  saveForgeToken(repoPath: string, provider: ForgeProvider, account: string, token: string): Promise<void>;
  forgetForgeToken(repoPath: string, provider: ForgeProvider, account: string): Promise<void>;
  listPullRequests(repoPath: string, remoteName: string, account: string): Promise<PullRequestList>;
  createPullRequest(repoPath: string, remoteName: string, account: string, pullRequest: CreatePullRequest): Promise<PullRequest>;
  // Opens `url` in the user's default external browser/handler rather than navigating this
  // app's own window away from the app entirely (see `tauriRepoClient.ts`). Callers: the pull
  // request panel, the user guide command and commit message links (#612). Both transports
  // refuse anything but an `http`/`https` URL.
  openExternalUrl(url: string): Promise<void>;
  // External tools. The five launch methods (`openInEditor` … `openMergeTool`) are answered by
  // the VSCode extension host itself (see `sidecarBridge.ts`'s `NATIVE_METHODS`); the desktop
  // app spawns the programs from Rust. `line` is 1-based, `null` when unknown.
  openInEditor(repoPath: string, path: string, line: number | null): Promise<void>;
  // `path === null` reveals the work tree root.
  revealInFileManager(repoPath: string, path: string | null): Promise<void>;
  openTerminal(repoPath: string): Promise<void>;
  // Which of `.gitignore` and `.gitattributes` exist at the work tree root, in that order;
  // `null` for a bare repository (#624). A symbolic link counts as existing.
  getRootFiles(repoPath: string): Promise<RootFileName[] | null>;
  // Creates a missing `.gitignore` or `.gitattributes` at the work tree root, empty, so
  // `openInEditor` can open it; an existing file is left as it is. No other name or path, and
  // never through a symbolic link (#624).
  createRootFile(repoPath: string, name: RootFileName): Promise<void>;
  openDiffTool(repoPath: string, path: string, source: DiffSource): Promise<void>;
  // Resolves when the tool exits on desktop, and at once with "HandedOff" in VSCode.
  openMergeTool(repoPath: string, path: string): Promise<MergeToolOutcome>;
  // With `force: false` a file that still has conflict markers is never staged
  // ("MarkersRemain"); the caller asks and retries with `force: true`.
  markResolved(repoPath: string, path: string, force: boolean): Promise<MarkResolvedOutcome>;
  getExternalToolSettings(): Promise<ExternalToolSettings>;
  setExternalToolSettings(settings: ExternalToolSettings): Promise<void>;
  // The `diff.tool`/`merge.tool` git would use for this repo (repository-level tool settings
  // are reported with a `problem`, because Browsitory ignores them).
  getGitTools(repoPath: string): Promise<GitTools>;
  // Single choke point for frontend failure logging (see `frontend/src/lib/logger.ts`'s global
  // `error`/`unhandledrejection` handlers, the only caller): each transport routes it into its
  // own durable log sink — `tauriRepoClient.ts` writes to the same rotated log file as the Rust
  // backend via `@tauri-apps/plugin-log`, `vscodeRepoClient.ts` forwards it to the extension
  // host's "Browsitory" output channel — so a bug report doesn't need console access to a
  // running session to diagnose, regardless of transport.
  logFrontendError(context: string, error: unknown): Promise<void>;
}

// Generated from the shared wire DTOs (crates/repo-service/src/wire.rs) by ts-rs.
// Do not edit. Regenerate: UPDATE_TRANSPORT_METHODS=1 cargo test -p repo-service wire_bindings_are_current

export type AppearanceSettings = { codeFont: string | null, codeFontSize: number | null, uiSize: UiSize, tabSize: number, dateFormat: DateFormat, };

export type BisectInfo = { 
/**
 * A branch name, or a commit id when the bisect started on a detached HEAD.
 */
startedFrom: string, };

export type BlameLine = { lineNumber: number, content: string, commitId: string, shortId: string, authorName: string, timestamp: number, };

export type BranchInfo = { name: string, isCurrent: boolean, upstream: string | null, ahead: number | null, behind: number | null, };

export type ChangeStamp = { refs: string, workTree: string, };

export type ChangedFile = { path: string, oldPath: string | null, };

export type CollapsedMerge = { mergeId: string, mergePosition: number, offset: number, };

export type CommitChangeRequest = { commitId: string, path: string, oldPath: string | null, 
/**
 * `null`: the whole file.
 */
hunk: CommitHunk | null, 
/**
 * Revert the change rather than apply it.
 */
reverse: boolean, 
/**
 * Also stage the result.
 */
stage: boolean, };

export type CommitDetails = { author: CommitPerson, committer: CommitPerson, branches: Array<string>, remoteBranches: Array<string>, tags: Array<string>, complete: boolean, };

export type CommitHunk = { oldStart: number, newStart: number, contextLines: number, algorithm: DiffAlgorithm, };

export type CommitLocation = { commitId: string, position: number | null, branches: Array<string>, remoteBranches: Array<string>, 
/**
 * The collapsed merge that holds the commit (#617); left out when there is none.
 */
merge?: CollapsedMerge, };

export type CommitMessageSettings = { summaryLimit: number, bodyRuler: number, showRuler: boolean, };

export type CommitOutcome = { "kind": "Committed", commitId: string, } | { "kind": "HookFailed", hook: string, output: string, };

export type CommitPerson = { name: string, email: string, timestamp: number, };

export type CommitSignature = { status: SignatureStatus, format: SignatureFormat | null, signer: string | null, key: string | null, detail: string | null, };

export type CommitStats = { filesChanged: number, insertions: number, deletions: number, 
/**
 * Empty unless `withFiles` asked for them.
 */
files: Array<FileStats>, };

export type CommitStatsEntry = { commitId: string, stats: CommitStats | null, };

export type CompareRange = { fromId: string, toId: string, };

export type ConfirmedAbort = { headId: string, startedAt: string, rewinds: boolean, commitsToRemove: number, pausedOnConflict: boolean, };

export type ConflictSegment = { "kind": "Clean", content: string, } | { "kind": "Conflict", ours: string, base: string, theirs: string, };

export type ConflictSide = "Ours" | "Theirs";

export type CreatePullRequest = { title: string, description: string | null, sourceBranch: string, targetBranch: string, };

export type DateFormat = "Iso" | "System";

export type DiffAlgorithm = "Default" | "Patience" | "Minimal";

export type DiffHunk = { oldStart: number, oldLines: number, newStart: number, newLines: number, lines: Array<DiffLine>, };

export type DiffLine = { origin: DiffLineOrigin, content: string, };

export type DiffLineOrigin = "Add" | "Remove" | "Context";

export type DiffOptions = { ignoreWhitespace: boolean, contextLines: number, algorithm: DiffAlgorithm, };

export type DiffSettings = { ignoreWhitespace: boolean, contextLines: number, syntaxHighlight: boolean, wordWrap: WordWrap, algorithm: DiffAlgorithm, showWhitespace: boolean, expandUntracked: boolean, };

export type DiffSource = { "kind": "Unstaged" } | { "kind": "Staged", oldPath: string | null, } | { "kind": "Commit", commitId: string, oldPath: string | null, } | { "kind": "Range", fromId: string, toId: string | null, oldPath: string | null, };

export type ErrorKind = "Other" | "WorkerStopped" | "CredentialStoreFailure" | "DirtyWorktree" | "PullFailed" | "Cancelled" | "AddDeleteConflict" | "InvalidSearchQuery" | "PermissionDenied" | "LocalChanges" | "MissingIdentity";

export type ExternalToolSettings = { editor: string | null, terminal: string | null, };

export type FastForwardMode = "Allow" | "Never" | "Only";

export type FetchSettings = { autoFetchMinutes: number, };

export type FileConflictChoice = "Ours" | "Theirs" | "Delete";

export type FileContent = { "kind": "Text", size: number, text: string, } | { "kind": "Image", size: number, base64: string, mime: string, } | { "kind": "Binary", size: number, } | { "kind": "TooLarge", size: number, };

export type FileHistoryEntry = { commit: GraphCommit, path: string, oldPath: string | null, };

export type FileSide = { "kind": "Missing" } | { "kind": "TooLarge", size: number, } | { "kind": "Loaded", size: number, base64: string, mime: string | null, };

export type FileSides = { old: FileSide, new: FileSide, };

export type FileStats = { path: string, insertions: number, deletions: number, binary: boolean, };

export type ForgeProvider = "GitHub" | "Bitbucket";

export type ForgeRepository = { provider: ForgeProvider, host: string, owner: string, name: string, remoteName: string, };

export type GitConfigSource = "System" | "Xdg" | "Global" | "Local" | "Worktree" | "Other";

export type GitToolInfo = { name: string, command: string | null, source: GitConfigSource, viaMergeTool: boolean, problem: string | null, };

export type GitTools = { diffTool: GitToolInfo | null, mergeTool: GitToolInfo | null, };

export type GraphCommit = { id: string, shortId: string, summary: string, authorName: string, authorEmail: string, timestamp: number, parentIds: Array<string>, branchRefs: Array<string>, remoteBranchRefs: Array<string>, tags: Array<string>, 
/**
 * Only on a merge row of a first-parent graph (#617): the commits its other parents brought
 * in, which `getMergedCommits` lists. Left out everywhere else.
 */
mergedCount?: number, };

export type GraphPage = { commits: Array<GraphCommit>, 
/**
 * `null` once the walk is done.
 */
nextCursor: string | null, 
/**
 * The cursor belonged to another walk: no commits, load the graph from the start.
 */
stale: boolean, };

export type GraphSelection = { 
/**
 * Local branch names: `null` walks every local branch, `[]` none of them.
 */
local: Array<string> | null, 
/**
 * Remote-tracking branches as `<remote>/<branch>`; required, `[]` walks no remote branch.
 */
remote: Array<string>, 
/**
 * "Collapse merges" (#617): the graph and `locateCommit` follow first parents from every
 * selected tip; `searchCommits` ignores it. Absent is `false`.
 */
firstParent?: boolean, };

export type HeadInfo = { headId: string | null, detachedAt: string | null, headPushed: boolean, operation: OperationKind | null, 
/**
 * `operation`'s changes are already in HEAD; only git's state files are left (#240). The next
 * commit or amend clears them, and the commit box isn't pre-filled with the old message.
 */
operationLeftover: boolean, squashMessage: string | null, commitHooks: Array<string>, sequence: SequenceProgress | null, rebase: RebaseInfo | null, 
/**
 * A `git bisect` started in a terminal: `{ "startedFrom": "main" }`, or `null`.
 */
bisect: BisectInfo | null, 
/**
 * A `git cherry-pick A B C` / `git revert A B` started in a terminal and not finished, or
 * `null` (#258).
 */
terminalSequence: TerminalSequence | null, 
/**
 * A `git rebase` or `git am` started in a terminal and not finished, or `null` (#263).
 */
terminalRebase: TerminalRebase | null, };

export type HistoryEditKind = "Reword" | "Squash" | "Fixup" | "MoveUp" | "MoveDown" | "Drop";

export type HistoryEditTarget = { kind: HistoryEditKind, commitId: string, };

export type HookProgress = { repoPath: string, operationId: string, hook: string, phase: HookProgressPhase, source: HookSource, };

export type HookProgressPhase = "Started" | "Finished";

export type HookSource = "Commit" | "Checkout" | "Merge" | "Pick" | "Patch";

export type LineHistory = { entries: Array<LineHistoryEntry>, 
/**
 * The walk stopped at its commit cap, so older changes may be missing.
 */
truncated: boolean, };

export type LineHistoryEntry = { commit: GraphCommit, path: string, oldPath: string | null, 
/**
 * The tracked lines in this commit's version of the file, 1-based and inclusive.
 */
startLine: number, endLine: number, };

export type MarkResolvedOutcome = "Resolved" | "MarkersRemain";

export type MergeOptions = { fastForward: FastForwardMode, squash: boolean, };

export type MergeOutcome = { "kind": "UpToDate" } | { "kind": "FastForwarded" } | { "kind": "Merged" } | { "kind": "Squashed" } | { "kind": "Conflicted", files: Array<string>, };

export type OpenRepoEntry = { path: string, workspaceId: string | null, };

export type OpenRepoEntryInput = { path: string, workspaceId: string | null, };

export type OperationKind = "Merge" | "CherryPick" | "Revert";

export type PatchApply = "WorkTree" | "Stage" | "Commits";

export type PatchApplyOutcome = { "kind": "Applied" } | { "kind": "HookFailed", hook: string, output: string, subject: string, };

export type PatchContent = { base64: string, };

export type PatchFileStatus = "Added" | "Deleted" | "Modified" | "Renamed" | "Copied";

export type PatchMail = { subject: string, body: string, authorName: string, authorEmail: string, 
/**
 * Seconds since the epoch; `null` when the mail's date couldn't be read.
 */
timestamp: number | null, };

export type PatchPreview = { 
/**
 * One per mail, in order (one for a plain diff).
 */
parts: Array<PatchPreviewPart>, 
/**
 * Why it doesn't apply to the working tree; only the first mail is checked against it.
 */
problem: string | null, };

export type PatchPreviewFile = { path: string, oldPath: string | null, status: PatchFileStatus, additions: number, deletions: number, binary: boolean, };

export type PatchPreviewPart = { 
/**
 * `null` for a plain diff.
 */
mail: PatchMail | null, files: Array<PatchPreviewFile>, };

export type PatchSource = { "kind": "Commits", commitIds: Array<string>, } | { "kind": "CommitFile", commitId: string, path: string, oldPath: string | null, } | { "kind": "Staged" };

export type PickOutcome = { "kind": "Committed", commitId: string, } | { "kind": "Conflicted", files: Array<string>, } | { "kind": "HookFailed", hook: string, output: string, };

export type PullOutcome = { "kind": "UpToDate" } | { "kind": "FastForwarded", upstreamRef: string, } | { "kind": "Diverged", upstreamRef: string, } | { "kind": "NoFastForward", upstreamRef: string, };

export type PullRequest = { id: string, number: number, title: string, url: string, author: string, sourceBranch: string, targetBranch: string, state: string, };

export type PullRequestList = { pullRequests: Array<PullRequest>, truncated: boolean, };

export type PullStrategy = "Merge" | "Rebase" | "FastForwardOnly";

export type RebaseAbort = { restored: boolean, };

export type RebaseAction = { "kind": "Pick" } | { "kind": "Reword", message: string, } | { "kind": "Edit" } | { "kind": "Squash" } | { "kind": "Fixup" } | { "kind": "Drop" };

export type RebaseDivergence = "HeadMoved" | "BranchMoved" | "BranchMissing" | "OtherOperation";

export type RebaseHealth = { "kind": "Paused" } | { "kind": "Diverged", "reason": RebaseDivergence };

export type RebaseInfo = { branch: string, onto: string, currentStep: number, totalSteps: number, health: RebaseHealth, };

export type RebasePlanCommit = { id: string, shortId: string, summary: string, authorName: string, timestamp: number, };

export type RebasePlanEntry = { commitId: string, action: RebaseAction, combinedMessage: string | null, };

export type RebaseStepResult = { "kind": "Conflicted", files: Array<string>, } | { "kind": "PausedForEdit" } | { "kind": "Advanced" } | { "kind": "Done" };

export type ReflogEntry = { reference: string, oldId: string, newId: string, committerName: string, committerEmail: string, timestamp: number, message: string, summary: string | null, };

export type RemoteAuthMode = "HttpsToken" | "SshAgent";

export type RemoteInfo = { name: string, fetchUrl: string, pushUrl: string | null, authMode: RemoteAuthMode | null, authUsername: string | null, 
/**
 * Effective git-config prune default for this remote (the UI's Prune toggle fallback).
 */
prune: boolean, };

export type RepoAlias = { repoPath: string, alias: string, };

export type RepoErrorPayload = { kind: ErrorKind, message: string, };

export type RepoWatch = { repoPath: string, kind: RepoWatchKind, };

export type RepoWatchKind = "Changed" | "Stopped";

export type ResetMode = "soft" | "mixed" | "hard";

export type SearchPage = { commits: Array<GraphCommit>, nextCursor: string | null, scanned: number, };

export type SequenceAbort = { rewound: boolean, };

export type SequenceKind = "CherryPick" | "Revert";

export type SequenceOutcome = { "kind": "Completed", applied: number, skipped: Array<string>, } | { "kind": "Conflicted", files: Array<string>, progress: SequenceProgress, } | { "kind": "HookFailed", hook: string, output: string, progress: SequenceProgress, } | { "kind": "Stopped", kept: string, progress: SequenceProgress, };

export type SequenceProgress = { kind: SequenceKind, currentStep: number, totalSteps: number, currentCommit: string, };

export type SignatureFormat = "openpgp" | "ssh";

export type SignatureInput = { name: string, email: string, };

export type SignatureStatus = "unsigned" | "good" | "goodUntrusted" | "bad" | "unknownKey" | "expired" | "expiredKey" | "revokedKey" | "error";

export type StashApplyOutcome = { "kind": "Applied" } | { "kind": "Conflicted", files: Array<string>, };

export type StashEntry = { index: number, message: string, commitId: string, branch: string | null, timestamp: number, fileCount: number, hasStagedChanges: boolean, };

export type StashFiles = { tracked: Array<string>, untracked: Array<string>, untrackedCommitId: string | null, };

export type StashOptions = { message: string | null, keepIndex: boolean, includeUntracked: boolean, paths: Array<string>, };

export type StashSwitchOutcome = { "kind": "Stashed" } | { "kind": "Reapplied" } | { "kind": "Conflicted", files: Array<string>, } | { "kind": "NotReapplied", reason: string, };

export type StatusEntry = { path: string, 
/**
 * A staged rename's old path; always present (`null` otherwise).
 */
oldPath: string | null, staged: boolean, kind: StatusKind, };

export type StatusKind = "New" | "Modified" | "Deleted" | "Renamed" | "TypeChange" | "Conflicted";

export type SubmoduleInfo = { path: string, url: string | null, gitlinkId: string | null, initialized: boolean, headId: string | null, 
/**
 * Absolute path of the checked-out submodule, the folder its tab opens; `null` when it
 * isn't checked out (#445).
 */
workdir: string | null, };

export type TagInfo = { name: string, targetId: string, commitId: string | null, annotated: boolean, message: string | null, taggerName: string | null, timestamp: number | null, };

export type TerminalRebase = { kind: TerminalRebaseKind, branch: string, onto: string, currentStep: number, totalSteps: number, };

export type TerminalRebaseKind = "Rebase" | "Am";

export type TerminalSequence = { kind: SequenceKind, pausedOnConflict: boolean, startedAt: string, startedAtSummary: string, rewinds: boolean, commitsToRemove: number, };

export type Trailer = { key: string, value: string, };

export type TransferErrorKind = "NonFastForward" | "RejectedRemoteRef" | "StaleLease" | "MissingCredential" | "CredentialStoreFailure" | "SshAgentFailure" | "TransferFailed" | "Cancelled" | "HookFailed";

export type TransferOperation = "Fetch" | "FetchAll" | "Pull" | "PushBranch" | "ForcePushBranch" | "PushTags" | "DeleteRemoteBranch" | "DeleteRemoteTag" | "Clone" | "BackgroundFetch";

export type TransferProgress = { operationId: string, operation: TransferOperation, phase: TransferProgressPhase, errorKind: TransferErrorKind | null, current: number, total: number, receivedBytes: number, message: string | null, 
/**
 * Only on `RemoteFailed`: the remote that failed during a Fetch all.
 */
remoteName?: string, 
/**
 * Local `pre-push` hook output on a `HookFailed` completion, `null` otherwise. Unlike
 * `message` this comes from the user's own script, not the remote.
 */
hookOutput: string | null, };

export type TransferProgressPhase = "Starting" | "Receiving" | "Updating" | "RunningHook" | "RemoteFailed" | "Completed" | "Failed";

export type TreeEntry = { path: string, name: string, kind: TreeEntryKind, size: number | null, };

export type TreeEntryKind = "Folder" | "File" | "Executable" | "Symlink" | "Submodule";

export type TreeListing = { entries: Array<TreeEntry>, truncated: boolean, };

export type UiSize = "Small" | "Normal" | "Large";

export type UndoOutcome = { "kind": "Done", label: string, } | { "kind": "Nothing" } | { "kind": "RepositoryChanged", label: string, } | { "kind": "BranchHeld", label: string, message: string, } | { "kind": "OperationInProgress" };

export type UndoState = { undo: string | null, redo: string | null, };

export type UpdateSettings = { checkAutomatically: boolean, };

export type UpstreamInfo = { localBranch: string, remoteName: string, remoteBranch: string, ahead: number | null, behind: number | null, 
/**
 * Where Push lands under `push.default`; null = ask with the Push to… dialog.
 */
pushBranch: string | null, };

export type WordWrap = "Auto" | "On" | "Off";

export type Workspace = { id: string, name: string, rootPath: string, memberPaths: Array<string>, };

export type WorktreeInfo = { name: string, path: string, head: string | null, isMain: boolean, isLocked: boolean, isPrunable: boolean, };

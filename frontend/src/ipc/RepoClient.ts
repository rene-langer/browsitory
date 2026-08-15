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
}

export interface WorktreeInfo {
  name: string;
  path: string;
  head: string | null;
  isMain: boolean;
  isLocked: boolean;
  isPrunable: boolean;
}

export interface RemoteInfo {
  name: string;
  fetchUrl: string;
  pushUrl: string | null;
  authMode: RemoteAuthMode | null;
  authUsername: string | null;
}

export type RemoteAuthMode = "HttpsToken" | "SshAgent";

export interface UpstreamInfo {
  localBranch: string;
  remoteName: string;
  remoteBranch: string;
}

export type TransferOperation = "Fetch" | "Pull" | "PushBranch" | "PushTags";
export type TransferErrorKind = "NonFastForward" | "RejectedRemoteRef" | "MissingCredential" | "CredentialStoreFailure" | "SshAgentFailure" | "TransferFailed";

export interface TransferProgress {
  operationId: string;
  operation: TransferOperation;
  phase: "Starting" | "Receiving" | "Updating" | "Completed" | "Failed";
  errorKind: TransferErrorKind | null;
  current: number;
  total: number;
  receivedBytes: number;
  message: string | null;
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
  | { kind: "Conflicted"; files: string[] };

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

export interface RepoClient {
  pickRepoFolder(): Promise<string | null>;
  listRecentRepos(): Promise<string[]>;
  openRepo(path: string): Promise<void>;
  getStatus(): Promise<StatusEntry[]>;
  getCommitGraph(limit: number): Promise<GraphCommit[]>;
  getWorkingDiff(path: string, staged: boolean): Promise<DiffHunk[]>;
  getCommitDiff(commitId: string, path: string): Promise<DiffHunk[]>;
  getCommitFiles(commitId: string): Promise<string[]>;
  stageFile(path: string): Promise<void>;
  unstageFile(path: string): Promise<void>;
  commit(message: string): Promise<void>;
  listBranches(): Promise<BranchInfo[]>;
  createBranch(name: string, startPoint: string): Promise<void>;
  switchBranch(name: string): Promise<void>;
  deleteBranch(name: string, force: boolean): Promise<void>;
  renameBranch(oldName: string, newName: string): Promise<void>;
  listWorktrees(): Promise<WorktreeInfo[]>;
  createWorktree(name: string, path: string, branch: string, startPoint: string | null): Promise<void>;
  removeWorktree(name: string): Promise<void>;
  pruneWorktrees(): Promise<void>;
  listRemotes(): Promise<RemoteInfo[]>;
  getCurrentUpstream(): Promise<UpstreamInfo | null>;
  getRemoteUpstreams(name: string): Promise<UpstreamInfo[]>;
  addRemote(name: string, fetchUrl: string, pushUrl: string | null): Promise<void>;
  renameRemote(oldName: string, newName: string): Promise<void>;
  updateRemoteUrls(name: string, fetchUrl: string, pushUrl: string | null): Promise<void>;
  removeRemote(name: string, clearUpstreams: boolean): Promise<void>;
  saveHttpsCredential(remoteName: string, username: string, token: string): Promise<void>;
  forgetHttpsCredential(remoteName: string): Promise<void>;
  setRemoteAuthMode(remoteName: string, mode: RemoteAuthMode, username: string | null): Promise<void>;
  setCurrentUpstream(remoteName: string, remoteBranch: string): Promise<void>;
  clearCurrentUpstream(): Promise<void>;
  listTags(): Promise<TagInfo[]>;
  createTag(name: string, message: string | null): Promise<void>;
  deleteTag(name: string): Promise<void>;
  fetchRemote(remoteName: string): Promise<string>;
  pushCurrentBranch(remoteName: string): Promise<string>;
  pushTags(remoteName: string, names: string[]): Promise<string>;
  pullCurrentUpstream(): Promise<PullOutcome>;
  subscribeTransferProgress(listener: (progress: TransferProgress) => void): () => void;
  listStashes(): Promise<StashEntry[]>;
  saveStash(): Promise<void>;
  applyStash(index: number): Promise<void>;
  dropStash(index: number): Promise<void>;
  getBlame(commitId: string, path: string): Promise<BlameLine[]>;
  mergeBranch(branchName: string): Promise<MergeOutcome>;
  getConflictHunks(path: string): Promise<ConflictSegment[]>;
  resolveConflict(path: string, resolvedContent: string): Promise<void>;
  abortMerge(): Promise<void>;
  getMergeMessage(): Promise<string | null>;
  resolveAddDeleteConflict(path: string, choice: FileConflictChoice): Promise<void>;
  commitsSince(onto: string): Promise<RebasePlanCommit[]>;
  startRebase(onto: string, plan: RebasePlanEntry[]): Promise<RebaseStepResult>;
  rebaseContinue(): Promise<RebaseStepResult>;
  abortRebase(): Promise<void>;
  getRebaseProgress(): Promise<{ currentStep: number; totalSteps: number } | null>;
}

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
  listWorkspaces(): Promise<Workspace[]>;
  saveWorkspace(name: string, root: string, members: string[]): Promise<string>;
  updateWorkspace(id: string, name: string, members: string[]): Promise<void>;
  deleteWorkspace(id: string): Promise<void>;
  getStatus(repoPath: string): Promise<StatusEntry[]>;
  getCommitGraph(repoPath: string, limit: number, selectedBranches: string[] | null): Promise<GraphCommit[]>;
  getGraphBranchSelection(repoPath: string): Promise<string[] | null>;
  setGraphBranchSelection(repoPath: string, selectedBranches: string[]): Promise<void>;
  getWorkingDiff(repoPath: string, path: string, staged: boolean): Promise<DiffHunk[]>;
  getCommitDiff(repoPath: string, commitId: string, path: string): Promise<DiffHunk[]>;
  getCommitFiles(repoPath: string, commitId: string): Promise<string[]>;
  stageFile(repoPath: string, path: string): Promise<void>;
  unstageFile(repoPath: string, path: string): Promise<void>;
  stageHunk(repoPath: string, path: string, oldStart: number, newStart: number): Promise<void>;
  unstageHunk(repoPath: string, path: string, oldStart: number, newStart: number): Promise<void>;
  discardHunk(repoPath: string, path: string, oldStart: number, newStart: number): Promise<void>;
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
  fetchRemote(repoPath: string, remoteName: string): Promise<string>;
  pushCurrentBranch(repoPath: string, remoteName: string): Promise<string>;
  pushTags(repoPath: string, remoteName: string, names: string[]): Promise<string>;
  pullCurrentUpstream(repoPath: string): Promise<PullOutcome>;
  subscribeTransferProgress(listener: (progress: TransferProgress) => void): () => void;
  listStashes(repoPath: string): Promise<StashEntry[]>;
  saveStash(repoPath: string): Promise<void>;
  applyStash(repoPath: string, index: number): Promise<void>;
  dropStash(repoPath: string, index: number): Promise<void>;
  getBlame(repoPath: string, commitId: string, path: string): Promise<BlameLine[]>;
  mergeBranch(repoPath: string, branchName: string): Promise<MergeOutcome>;
  getConflictHunks(repoPath: string, path: string): Promise<ConflictSegment[]>;
  resolveConflict(repoPath: string, path: string, resolvedContent: string): Promise<void>;
  abortMerge(repoPath: string): Promise<void>;
  getMergeMessage(repoPath: string): Promise<string | null>;
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
}

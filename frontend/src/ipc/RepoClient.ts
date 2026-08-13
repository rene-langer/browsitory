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

export type ConflictSegment =
  | { kind: "Clean"; content: string }
  | { kind: "Conflict"; ours: string; theirs: string };

export type FileConflictChoice = "Ours" | "Theirs" | "Delete";

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
}

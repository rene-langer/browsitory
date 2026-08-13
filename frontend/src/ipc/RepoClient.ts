export type StatusKind = "New" | "Modified" | "Deleted" | "Renamed" | "TypeChange";

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
}

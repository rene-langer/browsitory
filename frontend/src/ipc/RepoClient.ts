export type StatusKind = "New" | "Modified" | "Deleted" | "Renamed" | "TypeChange";

export interface StatusEntry {
  path: string;
  staged: boolean;
  kind: StatusKind;
}

export interface CommitInfo {
  id: string;
  shortId: string;
  summary: string;
  authorName: string;
  authorEmail: string;
  timestamp: number;
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

export interface RepoClient {
  pickRepoFolder(): Promise<string | null>;
  listRecentRepos(): Promise<string[]>;
  openRepo(path: string): Promise<void>;
  getStatus(): Promise<StatusEntry[]>;
  getLog(limit: number): Promise<CommitInfo[]>;
  getWorkingDiff(path: string, staged: boolean): Promise<DiffHunk[]>;
  getCommitDiff(commitId: string, path: string): Promise<DiffHunk[]>;
  getCommitFiles(commitId: string): Promise<string[]>;
  stageFile(path: string): Promise<void>;
  unstageFile(path: string): Promise<void>;
  commit(message: string): Promise<void>;
}

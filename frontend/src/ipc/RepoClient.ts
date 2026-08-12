export type StatusKind = "New" | "Modified" | "Deleted" | "Renamed" | "TypeChange";

export interface StatusEntry {
  path: string;
  staged: boolean;
  kind: StatusKind;
}

export interface RepoClient {
  openRepo(path: string): Promise<void>;
  getStatus(): Promise<StatusEntry[]>;
}

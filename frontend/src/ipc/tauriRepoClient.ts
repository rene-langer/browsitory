import { invoke } from "@tauri-apps/api/core";
import type {
  BlameLine,
  BranchInfo,
  CommitInfo,
  DiffHunk,
  RepoClient,
  StashEntry,
  StatusEntry,
} from "./RepoClient";

export const tauriRepoClient: RepoClient = {
  pickRepoFolder: () => invoke<string | null>("pick_repo_folder"),
  listRecentRepos: () => invoke<string[]>("list_recent_repos"),
  openRepo: (path: string) => invoke("open_repo", { path }),
  getStatus: () => invoke<StatusEntry[]>("get_status"),
  getLog: (limit: number) => invoke<CommitInfo[]>("get_log", { limit }),
  getWorkingDiff: (path: string, staged: boolean) =>
    invoke<DiffHunk[]>("get_working_diff", { path, staged }),
  getCommitDiff: (commitId: string, path: string) =>
    invoke<DiffHunk[]>("get_commit_diff", { commitId, path }),
  getCommitFiles: (commitId: string) =>
    invoke<string[]>("get_commit_files", { commitId }),
  stageFile: (path: string) => invoke("stage_file", { path }),
  unstageFile: (path: string) => invoke("unstage_file", { path }),
  commit: (message: string) => invoke("commit", { message }),
  listBranches: () => invoke<BranchInfo[]>("list_branches"),
  createBranch: (name: string, startPoint: string) =>
    invoke("create_branch", { name, startPoint }),
  switchBranch: (name: string) => invoke("switch_branch", { name }),
  deleteBranch: (name: string, force: boolean) =>
    invoke("delete_branch", { name, force }),
  renameBranch: (oldName: string, newName: string) =>
    invoke("rename_branch", { oldName, newName }),
  listStashes: () => invoke<StashEntry[]>("list_stashes"),
  saveStash: () => invoke("save_stash"),
  applyStash: (index: number) => invoke("apply_stash", { index }),
  dropStash: (index: number) => invoke("drop_stash", { index }),
  getBlame: (commitId: string, path: string) =>
    invoke<BlameLine[]>("get_blame", { commitId, path }),
};

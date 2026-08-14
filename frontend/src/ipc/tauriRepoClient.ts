import { invoke } from "@tauri-apps/api/core";
import type {
  BlameLine,
  BranchInfo,
  ConflictSegment,
  DiffHunk,
  FileConflictChoice,
  GraphCommit,
  MergeOutcome,
  RebasePlanCommit,
  RebasePlanEntry,
  RebaseStepResult,
  RemoteInfo,
  RepoClient,
  StashEntry,
  StatusEntry,
  UpstreamInfo,
} from "./RepoClient";

export const tauriRepoClient: RepoClient = {
  pickRepoFolder: () => invoke<string | null>("pick_repo_folder"),
  listRecentRepos: () => invoke<string[]>("list_recent_repos"),
  openRepo: (path: string) => invoke("open_repo", { path }),
  getStatus: () => invoke<StatusEntry[]>("get_status"),
  getCommitGraph: (limit: number) =>
    invoke<GraphCommit[]>("get_commit_graph", { limit }),
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
  listRemotes: () => invoke<RemoteInfo[]>("list_remotes"),
  getCurrentUpstream: () => invoke<UpstreamInfo | null>("get_current_upstream"),
  addRemote: (name: string, fetchUrl: string, pushUrl: string | null) =>
    invoke("add_remote", { name, fetchUrl, pushUrl }),
  renameRemote: (oldName: string, newName: string) =>
    invoke("rename_remote", { oldName, newName }),
  updateRemoteUrls: (name: string, fetchUrl: string, pushUrl: string | null) =>
    invoke("update_remote_urls", { name, fetchUrl, pushUrl }),
  removeRemote: (name: string) => invoke("remove_remote", { name }),
  setCurrentUpstream: (remoteName: string, remoteBranch: string) =>
    invoke("set_current_upstream", { remoteName, remoteBranch }),
  clearCurrentUpstream: () => invoke("clear_current_upstream"),
  listStashes: () => invoke<StashEntry[]>("list_stashes"),
  saveStash: () => invoke("save_stash"),
  applyStash: (index: number) => invoke("apply_stash", { index }),
  dropStash: (index: number) => invoke("drop_stash", { index }),
  getBlame: (commitId: string, path: string) =>
    invoke<BlameLine[]>("get_blame", { commitId, path }),
  mergeBranch: (branchName: string) =>
    invoke<MergeOutcome>("start_merge", { branchName }),
  getConflictHunks: (path: string) =>
    invoke<ConflictSegment[]>("get_conflict_hunks", { path }),
  resolveConflict: (path: string, resolvedContent: string) =>
    invoke("resolve_conflict", { path, resolvedContent }),
  abortMerge: () => invoke("abort_merge"),
  getMergeMessage: () => invoke<string | null>("get_merge_message"),
  resolveAddDeleteConflict: (path: string, choice: FileConflictChoice) =>
    invoke("resolve_add_delete_conflict", { path, choice }),
  commitsSince: (onto: string) =>
    invoke<RebasePlanCommit[]>("commits_since", { onto }),
  startRebase: (onto: string, plan: RebasePlanEntry[]) =>
    invoke<RebaseStepResult>("start_rebase", { onto, plan }),
  rebaseContinue: () => invoke<RebaseStepResult>("rebase_continue"),
  abortRebase: () => invoke("abort_rebase"),
  getRebaseProgress: () =>
    invoke<{ currentStep: number; totalSteps: number } | null>("get_rebase_progress"),
};

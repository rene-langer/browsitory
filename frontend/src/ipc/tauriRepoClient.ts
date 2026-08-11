import { invoke } from "@tauri-apps/api/core";
import type { RepoClient, StatusEntry } from "./RepoClient";

export const tauriRepoClient: RepoClient = {
  openRepo: (path: string) => invoke("open_repo", { path }),
  getStatus: () => invoke<StatusEntry[]>("get_status"),
};

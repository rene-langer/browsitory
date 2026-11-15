import { errorKind, type RepoClient } from "../ipc/RepoClient";
import { repoNameOf } from "./repoName";

/** Why opening a folder failed, as the app-level banner tells it (#973, #998). */
export type OpenFailure =
  /** Anything else: the backend's message, with Retry. */
  | { kind: "failed"; path: string; message: string }
  /** A recent repository whose folder is gone: offer to remove the entry, no Retry (#998). */
  | { kind: "missing"; path: string; message: string }
  /** A folder that isn't a repository: the repositories in its direct subfolders (#973). */
  | { kind: "notRepository"; path: string; message: string; repos: string[] };

/** Hidden folders and `node_modules` hold no repository anyone means to open (#973). */
function worthListing(path: string): boolean {
  const name = repoNameOf(path);
  return !name.startsWith(".") && name !== "node_modules";
}

/**
 * Tells why opening `path` failed. A recent repository whose folder is gone is `missing`. A
 * folder that exists but didn't open is `notRepository`, with the repositories its direct
 * subfolders hold (`scanReposInRoot`, which also lists a workspace's members). Any other
 * failure, or one the checks can't tell, stays `failed`.
 */
export async function describeOpenFailure(client: RepoClient, path: string, error: unknown): Promise<OpenFailure> {
  const message = error instanceof Error ? error.message : String(error);
  const failed: OpenFailure = { kind: "failed", path, message };
  // A typed kind (permissions, a lost connection) already says what happened.
  if (path === "" || errorKind(error) !== "Other") return failed;
  const missing = await client.listMissingRecentRepos().then(
    (paths) => paths.includes(path),
    () => false,
  );
  if (missing) return { kind: "missing", path, message };
  const repos = await client.scanReposInRoot(path).then(
    (found) => found.filter(worthListing),
    () => null,
  );
  return repos === null ? failed : { kind: "notRepository", path, message, repos };
}

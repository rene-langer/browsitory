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

/** `path`'s parent folder and `path` as its scan lists it, or `null` for a root. */
function parentOf(path: string): { parent: string; child: string } | null {
  const trimmed = path.replace(/[\\/]+$/, "");
  const cut = Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\"));
  if (cut < 0 || cut === trimmed.length - 1) return null;
  // "/work" is in "/"; "C:\work" in "C:\" (a drive's root keeps its separator).
  const parent = cut === 0 || trimmed[cut - 1] === ":" ? trimmed.slice(0, cut + 1) : trimmed.slice(0, cut);
  return { parent, child: trimmed };
}

/** Separators and a trailing one aside, the same path. */
const samePath = (a: string, b: string) => a.replace(/\\/g, "/").replace(/\/+$/, "") === b.replace(/\\/g, "/").replace(/\/+$/, "");

/**
 * Whether `path` holds a `.git` entry: its parent's scan (`scanReposInRoot`, one level deep)
 * lists it then. `null` when that can't be told (a root, an unreadable parent).
 */
async function hasGitEntry(client: RepoClient, path: string): Promise<boolean | null> {
  const where = parentOf(path);
  if (where === null) return null;
  return client.scanReposInRoot(where.parent).then(
    (found) => found.some((repo) => samePath(repo, where.child)),
    () => null,
  );
}

/**
 * Tells why opening `path` failed. A recent repository whose folder is gone is `missing`. A
 * folder that exists but didn't open and holds no `.git` entry is `notRepository`, with the
 * repositories its direct subfolders hold (`scanReposInRoot`, which also lists a workspace's
 * members). A folder with a `.git` entry that Git refused (ownership, an unsupported format, a
 * broken HEAD), any other failure, or one the checks can't tell, stays `failed`: Git's message.
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
  if ((await hasGitEntry(client, path)) !== false) return failed;
  const repos = await client.scanReposInRoot(path).then(
    (found) => found.filter(worthListing),
    () => null,
  );
  return repos === null ? failed : { kind: "notRepository", path, message, repos };
}

/**
 * The VSCode panel's open tabs (#1128): each window keeps its own in its `workspaceState`, under
 * `OPEN_REPOS_KEY`. The desktop app keeps its tabs in `config.toml`, which the extension neither
 * reads nor writes for them. A window that saved none (a new window, or the first start after
 * the update that moved them here) restores no tabs; "Browsitory: Open" then opens the
 * workspace's repositories.
 *
 * The frontend's `RepoClient.OpenRepos` is duplicated here on purpose: the extension never
 * imports from `frontend/`. `frontendTypes.contract.test.ts` fails when the two drift (#1059).
 */

export interface OpenRepoEntry {
  path: string;
  workspaceId: string | null;
}

export type OpenRepos = { entries: OpenRepoEntry[]; activePath: string | null };

export const OPEN_REPOS_KEY = "openRepos";

export const NO_OPEN_REPOS: OpenRepos = { entries: [], activePath: null };

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function hasExactKeys(value: Record<string, unknown>, keys: readonly string[]): boolean {
  const own = Object.keys(value);
  return own.length === keys.length && keys.every((key) => own.includes(key));
}

function isOpenRepoEntry(value: unknown): value is OpenRepoEntry {
  return (
    isRecord(value) &&
    hasExactKeys(value, ["path", "workspaceId"]) &&
    typeof value["path"] === "string" &&
    (value["workspaceId"] === null || typeof value["workspaceId"] === "string")
  );
}

/** `value` as `OpenRepos` when it has exactly that shape; else `undefined`. */
export function parseOpenRepos(value: unknown): OpenRepos | undefined {
  if (!isRecord(value) || !hasExactKeys(value, ["entries", "activePath"])) return undefined;
  const { entries, activePath } = value;
  if (!Array.isArray(entries) || !entries.every(isOpenRepoEntry)) return undefined;
  if (activePath !== null && typeof activePath !== "string") return undefined;
  return {
    entries: entries.map(({ path, workspaceId }) => ({ path, workspaceId })),
    activePath,
  };
}

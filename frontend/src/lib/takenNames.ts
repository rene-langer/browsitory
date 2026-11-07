/**
 * Checks behind the optimistic creates and renames in `src/state/` (#408, #413): a name the
 * list already holds is refused before the backend call, so the optimistic update never lists
 * it twice (or, in the branch tree, replaces the existing entry until the backend refuses).
 *
 * A refusal must never be wrong, so these compare exactly, as git does for ref names (case
 * collisions on a case-insensitive drive are the backend's call). The backend stays
 * authoritative for everything else.
 */

/**
 * The listed ref name that `name` can't coexist with, or `null`: the same name, or one that
 * is a folder of the other (`feature` and `feature/x`), which git refuses for any two refs
 * of one kind (branches, tags).
 */
export function refNameClash(existing: readonly string[], name: string): string | null {
  for (const candidate of existing) {
    if (candidate === name || name.startsWith(`${candidate}/`) || candidate.startsWith(`${name}/`)) return candidate;
  }
  return null;
}

/** The message for a refused `name` of `kind` ("branch", "tag"), given what it clashes with. */
export function takenNameMessage(kind: string, name: string, clash: string): string {
  if (clash === name) return `A ${kind} named "${name}" already exists.`;
  return `"${name}" can't be used while a ${kind} named "${clash}" exists.`;
}

/**
 * `path` in a form two spellings of one folder share, for deciding whether to show an
 * optimistic worktree row: a relative path starts from `base` (the open worktree's folder, as
 * the backend resolves it), separators are `/`, `.` and `..` are resolved, trailing separators
 * dropped, and case is folded. This is deliberately looser than the backend's check (which
 * looks at the file system: symlinks, the drive's own case folding): it only decides whether
 * to skip the optimistic row, never whether to refuse, so a false match costs nothing but the
 * row appearing after the refresh instead of at once.
 */
export function comparableWorktreePath(path: string, base: string): string {
  const full = withBase(slashes(path), slashes(base));
  const parts: string[] = [];
  for (const part of full.split("/")) {
    if (part === "" || part === ".") continue;
    if (part === "..") parts.pop();
    else parts.push(part);
  }
  const leading = full.startsWith("//") ? "//" : full.startsWith("/") ? "/" : "";
  return `${leading}${parts.join("/")}`.toLowerCase();
}

// `/` separators, without the `\\?\` (`\\?\UNC\`) prefix the backend's canonical Windows paths
// carry, so `C:\work\wt` and `\\?\C:\work\wt` compare equal.
function slashes(path: string): string {
  const slashed = path.replace(/\\/g, "/");
  if (/^\/\/\?\/UNC\//i.test(slashed)) return `//${slashed.slice("//?/UNC/".length)}`;
  if (slashed.startsWith("//?/")) return slashed.slice("//?/".length);
  return slashed;
}

function withBase(path: string, base: string): string {
  const absolute = path.startsWith("/") || /^[A-Za-z]:\//.test(path);
  return absolute ? path : `${base}/${path}`;
}

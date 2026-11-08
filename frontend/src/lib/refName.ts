/**
 * Client-side mirror of the ref-name rules for branch names, so the New Branch form can say
 * why a name is wrong before the round trip. Returns a message, or `null` when the name is fine.
 * The backend stays authoritative; this only covers the common mistakes.
 */
export function branchNameProblem(name: string): string | null {
  if (name === "") return null;
  if (/\s/.test(name)) return "Branch names can't contain spaces.";
  if (/[~^:?*[\\]/.test(name)) return "Branch names can't contain ~ ^ : ? * [ or \\.";
  if (name.includes("..") || name.includes("@{")) return "Branch names can't contain .. or @{.";
  if (name.startsWith("-")) return "Branch names can't start with a dash.";
  if (name.startsWith("/") || name.endsWith("/") || name.includes("//")) return "Branch names can't start or end with / or contain //.";
  if (name.endsWith(".") || name.endsWith(".lock")) return "Branch names can't end with . or .lock.";
  if (name === "@") return "Branch name can't be just @.";
  return null;
}

/**
 * The same for tag names (Create tag), mirroring git-core's `validate_tag_name`: libgit2's tag
 * name rules (a valid `refs/tags/<name>` that doesn't start with a dash) plus no leading `+`.
 */
export function tagNameProblem(name: string): string | null {
  if (name === "") return null;
  if (/\s/.test(name)) return "Tag names can't contain spaces.";
  if (/[~^:?*[\\]/.test(name)) return "Tag names can't contain ~ ^ : ? * [ or \\.";
  if (name.includes("..") || name.includes("@{")) return "Tag names can't contain .. or @{.";
  if (name.startsWith("-") || name.startsWith("+")) return "Tag names can't start with - or +.";
  if (name.startsWith("/") || name.endsWith("/") || name.includes("//")) return "Tag names can't start or end with / or contain //.";
  if (name.endsWith(".")) return "Tag names can't end with a dot.";
  if (name.split("/").some((part) => part.startsWith(".") || part.endsWith(".lock"))) {
    return "No part of a tag name can start with . or end with .lock.";
  }
  if (name === "@") return "Tag name can't be just @.";
  return null;
}

/**
 * The same for remote names (Add remote, Edit remote): libgit2 takes a remote name when
 * `refs/remotes/<name>/…` is a valid ref name, so only the rules for a ref's inner folders are
 * mirrored here, not the branch-only ones (a leading dash, a trailing dot, a bare `@`).
 */
export function remoteNameProblem(name: string): string | null {
  if (name === "") return null;
  if (/\s/.test(name)) return "Remote names can't contain spaces.";
  if (/[~^:?*[\\]/.test(name)) return "Remote names can't contain ~ ^ : ? * [ or \\.";
  if (name.includes("..") || name.includes("@{")) return "Remote names can't contain .. or @{.";
  if (name.startsWith("/") || name.endsWith("/") || name.includes("//")) return "Remote names can't start or end with / or contain //.";
  if (name.split("/").some((part) => part.startsWith(".") || part.endsWith(".lock"))) {
    return "No part of a remote name can start with . or end with .lock.";
  }
  return null;
}

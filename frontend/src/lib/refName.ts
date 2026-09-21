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

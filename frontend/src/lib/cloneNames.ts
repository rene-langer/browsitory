/** Last path segment of a clone URL without `.git` — what `git clone` names the folder. */
export function deriveRepoName(url: string): string {
  const trimmed = url.trim().replace(/[\\/]+$/, "");
  const segment = trimmed.split(/[\\/:]/).pop() ?? "";
  return segment.replace(/\.git$/, "");
}

export function joinPath(parent: string, name: string): string {
  const separator = parent.includes("\\") && !parent.includes("/") ? "\\" : "/";
  const base = parent.replace(/[\\/]+$/, "");
  return `${base}${separator}${name}`;
}

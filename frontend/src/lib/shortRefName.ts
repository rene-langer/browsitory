/** `refs/remotes/origin/main` → `origin/main`, `refs/heads/main` → `main`, as the branch list shows them. */
export function shortRefName(ref: string): string {
  return ref.replace(/^refs\/(remotes|heads)\//, "");
}

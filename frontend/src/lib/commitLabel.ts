/**
 * The commit button's label (#651): what it commits and where, "Commit 3 files to main". The
 * count and the branch are each left out when unknown (`undefined`), the branch also on a
 * detached HEAD (`null`), and a count of 0 (a merge's commit with nothing left to stage).
 */
export function commitLabel(stagedCount: number | undefined, branchName: string | null | undefined): string {
  const files = stagedCount === undefined || stagedCount === 0 ? "" : ` ${stagedCount} ${stagedCount === 1 ? "file" : "files"}`;
  const branch = branchName == null ? "" : ` to ${branchName}`;
  return `Commit${files}${branch}`;
}

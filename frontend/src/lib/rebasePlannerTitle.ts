import { shortRefName } from "./shortRefName";

/**
 * The planner's title (#1015): "Rebase main onto feature" or "Rebase main onto 4c5beaf", naming the
 * target by its branch when `onto` is a branch's full ref name (a branch menu or the palette, #619)
 * and by its short id when it is a commit (the graph). Without the current branch's name it reads
 * "Rebase onto …".
 */
export function rebasePlannerTitle(onto: string, branchName: string | null = null): string {
  const target = /^refs\/(heads|remotes)\//.test(onto) ? shortRefName(onto) : onto.slice(0, 7);
  return branchName === null ? `Rebase onto ${target}` : `Rebase ${branchName} onto ${target}`;
}

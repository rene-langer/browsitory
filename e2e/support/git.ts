import fs from "node:fs";
import path from "node:path";
import { browser } from "@wdio/globals";
import { git } from "../../extension/e2e/src/support/git";
import { UI_TIMEOUT } from "./timeouts";

// The git CLI for preparing and checking fixture repositories (#500). The runner itself
// (`git`, `gitIn`, `gitWith`) is shared with the VSCode suite (#524) and lives in
// `extension/e2e/src/support/git.ts`, which can't import `@wdio/globals`; this file adds the
// helpers that wait through the WebDriver session.
export { git, gitIn, gitWith, type GitOptions } from "../../extension/e2e/src/support/git";

/**
 * Waits until the app's merge commit has landed in `repoPath`: HEAD has two parents and
 * `.git/MERGE_HEAD` is gone (#182). Waiting on a history row instead isn't enough — a row
 * matching the branch name (the sidebar's branch row) exists before the commit does, so the
 * next step could run while the merge was still in progress.
 */
export async function waitForMergeCommit(repoPath: string, timeout = UI_TIMEOUT): Promise<void> {
  await browser.waitUntil(
    () => {
      const parents = git(repoPath, "rev-list", "--parents", "-n", "1", "HEAD").split(" ");
      return parents.length === 3 && !fs.existsSync(path.join(repoPath, ".git", "MERGE_HEAD"));
    },
    {
      timeout,
      timeoutMsg: "the merge commit did not land (HEAD has no two parents, or .git/MERGE_HEAD is still there)",
    },
  );
}

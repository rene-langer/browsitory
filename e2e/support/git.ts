import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { browser } from "@wdio/globals";

/**
 * Waits until the app's merge commit has landed in `repoPath`: HEAD has two parents and
 * `.git/MERGE_HEAD` is gone (#182). Waiting on a history row instead isn't enough — a row
 * matching the branch name (the sidebar's branch row) exists before the commit does, so the
 * next step could run while the merge was still in progress.
 */
export async function waitForMergeCommit(repoPath: string, timeout = 10000): Promise<void> {
  await browser.waitUntil(
    () => {
      const parents = execFileSync("git", ["rev-list", "--parents", "-n", "1", "HEAD"], {
        cwd: repoPath,
        encoding: "utf8",
      })
        .trim()
        .split(" ");
      return parents.length === 3 && !fs.existsSync(path.join(repoPath, ".git", "MERGE_HEAD"));
    },
    {
      timeout,
      timeoutMsg: "the merge commit did not land (HEAD has no two parents, or .git/MERGE_HEAD is still there)",
    },
  );
}

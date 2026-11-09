import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { browser } from "@wdio/globals";
import { UI_TIMEOUT } from "./timeouts";

// The git CLI for preparing and checking fixture repositories (#500). Every spec runs git
// through these, so a change to how it is run (environment, line endings, error output) is
// made once. The child inherits the runner's environment, so a run that isolates HOME and git's
// global config (`screenshots/demoRepo.ts`'s `isolateHome`) isolates these calls too.

export interface GitOptions {
  /**
   * Trim the output (the default). `false` returns git's output exactly, for a spec that compares
   * file contents or porcelain output whose leading or trailing whitespace matters.
   */
  trim?: boolean;
  /**
   * `"ignore"` discards git's output (the result is then `""`), for a call that may fail and
   * shouldn't clutter the log. By default stdout is returned, and stderr goes to the runner's
   * stderr and, when git fails, into the thrown error's message.
   */
  stdio?: "ignore";
}

/** Runs git in `cwd` with `options` and returns its output (trimmed unless `trim: false`). */
export function gitWith(cwd: string, args: string[], options: GitOptions = {}): string {
  // No `stdio` unless asked: only then does Node copy the child's stderr to ours (and still put
  // it in the thrown error's message).
  const output =
    options.stdio === undefined
      ? execFileSync("git", args, { cwd, encoding: "utf8" })
      : (execFileSync("git", args, { cwd, encoding: "utf8", stdio: options.stdio }) ?? "");
  return options.trim === false ? output : output.trim();
}

/** Runs git in `cwd` and returns its trimmed output; throws (with git's stderr) when it fails. */
export function git(cwd: string, ...args: string[]): string {
  return gitWith(cwd, args);
}

/** `git` bound to one repository: `const git = gitIn(E2E_REPO_PATH); git("add", file)`. */
export function gitIn(cwd: string, options: GitOptions = {}): (...args: string[]) => string {
  return (...args) => gitWith(cwd, args, options);
}

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

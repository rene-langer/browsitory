// The git CLI for preparing and checking fixture repositories, shared by both E2E suites
// (#500, #524): the VSCode specs import it from here, and the Tauri suite's `e2e/support/git.ts`
// re-exports it by relative path. Every spec runs git through these, so a change to how it is
// run (environment, line endings, error output) is made once. Like `fixtureRepo.ts`, it lives
// here because this package compiles with `rootDir: "src"`; keep it dependency-free (Node
// built-ins only) and valid under both packages' tsconfigs. The child inherits the runner's
// environment, so a run that isolates HOME and git's global config isolates these calls too.
import { execFileSync } from "node:child_process";

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

/** `git` bound to one repository: `const git = gitIn(repoPath); git("add", file)`. */
export function gitIn(cwd: string, options: GitOptions = {}): (...args: string[]) => string {
  return (...args) => gitWith(cwd, args, options);
}

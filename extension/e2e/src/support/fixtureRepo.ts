// Fixture repository set-up shared by both E2E suites (#500): the VSCode suite's
// `runTests.ts` and the Tauri suite's `e2e/wdio.conf.ts`, which imports this file by relative
// path. It lives here rather than under `e2e/` because this package compiles with
// `rootDir: "src"`, so it can't import a file outside `src/`, while `e2e/` runs its TypeScript
// through tsx and type-checks without a `rootDir`. Keep it dependency-free (Node built-ins
// only) and valid under both packages' tsconfigs: `e2e/` installs no packages of this one.
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

/**
 * The identity every fixture repository commits as. Matches the "Test User"/"test@example.com"
 * convention of `crates/git-core/tests/common/mod.rs`'s `init_repo()` helper.
 */
export const TEST_USER_NAME = "Test User";
export const TEST_USER_EMAIL = "test@example.com";

function run(repoPath: string, args: string[]): void {
  execFileSync("git", args, { cwd: repoPath, stdio: "inherit" });
}

/**
 * Sets the test identity in `repoPath`'s local config. Local (not global): GitHub-hosted runners
 * ship no global git identity, and `git_core::commit::commit`'s `repo.signature()` call errors
 * without one (the failure `crates/git-core/tests/stage_commit.rs`'s
 * `commit_without_a_configured_identity_returns_an_error` covers). Setting it locally, rather
 * than relying on the host's `~/.gitconfig`, also keeps the fixture hermetic for local runs.
 */
export function setTestIdentity(repoPath: string): void {
  run(repoPath, ["config", "user.name", TEST_USER_NAME]);
  run(repoPath, ["config", "user.email", TEST_USER_EMAIL]);
}

/**
 * Creates an empty repository at `repoPath` (removing whatever was there) with the test
 * identity and an uncommitted `README.md` holding `readme`.
 */
export function initFixtureRepo(repoPath: string, readme: string): void {
  fs.rmSync(repoPath, { recursive: true, force: true });
  fs.mkdirSync(repoPath, { recursive: true });
  run(repoPath, ["init"]);
  setTestIdentity(repoPath);
  fs.writeFileSync(path.join(repoPath, "README.md"), readme);
}

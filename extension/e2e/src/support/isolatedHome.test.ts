// Unit tests for the harness's own helpers (node:test, not the VSCode suite):
//   pnpm --ignore-workspace test:unit
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, test } from "node:test";
import { initFixtureRepo } from "./fixtureRepo";
import { git } from "./git";
import { isolateHome } from "./isolatedHome";

let scratch = "";
let savedEnv: NodeJS.ProcessEnv = {};

// A developer's setup the E2E runs must not see (#1112): a global git config that signs every
// commit with a gpg that isn't there and names its own default branch, an SSH agent, a GnuPG
// home, and a GIT_DIR inherited from a hook.
beforeEach(() => {
  savedEnv = { ...process.env };
  scratch = fs.mkdtempSync(path.join(os.tmpdir(), "browsitory-isolated-home-test-"));
  const developerHome = path.join(scratch, "developer");
  fs.mkdirSync(developerHome);
  fs.writeFileSync(
    path.join(developerHome, ".gitconfig"),
    "[commit]\n\tgpgSign = true\n[gpg]\n\tprogram = browsitory-no-such-gpg\n" +
      "[init]\n\tdefaultBranch = from-the-developer\n",
  );
  process.env["HOME"] = developerHome;
  process.env["XDG_CONFIG_HOME"] = path.join(developerHome, ".config");
  process.env["GIT_CONFIG_GLOBAL"] = path.join(developerHome, ".gitconfig");
  process.env["GNUPGHOME"] = path.join(developerHome, ".gnupg");
  process.env["SSH_AUTH_SOCK"] = path.join(developerHome, "agent.sock");
  process.env["SSH_AGENT_PID"] = "1";
  process.env["GIT_DIR"] = path.join(developerHome, "not-the-fixture");
});

afterEach(() => {
  for (const name of Object.keys(process.env)) {
    if (!(name in savedEnv)) delete process.env[name];
  }
  Object.assign(process.env, savedEnv);
  fs.rmSync(scratch, { recursive: true, force: true });
});

test("isolateHome points HOME, the XDG dirs, git's global config and GnuPG into a fresh root", () => {
  const root = path.join(scratch, "home");
  fs.mkdirSync(root);
  fs.writeFileSync(path.join(root, "left-over"), "from an earlier run\n");

  const vars = isolateHome(root);

  assert.deepEqual(vars, {
    HOME: root,
    XDG_CONFIG_HOME: path.join(root, ".config"),
    XDG_CACHE_HOME: path.join(root, ".cache"),
    XDG_DATA_HOME: path.join(root, ".local", "share"),
    XDG_STATE_HOME: path.join(root, ".local", "state"),
    GIT_CONFIG_GLOBAL: path.join(root, ".gitconfig"),
    GIT_CONFIG_NOSYSTEM: "1",
    GNUPGHOME: path.join(root, ".gnupg"),
  });
  for (const [name, value] of Object.entries(vars)) assert.equal(process.env[name], value, name);
  assert.equal(fs.existsSync(path.join(root, "left-over")), false);
  for (const dir of [vars.XDG_CONFIG_HOME, vars.XDG_CACHE_HOME, vars.XDG_DATA_HOME, vars.XDG_STATE_HOME]) {
    assert.ok(fs.statSync(dir).isDirectory(), dir);
  }
  assert.equal(fs.readFileSync(vars.GIT_CONFIG_GLOBAL, "utf8"), "");
  if (process.platform !== "win32") {
    // gpg refuses a GnuPG home others can read.
    assert.equal(fs.statSync(vars.GNUPGHOME).mode & 0o777, 0o700);
  }
});

test("isolateHome drops the developer's SSH agent and every inherited GIT_* variable", () => {
  process.env["GIT_CONFIG_PARAMETERS"] = "'commit.gpgsign'='true'";
  process.env["GIT_AUTHOR_NAME"] = "Developer";

  isolateHome(path.join(scratch, "home"));

  for (const name of ["SSH_AUTH_SOCK", "SSH_AGENT_PID", "GIT_DIR", "GIT_CONFIG_PARAMETERS", "GIT_AUTHOR_NAME"]) {
    assert.equal(process.env[name], undefined, name);
  }
});

test("after isolateHome, fixture git commands see none of the developer's git config", () => {
  isolateHome(path.join(scratch, "home"));
  const repo = path.join(scratch, "repo");

  initFixtureRepo(repo, "fixture\n");
  git(repo, "add", "README.md");
  git(repo, "commit", "-m", "base");

  assert.notEqual(git(repo, "symbolic-ref", "--short", "HEAD"), "from-the-developer");
  const scopes = git(repo, "config", "--list", "--show-scope")
    .split("\n")
    .map((line) => line.split("\t")[0]);
  assert.deepEqual([...new Set(scopes)], ["local"]);
});

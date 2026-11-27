// A throwaway home for an E2E run (#1112), shared by the Tauri suite (`e2e/wdio.conf.ts`, which
// imports it by relative path, as it does `fixtureRepo.ts`), its screenshot runs, the VSCode
// suite (`runTests.ts`) and the VSIX smoke test (`runVsixSmoke.ts`). It does for them what
// `crates/test-support`'s `hermetic_git!` does for the Rust tests: the fixture git commands, the
// app or VSCode, and the sidecar (whose libgit2 reads `$HOME/.gitconfig`) never see the
// developer's global git config (`commit.gpgSign`, `core.hooksPath`, `init.templateDir`, ...),
// GnuPG home or SSH agent. libgit2 still reads the system config (`/etc/gitconfig`): it has no
// counterpart of `GIT_CONFIG_NOSYSTEM`.
//
// Keep it dependency-free (Node built-ins only) and valid under both packages' tsconfigs.
import fs from "node:fs";
import path from "node:path";

/** The variables `isolateHome` sets. */
export interface IsolatedHome {
  HOME: string;
  XDG_CONFIG_HOME: string;
  XDG_CACHE_HOME: string;
  XDG_DATA_HOME: string;
  XDG_STATE_HOME: string;
  GIT_CONFIG_GLOBAL: string;
  GIT_CONFIG_NOSYSTEM: string;
  GNUPGHOME: string;
}

/** What a session's SSH agent hands its processes; `ssh-keygen -Y sign` would ask it first. */
const SSH_AGENT_VARIABLES = ["SSH_AUTH_SOCK", "SSH_AGENT_PID"];

/**
 * Empties `root` and makes it the home of this process and of everything it starts from now on:
 * HOME and the XDG dirs, an empty `GIT_CONFIG_GLOBAL`, `GIT_CONFIG_NOSYSTEM=1` and an empty
 * GnuPG home (mode 0700). Removes the SSH agent variables and every inherited `GIT_*` variable
 * (a hook's `GIT_DIR`, `GIT_CONFIG_PARAMETERS`, `GIT_AUTHOR_*`, ...). Call it before the first
 * git command, and resolve any path under the real home (such as `~/.cargo/bin`) before it.
 */
export function isolateHome(root: string): IsolatedHome {
  const vars: IsolatedHome = {
    HOME: root,
    XDG_CONFIG_HOME: path.join(root, ".config"),
    XDG_CACHE_HOME: path.join(root, ".cache"),
    XDG_DATA_HOME: path.join(root, ".local", "share"),
    XDG_STATE_HOME: path.join(root, ".local", "state"),
    GIT_CONFIG_GLOBAL: path.join(root, ".gitconfig"),
    GIT_CONFIG_NOSYSTEM: "1",
    GNUPGHOME: path.join(root, ".gnupg"),
  };
  fs.rmSync(root, { recursive: true, force: true });
  for (const dir of [vars.XDG_CONFIG_HOME, vars.XDG_CACHE_HOME, vars.XDG_DATA_HOME, vars.XDG_STATE_HOME]) {
    fs.mkdirSync(dir, { recursive: true });
  }
  // gpg refuses a GnuPG home others can read.
  fs.mkdirSync(vars.GNUPGHOME, { mode: 0o700 });
  fs.writeFileSync(vars.GIT_CONFIG_GLOBAL, "");

  for (const name of Object.keys(process.env)) {
    if (name.toUpperCase().startsWith("GIT_") || SSH_AGENT_VARIABLES.includes(name)) delete process.env[name];
  }
  Object.assign(process.env, vars);
  return vars;
}

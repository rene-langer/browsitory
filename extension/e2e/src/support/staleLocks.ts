// Stale git lock files between specs (#961). Every spec of the VSCode suite works in the one
// workspace repository, so a flow that leaves `.git/index.lock` behind (a sidecar stopped while
// it wrote the index, a git process that died) made every later spec fail on it. The suite
// (`suite/index.ts`) calls `clearStaleLocks` after each spec and reports what it removed.
// Keep it dependency-free (Node built-ins only).
import fs from "node:fs";
import path from "node:path";

export interface ClearStaleLocksOptions {
  /** How long a lock may still be held (a git process finishing) before it counts as stale. */
  graceMs: number;
  /** How often to look again during the grace period. */
  pollMs: number;
  /** Injected by the unit tests; a real timer otherwise. */
  sleep?: (ms: number) => Promise<void>;
}

const realSleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

/**
 * The lock files git takes in `gitDir`: those directly in it (`index.lock`, `HEAD.lock`,
 * `config.lock`, `packed-refs.lock`, ...) and under `refs/`, relative to `gitDir` and sorted.
 * Browsitory's own `browsitory/state.lock` (an OS lock on a file that stays) isn't one.
 */
function findLocks(gitDir: string): string[] {
  const found: string[] = [];
  const visit = (relative: string, recurse: boolean) => {
    let entries: fs.Dirent[];
    try {
      entries = fs.readdirSync(path.join(gitDir, relative), { withFileTypes: true });
    } catch {
      return;
    }
    for (const entry of entries) {
      const child = path.join(relative, entry.name);
      if (entry.isDirectory()) {
        if (recurse) visit(child, true);
      } else if (entry.name.endsWith(".lock")) {
        found.push(child);
      }
    }
  };
  visit("", false);
  visit("refs", true);
  return found.sort();
}

/**
 * Waits up to `graceMs` for git's lock files in `gitDir` to go away, then removes those still
 * there and returns them (relative to `gitDir`); `[]` when there were none or their holders
 * released them in time.
 */
export async function clearStaleLocks(gitDir: string, options: ClearStaleLocksOptions): Promise<string[]> {
  const sleep = options.sleep ?? realSleep;
  let waited = 0;
  let locks = findLocks(gitDir);
  while (locks.length > 0 && waited < options.graceMs) {
    await sleep(options.pollMs);
    waited += options.pollMs;
    locks = findLocks(gitDir);
  }
  for (const lock of locks) fs.rmSync(path.join(gitDir, lock), { force: true });
  return locks;
}

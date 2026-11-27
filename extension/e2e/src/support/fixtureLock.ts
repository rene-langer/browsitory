// One E2E run at a time per fixture set (#1144). Both suites keep their fixtures at fixed paths
// in the temp folder (the Tauri suite's repository path is built into the frontend through
// `VITE_E2E_REPO_PATH`), and each run deletes and rebuilds them first. A second run started
// while one is going on (another worktree, another agent) would delete the live run's
// fixtures, so each run takes this lock before it deletes anything. Shared by `runTests.ts` and
// `e2e/wdio.conf.ts` (by relative path). Keep it dependency-free (Node built-ins only) and
// valid under both packages' tsconfigs.
import fs from "node:fs";
import path from "node:path";

const OWNER_FILE = "owner.json";

interface Owner {
  pid: number;
  cwd: string;
  started: string;
}

function readOwner(lockDir: string): Owner | undefined {
  try {
    const owner = JSON.parse(fs.readFileSync(path.join(lockDir, OWNER_FILE), "utf8")) as Partial<Owner>;
    return typeof owner.pid === "number" ? (owner as Owner) : undefined;
  } catch {
    return undefined;
  }
}

function isAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    // EPERM: it exists, but belongs to another user.
    return (error as NodeJS.ErrnoException).code === "EPERM";
  }
}

/**
 * Takes the lock `lockDir` for this process, or throws when a live run holds it, naming that run
 * (pid, folder, start time), followed by `hint`. A lock whose process is gone, or that has no
 * owner, is taken over. Returns `release`, which removes the lock while this process still
 * holds it; it is safe to call more than once.
 *
 * The lock folder appears whole or not at all: the owner file is written into a private folder
 * that is then renamed to `lockDir`, which fails while `lockDir` holds an owner.
 */
export function acquireFixtureLock(lockDir: string, hint = ""): () => void {
  const mine: Owner = { pid: process.pid, cwd: process.cwd(), started: new Date().toISOString() };
  const staging = `${lockDir}.${process.pid}.tmp`;
  fs.rmSync(staging, { recursive: true, force: true });
  fs.mkdirSync(staging, { recursive: true });
  fs.writeFileSync(path.join(staging, OWNER_FILE), JSON.stringify(mine));
  try {
    for (let attempt = 0; ; attempt++) {
      try {
        fs.renameSync(staging, lockDir);
        break;
      } catch (error) {
        const code = (error as NodeJS.ErrnoException).code;
        if (!["EEXIST", "ENOTEMPTY", "EPERM", "EACCES"].includes(code ?? "") || attempt >= 2) throw error;
      }
      const owner = readOwner(lockDir);
      if (owner && isAlive(owner.pid)) {
        throw new Error(
          `Another E2E run is using these fixtures (pid ${owner.pid}, started ${owner.started}, ` +
            `in ${owner.cwd}); its lock is ${lockDir}.` +
            (hint ? ` ${hint}` : ""),
        );
      }
      console.warn(`[e2e] taking over the lock ${lockDir} from a run that is gone (pid ${owner?.pid ?? "unknown"})`);
      fs.rmSync(lockDir, { recursive: true, force: true });
    }
  } finally {
    fs.rmSync(staging, { recursive: true, force: true });
  }

  return () => {
    if (readOwner(lockDir)?.pid === process.pid) fs.rmSync(lockDir, { recursive: true, force: true });
  };
}

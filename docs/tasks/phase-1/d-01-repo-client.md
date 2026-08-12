# Task 1.D.01: Extend `RepoClient` for log/diff/staging/commit/repo-picking

## Goal

Grow `frontend/src/ipc/RepoClient.ts`'s interface and types, and `tauriRepoClient.ts`'s
implementation, to cover every Tauri command Tasks 1.C.01/1.C.02 added. This is the one and only
place the frontend's data shapes for commits/diffs are defined — every later frontend task reads
its types from here.

## Depends on

1.C.01 (`get_log`/`get_working_diff`/`get_commit_diff`/`get_commit_files`/`stage_file`/
`unstage_file`/`commit` Tauri commands), 1.C.02 (`pick_repo_folder`/`list_recent_repos`).

## Interfaces produced

`frontend/src/ipc/RepoClient.ts` (full new contents — extends the existing file, keep
`StatusKind`/`StatusEntry` unchanged):
```ts
export type StatusKind = "New" | "Modified" | "Deleted" | "Renamed" | "TypeChange";

export interface StatusEntry {
  path: string;
  staged: boolean;
  kind: StatusKind;
}

export interface CommitInfo {
  id: string;
  shortId: string;
  summary: string;
  authorName: string;
  authorEmail: string;
  timestamp: number;
}

export type DiffLineOrigin = "Add" | "Remove" | "Context";

export interface DiffLine {
  origin: DiffLineOrigin;
  content: string;
}

export interface DiffHunk {
  oldStart: number;
  oldLines: number;
  newStart: number;
  newLines: number;
  lines: DiffLine[];
}

export interface RepoClient {
  pickRepoFolder(): Promise<string | null>;
  listRecentRepos(): Promise<string[]>;
  openRepo(path: string): Promise<void>;
  getStatus(): Promise<StatusEntry[]>;
  getLog(limit: number): Promise<CommitInfo[]>;
  getWorkingDiff(path: string, staged: boolean): Promise<DiffHunk[]>;
  getCommitDiff(commitId: string, path: string): Promise<DiffHunk[]>;
  getCommitFiles(commitId: string): Promise<string[]>;
  stageFile(path: string): Promise<void>;
  unstageFile(path: string): Promise<void>;
  commit(message: string): Promise<void>;
}
```

`frontend/src/ipc/tauriRepoClient.ts` (full new contents):
```ts
import { invoke } from "@tauri-apps/api/core";
import type {
  CommitInfo,
  DiffHunk,
  RepoClient,
  StatusEntry,
} from "./RepoClient";

export const tauriRepoClient: RepoClient = {
  pickRepoFolder: () => invoke<string | null>("pick_repo_folder"),
  listRecentRepos: () => invoke<string[]>("list_recent_repos"),
  openRepo: (path: string) => invoke("open_repo", { path }),
  getStatus: () => invoke<StatusEntry[]>("get_status"),
  getLog: (limit: number) => invoke<CommitInfo[]>("get_log", { limit }),
  getWorkingDiff: (path: string, staged: boolean) =>
    invoke<DiffHunk[]>("get_working_diff", { path, staged }),
  getCommitDiff: (commitId: string, path: string) =>
    invoke<DiffHunk[]>("get_commit_diff", { commitId, path }),
  getCommitFiles: (commitId: string) =>
    invoke<string[]>("get_commit_files", { commitId }),
  stageFile: (path: string) => invoke("stage_file", { path }),
  unstageFile: (path: string) => invoke("unstage_file", { path }),
  commit: (message: string) => invoke("commit", { message }),
};
```
Tauri's `invoke` auto-converts a JS object's keys to the Rust command's parameter names (both
sides already agree on `camelCase` JS / `snake_case` Rust per Tauri's own convention — this
matches every parameter name Task 1.C.01/1.C.02 used in their `#[tauri::command]` function
signatures, e.g. `commitId` here ↔ `commit_id` there).

## Implementation notes

No behavior beyond direct pass-through — this task is a mechanical extension of an existing,
established pattern (`openRepo`/`getStatus` already do exactly this). The main risk is a typo in
a command name or parameter name breaking the IPC call silently until runtime; double-check every
`invoke("...")` string against the exact `#[tauri::command]` function name from 1.C.01/1.C.02,
and every object key against that function's exact parameter names.

## TDD requirement

No test file for this task. `RepoClient.ts` is a pure type/interface definition (nothing to
test). `tauriRepoClient.ts` is the one file allowed to import `@tauri-apps/api` — it's the
transport boundary itself, and per `CLAUDE.md`'s testing conventions, frontend tests mock
`RepoClient`, never `@tauri-apps/api` directly, so there's nothing to meaningfully unit-test
here (mirrors 1.C.01's note that thin pass-through Tauri commands don't get dedicated tests).
This boundary is exercised for real by Task 1.F.02's E2E test instead.

What this task must still verify: `pnpm build` (runs `tsc -b` first) succeeds — this is the
actual check that every method in `tauriRepoClient.ts` satisfies the `RepoClient` interface
with matching types.

## Acceptance criteria

- [ ] `cd frontend && pnpm build` succeeds (TypeScript compiles clean).
- [ ] `pnpm lint` clean (confirms `tauriRepoClient.ts`'s `@tauri-apps/api` import doesn't trip
      the `no-restricted-imports` rule — it shouldn't, since that rule only scopes
      `src/components/**` and `src/state/**`, but confirm rather than assume).
- [ ] `pnpm test -- --run` still passes (existing `StatusView.test.tsx` unaffected).
- [ ] Commit: `git add frontend/src/ipc/RepoClient.ts frontend/src/ipc/tauriRepoClient.ts && git commit -m "feat(frontend): extend RepoClient for log, diff, staging, commit, repo picking"`.

## Out of scope

A `vscodeRepoClient.ts` second implementation — not until a VSCode extension is actually built
(explicitly out of scope for the whole project at this stage, per the original architecture
spec).

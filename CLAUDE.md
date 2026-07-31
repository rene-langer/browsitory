# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
npm run dev            # Vite dev server (http://localhost:5173)
npm run build           # tsc + vite build (production)
npm run lint             # eslint . --ext .ts,.tsx
npm run type-check      # tsc --noEmit
npm test                # vitest run (single run, used in CI)
npm run test:watch      # vitest (watch mode)
npm run test:coverage   # vitest run --coverage
npm run format           # prettier --write
```

Run a single test file or test: `npx vitest run src/services/git.test.ts` or
`npx vitest run -t "reports a modified tracked file"`.

CI (`.github/workflows/ci.yml`) runs, in order: `npm ci`, `lint`, `type-check`, `build`, `test`.
All four must pass.

## Project status

Phase 1 (MVP) is implemented: opening a local repository, commit history, diff viewing,
staging/unstaging, and committing. Phase 2 is also implemented: branch management (create/
delete/rename/switch), stash (push/apply/pop/drop/list), merge with conflict resolution,
interactive rebase (pick/drop only — no reword/squash), a blame viewer, and a multi-branch
commit graph view. Push/pull to a remote is still not implemented. See `docs/PROJECT_SETUP.md`
for the phase roadmap and `docs/ARCHITECTURE.md` for the full tech-stack rationale.

### Phase 2 additions worth knowing before touching them

- **Merge conflicts**: `mergeBranch`/`cherryPick` (the latter used by rebase) both use
  `abortOnConflict: false`, which writes conflict markers to the working tree and unmerged
  stage-1/2/3 (base/ours/theirs) index entries, then throws `MergeConflictError` — without
  moving the branch ref. `getConflictDiff()` in `git.ts` reads those stage-2/3 entries
  directly via isomorphic-git's `isomorphic-git/managers` (`GitIndexManager`) and
  `isomorphic-git/models` (`FileSystem`) subpath exports — these are legitimate, declared
  entries in the package's own `exports` map, not undocumented internals; verify against
  `node_modules/isomorphic-git/package.json` if this ever looks broken after an upgrade.
- **Interactive rebase** (`src/services/rebase.ts`) is hand-rolled from `cherryPick` +
  `checkout`/`branch` — isomorphic-git has no native `rebase` command. State is persisted to
  a `.git/browsitory-rebase.json` sidecar (via the same injected `fs`) so a paused rebase
  survives a reload. The branch ref is never touched until the rebase fully completes —
  every step runs in detached HEAD, which is what makes `abortRebase` a trivial checkout
  back to the original branch.
- **Blame** (`src/services/blame.ts`) is also hand-rolled — no native `blame` command either.
  It walks `git.log({ filepath })` oldest-to-newest, re-diffing the running line-attribution
  model against each commit with the `diff` package's `diffLines`. Both diff inputs must end
  with a trailing `\n` or jsdiff misreports an unchanged last line as a remove+add whenever
  old/new line counts differ — a real bug hit once, see the comment in `blame.ts`.
- **Branch/stash** (`listAllBranches`, `createBranch`, `switchBranch`, `listStashes`, etc. in
  `git.ts`) are thin wrappers — isomorphic-git supports all of this natively. One gotcha:
  `git.stash({op:'list'})` returns `string[]` formatted as `"stash@{N}: <message>"` at
  runtime despite its looser declared type; parse it, don't expect objects.
- **`Repository.tsx`** has no per-feature routing (branch/merge/rebase/blame/graph are all
  inline state in that one page, same precedent as Phase 1's diff viewing) — it composes
  `gitStore`, `rebaseStore`, and `repositoryStore` together and switches between a History
  view (staging/stash/commit sidebar + diff/blame/merge/rebase content) and a Graph view.

## Architecture

This is a client-only PWA (React + TypeScript + Vite) — there is no backend in Phase 1. Git
repositories are opened directly from the user's disk via the browser's **File System Access
API** (`showDirectoryPicker`), which is why the app currently only works in Chromium-based
browsers (Chrome/Edge/Opera).

### The fs adapter is the load-bearing piece

`src/services/fsaGitFs.ts` bridges the File System Access API to the `fs.promises`-shaped
object isomorphic-git expects (`readFile`, `writeFile`, `unlink`, `readdir`, `mkdir`, `rmdir`,
`stat`, `lstat`, `rename`). This is hand-written, not a dependency — there is no MIT-licensed
off-the-shelf bridge; the closest option (ZenFS's `@zenfs/dom` `WebAccess` backend) is
LGPL-3.0 and was rejected on license grounds (see "License policy" below). Key gotchas already
worked through here, worth knowing before touching this file:

- `mkdir` is fully recursive and idempotent by design, independent of whatever
  parent-directory-creation behavior isomorphic-git's own `FileSystem` wrapper does or doesn't
  do internally — don't rely on isomorphic-git to create parent dirs for you.
- Errors must carry Node-style `.code` values (`ENOENT`, `ENOTDIR`, `EISDIR`, ...) because
  isomorphic-git branches on `err.code`, not on `DOMException.name`.
- No symlink support (`lstat` is aliased to `stat`).

### git.ts is dependency-injected on purpose

`src/services/git.ts` wraps isomorphic-git calls (`statusMatrix`, `log`, `walk`, `add`,
`commit`, ...) as functions taking `(fs, dir, ...)` explicitly rather than reading a module
singleton. This is what makes `src/services/git.test.ts` possible without a browser: tests
pass Node's real `fs` module against real temp-directory repos
(`fs.mkdtempSync` + `isomorphic-git`'s own `init`), exercising the *exact same* code paths the
browser uses with `fsaGitFs`. When adding a new git operation, keep this shape.

Two isomorphic-git behaviors that are easy to get wrong here (both already hit and fixed once
— see git history / commit messages if you need the "why"):
- `git.walk`'s `map()` callback: returning `null` for a tree entry tells isomorphic-git to
  **prune that subtree** (skip walking its children). Returning `undefined` just excludes that
  entry from the results without pruning. Always use `undefined`, never `null`, unless you
  intend to prune.
- The `STAGE()` walker's entries have a **no-op `content()`** ("Cannot get content for an
  index entry" — this is intentional upstream). To read staged file content, fetch the oid via
  `entry.oid()` and call `git.readBlob({ fs, dir, oid })` instead. `readEntryContent()` in
  `git.ts` already does this fallback — reuse it rather than calling `entry.content()`
  directly on new code paths.

### State flow

`src/store/repositoryStore.ts` (Zustand) owns the list of opened repositories
(persisted via `src/services/repositoryRegistry.ts`, an `idb-keyval` wrapper that stores
`FileSystemDirectoryHandle`s directly in IndexedDB) and the currently-open repo's `{ fs, dir }`
pair. `src/store/gitStore.ts` owns commit/status/diff state and takes an `OpenRepository`
(`{ fs, dir }`) as an explicit argument on every action rather than reading
`repositoryStore` internally — this keeps it mockable in isolation (see `gitStore.test.ts`).

Pages (`src/pages/Repository.tsx`) compose both stores; there is no separate routed page for
commit diffs — commit and staged/unstaged file diffs are both rendered inline via
`selectedDiff` in `gitStore`, not through nested routes. If you're tempted to add a
`/repo/:id/commit/:hash` route, know that this was tried and deliberately backed out because
staged/unstaged file diffs have no natural URL of their own in this design.

### Browser permission handling

`FileSystemDirectoryHandle`s can be restored from IndexedDB across reloads, but the browser
can silently revoke write permission on them. Any code path that resumes a stored handle must
call `verifyPermission()` (in `fsaGitFs.ts`) before use — see `openHandle()` in
`repositoryStore.ts` for the pattern.

## License policy

MIT-only. Every dependency must be MIT or a compatible permissive license (Apache-2.0, ISC,
BSD, MIT-0). GPL/AGPL/LGPL/SSPL and similar copyleft licenses are disallowed — this has
already ruled out at least one otherwise-convenient library (ZenFS, LGPL-3.0). Verify with
`npm view <package> license` before adding a dependency, and record it in
`docs/LICENSE_COMPLIANCE.md`.

## Testing conventions

- Vitest config is in `vitest.config.ts` (separate from `vite.config.ts` so the PWA plugin
  never loads during tests).
- Browser-only APIs (`FileSystemDirectoryHandle`, `showDirectoryPicker`) are faked, not
  mocked-at-the-module-level, where practical: `src/test/fakeDirectoryHandle.ts` is a minimal
  in-memory implementation used to unit-test `fsaGitFs.ts` directly.
- Service-layer tests (`git.test.ts`) use real temp-directory git repos via Node's `fs`, not
  mocks — prefer this pattern for new git.ts functions over mocking isomorphic-git.
- Store tests mock the service layer (`vi.mock('@services/git', ...)`) since stores are
  thin orchestration over services.
- Path aliases (`@components`, `@pages`, `@hooks`, `@store`, `@lib`, `@services`) are defined
  in both `tsconfig.json` and `vitest.config.ts` — keep them in sync if you add a new one.

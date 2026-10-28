# Security and Bug Fix Wave — Design

Date: 2026-10-28. Branch: `fix/security-bug-wave` (from `main` 0632de19).
Plan: `docs/superpowers/plans/2026-10-28-security-bug-wave.md`.

Issues covered:

| # | Kind | Summary |
|---|---|---|
| #114 | security | Forced checkouts write a blob **through** a worktree symlink (leaf typechange or symlinked ancestor directory), possibly outside the repo |
| #138 | security, low | Work-tree image preview: directory-symlink swap race reads files outside the repo |
| #110 | robustness | Signing program has no timeout / cancel; a hung signer blocks the repo worker forever |
| #116 | bug | Per-file diff for path `d` also includes hunks of `d/…` |
| #117 | bug | Working-tree compare shows no hunks for a conflicted file |
| #137 | bug | Staged rename of an image previews as Added (`StatusEntry` has no `oldPath`) |

Decisions that need the user are collected in [Product decisions](#product-decisions). The plan
implements the **recommended** option of each; a different answer changes only the tasks named
there.

---

## 1. #114 — forced checkouts write through symlinks

### 1.1 Probe

A throwaway Rust binary linked against the real `git-core` crate (source kept outside the repo,
under the session scratchpad `plan-wave/probe114/`; Task S9's regression test is its permanent
replacement) ran every worktree-writing Browsitory operation against a fresh real repository
(`git2::Repository::init`, `core.autocrlf=false`, `commit.gpgSign=false`, pinned
`core.hooksPath`, branch `main` set explicitly). Next to each repo sat an `outside/` directory
holding `target.txt` and `f`, both `OUTSIDE\n`. Base commit: `tc` = `plain\n`, `d/f` =
`inner\n`, `a.txt` = `base\n`.

Two hazards:

- **Leaf** — tracked regular file replaced by a symlink to `outside/target.txt` (typechange).
- **Ancestor** — tracked directory `d/` replaced by a symlink to `outside/` (which holds `f`).

Linux, libgit2 1.9 (git2 0.21), 2026-10-28. "WRITE-THROUGH" = an outside file was modified or
deleted.

| Operation (git-core fn) | Checkout | Leaf | Ancestor |
|---|---|---|---|
| Stash all, `save_stash` without paths (`stash_save2`), incl. `-u` off and `keep_index` | libgit2 internal, forced | **WRITE-THROUGH** | **WRITE-THROUGH** |
| Stash with paths, `save_path_limited_stash` (batch 4c fix) | forced, guarded | safe (link removed, file restored) | safe (refused: `'d/f' is beyond a symbolic link.`) |
| Stash apply / pop | safe (default) | safe (refused: would overwrite) | safe (refused) |
| `stage::discard_file`, `discard_all_unstaged` | `checkout_index` forced | **WRITE-THROUGH** | safe (refused by `ensure_no_symlinked_parent`) |
| `stage::discard_hunk` / `discard_lines` on the typechanged file | `apply` to workdir | safe (errors; confusing copy) | — |
| `reset::reset_to(Hard)` (same commit and other commit) | `repo.reset` hard | **WRITE-THROUGH** | **WRITE-THROUGH** |
| `undo::restore` (undo/redo) when the snapshot pair holds the symlink | `checkout_index` forced (`checkout_from_index`) | **WRITE-THROUGH** | **WRITE-THROUGH** |
| `undo::restore` when the link appeared after the snapshot | — | safe (`RepositoryChanged`) | safe (`RepositoryChanged`) |
| `merge::abort_merge` (merge, cherry-pick, revert; also `pick::abort_sequence` and the conflicted step of a sequence) — conflicted path, cleanly merged path | `checkout_head` forced, path-limited | **WRITE-THROUGH** | **WRITE-THROUGH** |
| `merge::abort_merge` — path *added* by the merge, under a symlinked `d` | `remove_file` after `is_file()` (follows links) | — | **DELETES the outside file** |
| `merge::resolve_whole_file` | `checkout_index` forced + `use_ours` | safe (libgit2 replaced the link) | safe (refused by `ensure_inside_workdir`) |
| `merge::resolve_conflict` (text resolution) | `std::fs::write` | **WRITE-THROUGH** | safe (refused) |
| `merge::resolve_add_delete_conflict` | `std::fs::write` | **WRITE-THROUGH** | safe (refused) |
| `rebase::abort_rebase` (and `start_rebase`'s rollback, same fn) | `checkout_head` forced, whole tree | **WRITE-THROUGH** | **WRITE-THROUGH** |
| `pick::abort_sequence` rewind | `checkout_tree` safe | safe (refused: conflict prevents checkout) | — |
| `branch::switch_branch`, `checkout_commit`, `merge::start_merge` (ff / non-ff), `pick::cherry_pick`, `rebase::start_rebase` | safe | safe (refused) | safe (refused) |
| `remote::pull_after_fetch` | safe; also refuses a dirty worktree first | not reachable | not reachable |
| `clone::clone` | fresh directory | n/a | n/a |

`crates/repo-service/src/worker/mod.rs`'s forced `checkout_head` calls (lines ~1903, 1908, 2421,
4798) are test fixtures only.

Reference behaviour (`git` 2.53, same setup): `git reset --hard` and `git stash` **remove the
symlink and write a regular file / recreate the directory**; the outside files stay untouched.

Conclusions:

1. Every **forced** checkout over a path set that can contain a typechanged symlink is affected;
   libgit2 1.9 follows both a leaf symlink and a symlinked ancestor directory when it writes.
2. **Safe** checkouts refuse (the typechange counts as a local modification), so they are not
   affected; the refusal is the existing behaviour and stays.
3. Browsitory's own raw writes (`std::fs::write` in conflict resolution, `remove_file` in
   `abort_merge`) follow links too.
4. Batch 4c's path-limited stash (`stash.rs`: `has_symlinked_ancestor`, the
   "remove the typechanged symlink before checkout" step 2) is the working pattern to
   generalize.

### 1.2 Design: `git-core::worktree_guard`

New module `crates/git-core/src/worktree_guard.rs` (`pub(crate)`, plus `pub` items only where
tests need them). It owns every "never through a symlink" rule; `stash.rs`'s
`has_symlinked_ancestor` and `stage.rs`'s `ensure_no_symlinked_parent` move here (the old names
stay as thin wrappers or are replaced at their call sites).

```rust
/// What a forced checkout is about to write: entries of a tree or of the index, optionally
/// limited to literal paths.
pub(crate) enum GuardSource<'a> {
    Tree(&'a git2::Tree<'a>),
    Index(&'a git2::Index),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Hazard {
    /// `path` is a symlink in the worktree but a regular/executable file in the source.
    Leaf(String),
    /// `dir` (a strict ancestor of a source path) is a symlink (or a Windows junction).
    Ancestor(String),
}

/// Lists hazards without touching the worktree. Each directory is `symlink_metadata`'d once
/// (memoised); each source path once.
pub(crate) fn scan(workdir: &Path, source: &GuardSource, paths: Option<&[String]>) -> Result<Vec<Hazard>, git2::Error>;

/// Removes every hazard's link (never its target): `remove_file` for a file symlink,
/// `remove_dir` for a Windows directory symlink/junction (falls back on error). Returns
/// what was removed. Call immediately before the forced checkout.
pub(crate) fn clear_hazards(workdir: &Path, hazards: &[Hazard]) -> std::io::Result<()>;

/// True when a strict ancestor of `rel` (inside `workdir`) is a symlink. (Moved from stash.rs.)
pub(crate) fn has_symlinked_ancestor(workdir: &Path, rel: &str) -> bool;

/// Removes `rel` without following a link at the leaf or above it: returns `Ok(false)` and does
/// nothing when an ancestor is a symlink.
pub(crate) fn remove_beneath(workdir: &Path, rel: &str) -> std::io::Result<bool>;

/// Writes `bytes` to `rel`, first removing a leaf symlink and refusing (`Ok(false)`) when an
/// ancestor is a symlink. For Browsitory's own raw writes (conflict resolution).
pub(crate) fn write_beneath(workdir: &Path, rel: &str, bytes: &[u8]) -> std::io::Result<bool>;
```

Rules:

- A source entry with mode `120000` (symlink) is not a leaf hazard: libgit2 writes links with
  `symlink()` after unlinking, which never follows. Submodule entries (`160000`) are skipped.
- Only paths **in the source** (and their ancestors) are considered: an untracked symlink that
  no source path goes through is the user's and is never removed.
- `scan` is O(source paths) `lstat`s, the same order of work libgit2's checkout already does.

Per call site (recommended answer to D1 — "remove the link, then write", git parity):

| Call site | Change |
|---|---|
| `reset::reset_to(Hard)` | `scan(Tree(target))` → `clear_hazards` → `repo.reset` |
| `stage::discard_file` (and so `discard_all_unstaged`) | leaf only (ancestors are already refused): if `rel` is a symlink and the index entry is not a link, remove it before `checkout_index` |
| `undo::WorktreeChange::apply` / `checkout_from_index` | `scan(Index, from_index)` → `clear_hazards` before `checkout_index`; `write_worktree_file` creates parents only after clearing symlinked ancestors of that path; deletions go through `remove_beneath`; `prune_empty_parents` never removes or descends into a symlink |
| `merge::abort_merge` | `scan(Tree(HEAD), to_checkout)` → `clear_hazards` before `checkout_head`; the "added by the operation" removal uses `remove_beneath` (no `is_file()` follow) |
| `merge::resolve_conflict`, `merge::resolve_add_delete_conflict` | `write_beneath` instead of `std::fs::write` (the ancestor check already exists) |
| `rebase::restore_original_branch` | `scan(Tree(HEAD tree))` → `clear_hazards` before `checkout_head` |
| `stash::save_stash` without paths | `scan(Tree(HEAD))` + `scan(Index)`; any hazard → route to `save_path_limited_stash` with `paths` = every changed path from status (D2); none → `stash_save2` as today |
| `stash.rs` path-limited step 2 | replaced by the shared helpers (no behaviour change) |
| safe checkouts (`switch_branch`, `checkout_commit`, `start_merge`, `cherry_pick`/`revert`, `start_rebase`, `pull_after_fetch`, `abort_sequence` rewind, stash apply) | unchanged; covered by the regression test so a future switch to `force()` is caught |

Order matters: clear the hazards *after* anything that must record them (stash) and *right
before* the libgit2 call.

### 1.3 Residual risks (documented, not fixed here)

- **Race**: a concurrent local process can swap a symlink in between `clear_hazards` and
  libgit2's write. Same threat class as #138 (needs a local writer racing the app); libgit2
  offers no `openat`-style checkout. Recorded in `docs/ARCHITECTURE.md`.
- **Windows**: `FileType::is_symlink()` is true for symlinks and junctions (name-surrogate
  reparse points), so the guard covers both; directory links are removed with `remove_dir`
  (never recursive). Creating symlinks on Windows needs Developer Mode, so the new tests are
  `#[cfg(unix)]`; Windows behaviour rests on the shared code path. With `core.symlinks=false`
  (Windows default) libgit2 writes tracked links as plain files, which is not a hazard.
- `discard_hunk`/`discard_lines` on a typechanged path fail with confusing copy ("failed to
  update the index: 1 conflict prevents checkout") but write nothing. Not changed.
- `abort_sequence`'s safe rewind refuses while a typechange is present; the user must discard
  it first. Not changed (safe).

---

## 2. #138 — race-free work-tree read for the image preview

Today `diff::workdir_side` checks the path (`parent_stays_inside`, `symlink_metadata`), then
opens with `O_NOFOLLOW|O_NONBLOCK` and compares inode numbers. `O_NOFOLLOW` covers only the last
component; a directory component swapped for a symlink between check and open is followed, and
`same_file` compares two lookups that both happen after the swap. Reproduced: 9,725 / 200k reads
leaked.

### 2.1 Unix: `openat` walk (recommended, no new dependency)

New `pub(crate) fn open_beneath(workdir: &Path, rel: &Path) -> io::Result<Option<File>>` in
`worktree_guard.rs`:

1. `open(workdir, O_RDONLY|O_DIRECTORY|O_CLOEXEC)` — the repo root itself may be reached
   through a symlink (the user chose that path).
2. For each intermediate component: `openat(dirfd, c, O_RDONLY|O_DIRECTORY|O_NOFOLLOW|O_CLOEXEC)`;
   close the previous fd.
3. Final component: `openat(dirfd, last, O_RDONLY|O_NOFOLLOW|O_NONBLOCK|O_CLOEXEC)`.
4. `fstat`: not a regular file → `None`.
5. `ELOOP`, `ENOTDIR`, `ENOENT` at any step → `None` (a symlink, a file where a dir was
   expected, or gone). Other errors propagate.

Each step resolves relative to an fd that cannot have been redirected, so no swap at any depth
can escape. `libc` (already a unix dependency, MIT/Apache, listed in `LICENSE_COMPLIANCE.md`)
provides `openat` on Linux and macOS; the row's usage text is extended, no new row. `rel` is
already checked by `is_safe_relative` (no `..`, not absolute). Linux's `openat2(RESOLVE_BENEATH)`
would be shorter but is Linux ≥ 5.6 only.

`workdir_side` then becomes: `open_beneath` → `None` ⇒ `Missing`; size from the handle's
`metadata()` (> cap ⇒ `TooLarge`); bounded read as today. `parent_stays_inside` and `same_file`
are no longer needed there (kept for their other callers).

### 2.2 Windows (recommended: handle path check, no new dependency)

`open_beneath` on Windows: open with `FILE_FLAG_OPEN_REPARSE_POINT` (via
`OpenOptionsExt::custom_flags`, std) so a leaf link is opened as itself (then its metadata is a
symlink → `None`); then call `GetFinalPathNameByHandleW` on the opened handle (declared with a
small `extern "system"` block against kernel32, which std already links) and require the result
to start with `std::fs::canonicalize(workdir)` (both are `\\?\` paths from the same resolver).
The check runs on the open handle, so a junction swapped in before the open is detected and a
swap after the open cannot change what was opened. See D4.

### 2.3 Tests

Deterministic tests for `open_beneath` (unix): regular file opens; leaf symlink, symlinked
intermediate directory, FIFO (non-blocking, `None`), directory, and missing paths → `None`. The
race itself goes into an `#[ignore]`d stress test (thread swapping `rd/` between a real dir and a
symlink while 200k `file_sides` calls run; asserts zero outside reads) — run manually, not in CI.

---

## 3. #110 — signing timeout and cancel

`signing::run_program` uses `wait_with_output` with no deadline. Batch 4e's
`process::run_captured` has timeout, cancel and process-group kill, **but it merges stdout and
stderr into one pipe**; signing needs them apart (gpg: signature on stdout, `[GNUPG:]` status on
stderr; ssh-keygen: signature on stdout).

Design:

- `process.rs`: factor the poll/cancel/deadline/kill/reap loop out of `run_captured` into a
  shared private fn; add `pub(crate) fn run_captured_split(command, stdin, cancel, timeout,
  stdout_limit, stderr_limit) -> Result<SplitCaptured, RunError>` with two drain threads.
  `SplitCaptured { success, stdout: Vec<u8>, stdout_overflowed: bool, stderr: String /* tail */, timed_out }`.
  A signature must not be truncated: overflowing `stdout_limit` (1 MiB) is reported, and signing
  treats it as failure.
- `signing.rs`: `pub struct SignOptions<'a> { pub cancel: &'a dyn Fn() -> bool, pub timeout: Duration }`
  with `Default` (never cancelled, `SIGN_TIMEOUT`). `sign_buffer_with`, `create_commit_with`
  take it; `sign_buffer` / `create_commit` keep their signatures and use the default, so the six
  existing callers compile unchanged. `verify_commit` uses `VERIFY_TIMEOUT` (no cancel).
- New `SignError::TimedOut { seconds }` → `"The signing program did not finish within {seconds} s. If it is waiting for a passphrase, unlock your key and try again."`
  and `SignError::Cancelled` → `"Signing was cancelled."`. A verify timeout maps to the existing
  "error" signature status with detail `"timed out"` (never blocks the history view).
- `SignerNotFound` keeps working: `RunError::Spawn` with `NotFound` maps to it.
- Commit/amend: `commit::commit` already receives `HookOptions.cancel` (the 4e operation's
  cancel check) and its reporter; it passes the cancel into `create_commit_with` and, when
  signing is configured, reports `started("signing")` / `finished("signing", …)` so the existing
  CommitBox status line + Cancel button appear while the signer runs (D3). The frontend's
  `runningHookLabel("signing")` returns `Waiting for the signing program…`.
- Rebase / cherry-pick / revert / sequence commits: timeout only (no cancel plumbing exists
  there; adding it is out of scope).

Behaviour change to note: `run_captured` puts the child in its own process group. A signer
that prompts on `/dev/tty` (ssh-keygen with an encrypted key and no agent) when the app was
started from a terminal is stopped by `SIGTTIN` until timeout/cancel, where today it prompts in
that terminal. Desktop launches have no terminal either way; gpg prompts through gpg-agent's
pinentry (a separate process), which is unaffected.

---

## 4. #116 — per-file diff includes hunks of `path/…`

Literal pathspec `d` matches `d` and everything under `d/`. Affected: `working_diff` (staged and
unstaged), `commit_diff`, `commit_rename_diff`, `range_diff`, `compare_working_tree`, and
**`stage::find_hunk`** (used by stage/unstage/discard hunk and lines) — so staging "a hunk of
`d`" could pick up a hunk of `d/inner.txt`.

Fix: `hunks_from_diff(diff, wanted: &[&str])` keeps only deltas whose old or new path equals one
of `wanted` exactly (byte comparison on `Path`), and `find_hunk` gets the same filter. Every
per-file API passes `[path]` or `[path, old_path]`. No pathspec change (the pathspec still
narrows libgit2's work).

## 5. #117 — conflicted file in working-tree compare

`compare_working_tree` diffs tree → workdir *with index*; a conflicted path is a
`Delta::Conflicted` without a patch. Fix: when the requested path's delta is `Conflicted` (or
the index has conflict entries for it), diff that path with `diff_tree_to_workdir` (no index,
same literal pathspec, filters still applied) and return those hunks. If libgit2 still reports
no patch there (to be confirmed by the first test), fall back to
`Patch::from_blob_and_buffer(commit blob, worktree bytes read with open_beneath)`. A worktree
file that is a symlink or missing yields the deletion hunks, never a read through a link.

## 6. #137 — staged rename has no old path

Probe (same session): for a staged rename `old.png → new.png`, `status()` reports
`StatusEntry { path: "old.png", staged: true, kind: Renamed }` — git2's
`StatusEntry::path()` returns `head_to_index.old_file`. So the row shows the **old** name, and
`file_sides(Staged, "new.png")` yields `(Missing, Loaded)` = "Added".

Fix:

- `StatusEntry` gains `old_path: Option<String>`; for `Renamed`, `path` is the **new** path
  (`head_to_index.new_file` / `index_to_workdir.new_file`) and `old_path` the old one.
- `status()` sets `renames_index_to_workdir(false)` (D5): an unstaged rename becomes "Deleted
  old" + "New new" like `git status`, so per-file stage/discard keep meaning one path. (Today
  the untracked new file is hidden inside the rename row and only the old path is actionable.)
- `tools::DiffSource::Staged` becomes `Staged { old_path: Option<String> }`; `file_sides` and
  `prepare_diff_tool_with` read the old side from `old_path`. Wire format: `{"kind":"Staged"}`
  stays valid (`oldPath` defaults to `null`).
- `diff::working_diff` gains `old_path: Option<&str>`: for a staged rename it diffs both paths
  with rename detection (like `commit_rename_diff`), so the text view shows only the edits plus
  "Renamed from …".
- `stage::unstage_file(path)` on a staged rename resets **both** paths (else the old path stays
  staged as Deleted).
- DTOs (`StatusEntryDto` Tauri + sidecar), `RepoClient.StatusEntry.oldPath: string | null`,
  `DiffSource` Staged `oldPath`, `getWorkingDiff(…, oldPath)`; `DiffPane` passes `entry.oldPath`
  to the image preview, the text diff and "Open in diff tool", and shows "Renamed from {old}".

---

## Product decisions

**D1 — A forced checkout finds a typechanged symlink (#114).**
- A. Remove the link, then write the file / recreate the directory — `git` parity; the link was
  an uncommitted change the operation is discarding anyway (stash records it first).
  **Recommended.**
- B. Refuse the whole operation and name the path ("tc is a symbolic link; remove it first").
  Safest, but reset --hard / abort merge / abort rebase can then get stuck until the user fixes
  the worktree by hand.
- C. Remove leaf links, refuse when a *directory* was replaced by a link.

Existing per-path refusals stay either way (`discard_file` and path-limited stash refuse a
path beyond a symlinked directory; conflict resolution refuses a symlinked parent).

**D2 — "Stash all" while a hazard exists (#114).** libgit2's `stash_save2` records and resets
in one call, so the link cannot be removed in between.
- A. Route that stash through Browsitory's own path-limited implementation (batch 4c) with every
  changed path; libgit2 stays in charge otherwise. A symlinked *directory* is then refused
  (`'d/f' is beyond a symbolic link.`), a leaf link is stashed and replaced. **Recommended.**
- B. Always use Browsitory's implementation for stash all (one code path, larger behaviour
  surface: submodules, intent-to-add, ignored files).
- C. Refuse "Stash all" while any hazard exists.

**D3 — Signing timeout and cancel (#110).**
- A. 120 s to sign, 30 s to verify; Cancel via the existing commit-hook status line
  ("Waiting for the signing program…" + Cancel) on commit/amend; timeout only for
  rebase/pick commits. **Recommended** (pinentry rarely needs more than two minutes; Cancel
  covers the rest).
- B. 600 s like hooks, no UI change.
- C. Configurable timeout in Preferences (new `config` setting + UI) on top of A.

**D4 — Windows for #138.**
- A. `FILE_FLAG_OPEN_REPARSE_POINT` + `GetFinalPathNameByHandleW` check via a hand-written
  `extern "system"` declaration; no new dependency. **Recommended.**
- B. Same, through `windows-sys` (MIT/Apache, already in `Cargo.lock` transitively) — adds a
  direct dependency + `LICENSE_COMPLIANCE.md` row, safer FFI types.
- C. Document only: Windows keeps the check-then-open (its `same_file` is always true), noted
  as a known limitation.

**D5 — Unstaged renames (#137).**
- A. Stop detecting index→worktree renames in status (`git status` parity): a moved file is
  "Deleted" + "New". **Recommended.**
- B. Keep them as one row with `oldPath`, and make stage/discard act on both paths.

---

## Testing strategy

- Real temp-dir repos only; helpers pin `core.autocrlf=false`, `core.eol=lf`,
  `core.safecrlf=false`, `commit.gpgSign=false`, `core.hooksPath`, and never rely on
  `init.defaultBranch` (tests read the branch from `HEAD` or set `refs/heads/main` explicitly).
- Symlink tests are `#[cfg(unix)]`; the "outside" directory is a second `TempDir` so a
  write-through is observable and never touches the developer's files.
- Regression matrix `crates/git-core/tests/symlink_write_through.rs` mirrors the probe table,
  including the safe-strategy rows, so a future `force()` is caught.
- Signing tests use fake signer scripts (unix) with a short `SignOptions.timeout`.
- Frontend: Vitest mocking `RepoClient`.
- No E2E changes; existing suites must stay green.

## Out of scope

- libgit2-level race between `clear_hazards` and the checkout write.
- Cancel for rebase/pick signing; a configurable timeout (unless D3-C).
- Text diff of an unstaged rename (D5-A removes the case).
- Hard links to outside files (not a symlink issue; same on every git client).

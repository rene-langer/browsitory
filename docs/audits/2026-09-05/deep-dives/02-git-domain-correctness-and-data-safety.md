# Deep-dive 02: Git-domain correctness and data safety

## Scope and audit questions

`crates/git-core` is the sole owner of libgit2 interaction in Browsitory (see
`docs/ARCHITECTURE.md`). This deep-dive asks:

1. Do the git2 API "Result vs Option" gotchas CLAUDE.md documents (for
   `StatusEntry::path`, `Signature::name`/`email`, `Reference::shorthand`,
   `Commit::summary`, `StringArray::iter`) hold consistently across every
   module, with no panicking `.unwrap()`/`.expect()` on a fallible accessor?
2. Do destructive/recovery operations (merge abort, rebase abort, worktree
   removal) restore exactly the pre-operation state, with no data loss?
3. Are conflict-resolution and file-write code paths safe against
   unvalidated or unexpected input, given they write directly to the
   repository's working directory?
4. Is `git-core`'s test suite (`crates/git-core/tests/*.rs`) proportionate to
   the risk of each module?

## Paths examined

- `crates/git-core/src/{status,stage,commit,branch,diff,merge,rebase,blame,
  reflog,remote,forge,graph,submodule,worktree,repo}.rs` (all 15 modules).
- `crates/git-core/tests/*.rs` (18 test files, 164 `#[test]` functions).
- `crates/tauri-app/src/commands/merge.rs`, `crates/tauri-app/src/worker/
  merge.rs`, `frontend/src/ipc/tauriRepoClient.ts` (to trace the IPC trust
  boundary for one finding below).
- `CHANGELOG.md`'s 0.2.0 entry describing a fix to
  `abort_rebase_restores_the_original_branch_and_tip_exactly`.

## Evidence reviewed and checks run

- **Static trace** of every `.path()`, `.shorthand()`, `.name()`, `.email()`,
  `.summary()`, and `StringArray`/`iter()` call site in `crates/git-core/src`
  (18 call sites across 12 files) against the git2 0.21 accessor shapes
  CLAUDE.md documents.
- **Static trace** of every `.unwrap()`/`.expect()` in `crates/git-core/src`:
  none found (`grep -n '\.unwrap()\|\.expect(' crates/git-core/src/*.rs`
  returned no matches). The two apparent hits in `worktree.rs:175` and
  several `reflog.rs`/`graph.rs`/`remote.rs` lines are all
  `.unwrap_or_default()` on an already-`.ok()`-converted `Result`, which
  cannot panic.
- **Read-through** of `merge.rs`, `rebase.rs`, `status.rs`, `worktree.rs` in
  full; targeted reads of `stage.rs`, `commit.rs`, `submodule.rs`,
  `remote.rs`, `branch.rs`.
- **Test attempted, did not complete**: `cargo test -p git-core -p config`,
  run twice against the shared workspace `target/` (both blocked
  indefinitely on `Blocking waiting for file lock on build directory`) and
  twice more against an isolated `CARGO_TARGET_DIR` (both hit a 280s
  timeout still compiling `libgit2-sys` from source). See "Coverage gaps"
  below — this is an environment constraint of the audit run, not a
  defect, but it means **no test results for `git-core`/`config` are
  reported by this deep-dive**; only static analysis.
- Line-counted `#[test]` functions per test file to gauge coverage breadth
  (`grep -c '^#\[test\]' crates/git-core/tests/*.rs`).

## Findings

### AUD-2026-09-05-GIT-001 — `resolve_conflict` writes attacker-reachable paths/content into the working tree without validating a conflict exists

- **Severity**: High
- **Confidence**: Confirmed (code-level defect, static trace); the
  downstream exploitability depends on the trust boundary discussed below,
  which is not independently confirmed in this deep-dive.
- **Affected components**: `crates/git-core/src/merge.rs:169-181`
  (`resolve_conflict`), reached via `crates/tauri-app/src/worker/merge.rs:27`
  and `crates/tauri-app/src/commands/merge.rs:26-32`
  (`#[tauri::command] resolve_conflict`), invoked from
  `frontend/src/ipc/tauriRepoClient.ts:191`.
- **Evidence**: `resolve_conflict` is:

  ```rust
  pub fn resolve_conflict(
      repo: &Repository,
      path: &str,
      resolved_content: &str,
  ) -> Result<(), MergeError> {
      let workdir = repo.workdir().ok_or(MergeError::NoWorkdir)?;
      std::fs::write(workdir.join(path), resolved_content)?;

      let mut index = repo.index()?;
      index.add_path(Path::new(path))?;
      index.write()?;
      Ok(())
  }
  ```

  Contrast this with its sibling `resolve_add_delete_conflict`
  (`merge.rs:208-242`), which calls `find_conflict(repo, path)` first and
  returns `MergeError::NoConflict` if `path` is not an actual conflicted
  path in the index. `resolve_conflict` has no such check: it writes
  `resolved_content` to `workdir.join(path)` unconditionally, for any
  `path` string, then unconditionally stages it. The Tauri command layer
  performs no additional validation — `path` and `resolved_content` are
  both passed through verbatim from the IPC call
  (`worker_handle(&state, &repo_path)?.resolve_conflict(path, resolved_content)`,
  `commands/merge.rs:32`), which originates as a plain string argument
  from the webview's `invoke()` call.
- **Impact**: Any caller of this Tauri command — not just one legitimately
  resolving a listed conflict — can write arbitrary content to any
  `path` under the repository's working directory, including into `.git/`
  itself (e.g. `path = ".git/config"` or `path = ".git/hooks/post-checkout"`
  is a normal subpath of `workdir`, not a traversal). Overwriting
  `.git/config` can silently redirect the repo's remotes, insert a
  malicious `core.hooksPath`, or add a `url.<base>.insteadOf` rewrite that
  redirects future fetches/pushes. Overwriting an existing hook file's
  content is also possible, though the file `std::fs::write` creates is
  not executable by default, so this alone does not achieve code execution
  without an additional step (e.g. an existing hook already being
  executable, or another `git` operation that sets the bit).
- **Trigger/reproduction**: Static — call `git_core::merge::resolve_conflict`
  (or the `resolve_conflict` Tauri command) with a `path` that does not
  appear in `repo.index()?.conflicts()`, e.g. `.git/config`, and arbitrary
  `resolved_content`; the write and `index.add_path`/`index.write()` both
  succeed with no error. This was traced by reading the function body and
  its only two call paths; it was not exercised at runtime in this audit
  (see the "test could not run" limitation above), so mark this a static
  finding pending a live repro.
- **Realistic exploitation path**: Reaching this from outside the app
  requires either (a) a bug or future change elsewhere in the frontend that
  calls `resolveConflict` with a non-validated path (there is currently
  exactly one call site, `frontend/src/components/.../*ConflictResolution*`
  — not audited in this deep-dive, see deep-dive 05), or (b) a webview
  content-injection (XSS) vulnerability that lets injected script call
  `window.__TAURI__.invoke('resolve_conflict', ...)` directly. Browsitory's
  webview only loads its own bundled frontend, so (b) requires a separate,
  unconfirmed XSS bug; this finding stands on its own as a missing
  invariant regardless of whether such a bug exists today.
- **Remediation**: Make `resolve_conflict` call `find_conflict(repo, path)`
  first (mirroring `resolve_add_delete_conflict`) and return
  `MergeError::NoConflict` for any path that is not actually conflicted.
  This is a small, local, low-risk fix — it does not change the function's
  signature or its behavior for any legitimate caller, since a legitimate
  caller always resolves a path drawn from the conflict list the UI
  already displays.
- **Verification**: Add a `git-core` test asserting `resolve_conflict`
  returns `Err(MergeError::NoConflict(_))` for a path with no index
  conflict entry (mirroring the existing add/delete-conflict test
  coverage), and a regression test that a `.git/`-relative path is
  specifically rejected.

### AUD-2026-09-05-GIT-002 — `git-core`/`config` test execution — resolved: full suite confirmed green after this deep-dive was drafted

- **Severity**: N/A (was a coverage gap at drafting time; closed by a later,
  successful run in the same audit session)
- **Update:** the concurrent `cargo build --workspace` this deep-dive's own
  attempts were contending against (see the original note below) finished
  and a subsequent `cargo test --workspace` run against the now-warm shared
  `target/` completed cleanly, exit 0. Per-crate results, read directly from
  that run's log:
  - `config`: 32 tests passed, 0 failed (`graph_branch_selection` 4,
    `last_seen_version` 3, `recent_repos` 11, `scan_repos` 4, `workspaces`
    10).
  - `git-core`: 173 tests passed, 0 failed across the unittest binary (4)
    and all 18 integration-test files (`blame` 4, `branch` 11, `diff` 6,
    `forge` 13, `graph` 7, `merge` 20, `rebase` 25, `reflog` 1,
    `reflog_head` 3, `reflog_missing_log` 2, `reflog_safety` 1, `remote` 27,
    `repo` 3, `stage_commit` 13, `stash` 6, `status` 9, `submodule` 12,
    `worktree` 6). This is the exact module set this deep-dive traced
    statically above, now test-confirmed rather than static-only — no test
    exercises the missing-conflict-validation gap GIT-001 describes (that
    finding stands; there is simply no negative-path test for it in either
    direction).
  - `tauri-app` (both default and `--features forge-fixture-override`):
    90 tests passed, 0 failed (covered fully by deep-dives 03/04, not
    re-detailed here).
  - `cargo clippy --workspace --all-targets -- -D warnings`: clean, 0
    warnings, exit 0.
  - `cargo fmt --all -- --check`: clean, exit 0.
- **Original note (context for why four earlier attempts failed):** this
  audit session ran many concurrent `cargo build`/`test`/`clippy`
  invocations across its several deep-dives at once, which exhausted the
  shared `target/` build lock and CPU for a period; four attempts at
  `cargo test -p git-core -p config` during that window (two against the
  shared `target/`, two more against an isolated `CARGO_TARGET_DIR`) each
  failed to complete before the deep-dive was drafted. This was an
  audit-session scheduling artifact, not anything about the code, and it
  self-resolved once the contending builds finished.
- **Assessment:** with the full suite now confirmed green and `clippy`/`fmt`
  also clean, this deep-dive's static findings (GIT-001) should be read as
  additive to a healthy, currently-passing test baseline — not as an
  unverified claim sitting on top of an unknown test state.

## Coverage gaps, open questions, and accepted risks

- ~~Test-run verification~~ — resolved; see the update to GIT-002 above. All
  173 `git-core` tests and 32 `config` tests are confirmed passing as of
  this audit session.
- **Malformed/unusual repository behavior** (empty repo with zero commits,
  detached HEAD, bare repository passed where a workdir is assumed) was
  assessed only by static trace of error paths (e.g. `MergeError::NoWorkdir`,
  `WorktreeError` variants), not by constructing and exercising such repos.
  `crates/git-core/tests/repo.rs` has only 3 tests, the thinnest file in the
  suite relative to how many edge cases "repo discovery/open" implies (e.g.
  no test file name suggests a bare-repository or zero-commit-repository
  case is covered) — flagged as a coverage gap, not a confirmed defect.
- **Blame** (`blame.rs`) has only 4 tests against an 8-parameter core
  operation (line-by-line attribution); not deeply traced in this pass
  beyond confirming no panicking accessor use.
- **Submodule and worktree edge cases** (nested submodules, a submodule
  whose `.gitmodules` entry has no corresponding working-tree directory, a
  worktree whose backing directory was deleted out-of-band) were read for
  error-handling shape but not exercised.
- This deep-dive did not audit `crates/git-core/src/forge.rs` or
  `remote.rs`'s credential-adjacent logic in depth — that is deep-dive 04's
  scope (remote/forge/credential security); only the `StringArray`
  Result/Option-flattening pattern in those files was checked here.

## Strengths and positive controls

- **No panicking accessor usage found** anywhere in `crates/git-core/src`
  across all 18 identified `.path()`/`.name()`/`.email()`/`.summary()`/
  `StringArray` call sites — every one correctly matches the git2 0.21
  Result/Option shape CLAUDE.md documents (e.g. `status.rs:43`'s
  `let Ok(path) = entry.path() else { continue };`, `forge.rs:50`'s
  double-`.flatten()` on `repo.remotes()?.iter()`, `graph.rs:64-66`'s
  `.summary().ok().flatten()`). This is a real, verified discipline, not
  just a documented convention — CLAUDE.md's warning appears to be
  followed consistently.
- **`abort_rebase`/`restore_original_branch`** (`rebase.rs:442-457`) is
  correct by construction: the original branch ref is never mutated while
  a rebase is in progress (only the detached `HEAD` moves), so recovery is
  a pure re-attach-and-force-checkout with no reconstruction logic that
  could drift from the true prior state. The de-flaked test at
  `crates/git-core/tests/rebase.rs:542-593` now asserts against a tip
  captured immediately before the aborted rebase rather than a stale
  pre-everything tip, which is the correct fix (not a coincidence-driven
  workaround) — the underlying implementation was never wrong, only the
  test's assertion target was.
- **`abort_merge`** (`merge.rs:183-191`) correctly gates on `is_merging()`
  or `index.has_conflicts()` before doing a hard reset, preventing an
  accidental hard-reset call when no merge is in progress.
- **`worktree::remove_worktree`** (`worktree.rs:103-131`) checks lock state
  and working-tree cleanliness (via a real `status()` call on the linked
  repo) before pruning, and refuses to remove the main worktree — sound
  layered guards against data loss.
- **`merge::parse_conflict_markers`** (`merge.rs:126-167`) is carefully
  documented and implemented to preserve exact original line endings
  (including CRLF and a missing final newline) via `split_inclusive('\n')`
  and `""`-joins rather than `.lines()` + `"\n"`-joins — a subtle
  correctness detail that is easy to get wrong and is handled deliberately
  here, with the reasoning left in a comment.

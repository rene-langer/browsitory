# Security Follow-ups Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close #156 (case-colliding link/folder on case-insensitive file systems), #149
(race-free raw work-tree I/O), #150 (Windows `open_beneath` walks components) and #131
(runnable-file refusal gaps). Rust only; no DTO or frontend change.

**Architecture:** `git-core::worktree_guard` stays the single owner of every "never through a
link" rule.
- It gains a pure tree check (`find_case_collision`), hooked into `refuse_linked_folders`,
  `prepare_forced_checkout`, clone, worktree add and submodule update.
- It gains a component walker per platform: `openat`/`unlinkat`/`O_NOFOLLOW` on unix in
  `worktree_guard/beneath_unix.rs`, and pinned handles without `FILE_SHARE_DELETE` +
  `FILE_FLAG_OPEN_REPARSE_POINT` on Windows in `worktree_guard/beneath_windows.rs`. Every raw
  read, write and delete, including undo's, goes through the walker.
- `launch.rs` gets list and sniff fixes.

**Tech Stack:** Rust (`git2` 0.21 / libgit2 1.9.6, `libc` on unix, hand-declared `kernel32` on
Windows, `thiserror`).

**Spec:** `docs/superpowers/specs/2026-10-28-security-followups-design.md` (libgit2's built-in
protections, Linux probe, design, decisions D1–D5). This plan implements the **recommended**
option of each; see "Decision hooks" for the others.

---

## Global Constraints

- **No new dependency** (D3-A, D4-A). `libc` is already a unix dependency of `git-core`. Extend
  its `docs/LICENSE_COMPLIANCE.md` usage text in Task Z1 (`fstatat`, `unlinkat`, `mkdirat`,
  `symlinkat`, `readlinkat`, `fchmod`, `AT_*`); no new row.
- **Branch:** `fix/security-followups` (pushed). One PR, opened as a **draft** in Task C1 so
  CI runs the macOS/Windows probe and the Windows tests; marked ready in Task Z2.
- **Hermetic tests:** every **new** integration test binary starts with
  `test_support::hermetic_git!()` (or `mod common;`, whose `tests/common/mod.rs` already has
  it). Unit tests in `src/` are covered by `lib.rs`'s `#[cfg(test)] hermetic_git!()`.
- **Windows-safe tests:**
  - Anything that creates a symlink or FIFO, or uses `libc`, is `#[cfg(unix)]`, test *and*
    helper.
  - Junction tests are `#[cfg(windows)]` (`cmd /C mklink /J`). Windows symlink tests return
    early with `eprintln!("SKIP …")` on `ERROR_PRIVILEGE_NOT_HELD` (1314).
  - Case-collision tests build link **entries** with `TreeBuilder` (`FileMode::Link`). They
    need no file-system link and run everywhere.
  - Repo helpers pin `core.autocrlf=false`, `core.eol=lf`, `core.safecrlf=false`,
    `commit.gpgSign=false`, `core.hooksPath=<gitdir>/hooks`.
  - Never rely on `init.defaultBranch`: use `repo.set_head("refs/heads/main")` before the
    first commit, or read the branch from `HEAD`.
  - The "outside" target is always a second `TempDir`.
- **Windows code locally:** no mingw on the dev box. Compile-check and clippy-check with
  `cargo clippy -p git-core --target x86_64-pc-windows-gnu` with git2 stubbed (as in the
  previous wave), or rely on CI. Never mark a Windows task done before the `windows-latest`
  `rust` job is green on the draft PR.
- **Never** touch the real `~/.local/share/com.browsitory.app` or the real config dir.
- **Error copy, exactly** (`CaseCollision`): `'{link}' is a symbolic link and '{folder}' a
  folder whose names differ only in case. On this file system they are the same name, so
  Browsitory won't check them out. Rename one of them in the repository.`
- **CHANGELOG:** Task C1 adds a `### Security` stub line for #156 under `## [Unreleased]`, so
  the pre-push hook and the PR `changelog` job pass from the first push. Z1 writes the final
  entries.

## Pre-flight notes

Reconciled with the code at 3dabf144 on 2026-10-28:

- Raw work-tree I/O outside libgit2: `worktree_guard::{write_beneath, remove_beneath,
  clear_hazards, remove_link, clear_symlinked_ancestors, open_beneath}` (callers: `merge.rs`
  resolve/abort, `stash.rs` path-limited stash, `diff.rs` preview) and
  `undo::{read_worktree_file, remove_existing, prune_empty_parents, write_worktree_file,
  set_executable}` (`read_worktree_file` is also used by `stash::worktree_blob`).
- libgit2 work-tree writers without a Browsitory guard today: `clone::clone_repo`
  (`RepoBuilder::clone`), `worktree::create_worktree` (`repo.worktree`; git2 0.21 has no
  checkout options for it) and `submodule::update_submodule` (`submodule.update`: libgit2
  clones with `GIT_CHECKOUT_NONE`, then `set_head_detached`, then
  `checkout_head(update_opts.checkout)`).
- `RemoteError::LinkedFolder` is passed through as text in
  `repo-service/src/worker/remote.rs:347`. `CaseCollision` joins that arm.
- Linux probe results and libgit2 source facts: spec §1.1–1.2. Scratch notes and the probe
  live outside the repo.

Implementation notes and deviations (2026-10-29; code wins):

- **Missed raw I/O:** `stash.rs`'s path-limited stash removed untracked files and pruned their
  folders by path (`remove_file` / `remove_dir` after `has_symlinked_ancestor`). It now calls
  `remove_and_prune_beneath`.
- **Undo writes before its forced checkout.** `restore` writes raw files (`apply`) before
  `checkout_from_index` reaches `prepare_forced_checkout`, so the C3 hook alone came too late;
  `restore` also checks the target work tree (limited to the changed paths) in its verify phase.
- **Path filter:** `find_case_collision` matches `paths` ignoring case (as libgit2 under
  `core.ignorecase`), so `paths = ["d"]` selects the link `D` too and is refused; the C2 test
  checks `["d/x"]` → `None` and `["d"]` → `Some` instead of the plan's `["d"]` → `None`.
- **Ordering:** `refuse_linked_folders` runs its link scan first and the case check after it.
  macOS CI showed a merge result holding the link `d` and `d/new.txt` (exact same name) refused
  as `CaseCollision` instead of the existing, more specific `LinkedFolder`.
- **Submodule update:** git2 0.21's `CheckoutBuilder` has no `baseline`, so an initialized
  submodule is checked against its target commit *before* libgit2 updates it (fetching the
  commit from its origin first if missing, as libgit2 would); a fresh one is cloned with
  `dry_run`, checked, then `checkout_head(safe + recreate_missing)` (the no-checkout pass already
  wrote the index, so plain safe recreates nothing). A refused fresh submodule stays cloned with
  an empty work tree.
- **Worktree add:** a new branch at the start point is created only after the check passes.
- **Clone DTO:** `CloneError::CaseCollision` needed a `kind` in `tauri-app`'s `CloneErrorDto`
  (`"CaseCollision"`, exhaustive match) and the TS `CloneErrorKind` union; the dialog shows the
  message (its default case). No other DTO change.
- **Probe:** the `ic=false` raw clone row clones without a checkout, sets the config, then checks
  out forced. On the GitHub Windows runner libgit2 decided `core.symlinks=false` for fresh
  repositories despite the global opt-in, so the probe sets `core.symlinks=true` per repository
  on Windows (fresh clones and submodules still decide on their own). The temporary CI step also
  needed `fail-fast: false` on the `rust` matrix, so one OS failing doesn't cancel the others'
  tables; both are removed in Z2.
- **Walker layout:** the leaf operations live next to each walker (`beneath_unix.rs`,
  `beneath_windows.rs`, same function set) and `worktree_guard` dispatches; the shared
  `OnLink`/`OnMissing`/`Step`/`Walk` types are in `worktree_guard`. `clear_symlinked_ancestors`
  is gone (its only caller, undo's write, uses `replace_beneath`, whose walk clears the link).
  The old path-based non-unix fallbacks are gone too: unix and Windows are the only targets.
  The pure reparse-tag rules are in `worktree_guard/reparse.rs` (tested on every platform).
- **FIFO test:** libgit2's status never lists a FIFO, so a snapshot never read one; the hang was
  in undo's verify of a tracked file replaced by a FIFO. `undo_skips_a_fifo_that_replaced_a_file…`
  covers that (verified to time out on the pre-R code).
- **Commits:** R1–R4 landed as one commit, L1+L2 as one, W1+W2 as one.
- **Stress (Linux, debug build):** `write_swap_race_never_writes_outside` 199,966 of 200,000
  writes landed inside, outside file unchanged; `directory_swap_race_never_reads_outside`
  57,444 inside reads, 0 leaks.
- **Still path-based (stat/listing only, no write):** undo's `verify_directory_is_removable`
  (`read_dir`) and its `symlink_metadata` checks, `has_symlinked_ancestor`, `refuse_linked_path`
  and the `Scanner`; the refusal checks before libgit2 operations are the documented residual.

## Review Focus

1. **Nothing outside the repo is written, deleted or read through a link, even with a racing
   process**, for every raw I/O function. The `step`-seam tests (R1–R4) are deterministic
   proofs. Check that no `std::fs` call by **path** below the work tree remains in
   `worktree_guard` and `undo` (grep in Z2).
2. **`find_case_collision` has no false negatives for link-vs-folder** (both orders, nested,
   gitlinks, exact-name duplicates) and no refusal of file-vs-folder pairs (C2).
3. **A repo whose HEAD already has a collision still works** for unrelated operations; only
   operations that bring the collision in are refused (C3).
4. **Clone/submodule/worktree still populate the work tree** with progress and cancel
   (existing `tests/clone.rs`, `tests/submodule.rs`, `tests/worktree.rs` stay green).
5. **Windows walker** holds every component without `FILE_SHARE_DELETE` until the act is
   done, never opens a name-surrogate's target, and checks the file ID when reopening a
   non-surrogate leaf (W1).

---

## File Structure

| File | Tasks | Responsibility |
|---|---|---|
| `crates/git-core/tests/case_insensitive_links.rs` (new) | C1 | macOS/Windows probe and regression (self-skipping) |
| `crates/git-core/src/worktree_guard.rs` | C2, C3, R2–R4, W2 | `CaseCollision`, `fold_name`, `folds_case`, `find_case_collision`, hooks; raw I/O functions on the walkers |
| `crates/git-core/src/worktree_guard/beneath_unix.rs` (new) | R1 | `parent_beneath`, `stat_at`, `unlink_at`, `open_at`, `readlink_at`, `symlink_at`, `Step` seam |
| `crates/git-core/src/worktree_guard/beneath_windows.rs` (new) | W1 | pinned walk, `GetFileInformationByHandleEx`, `is_name_surrogate` |
| `crates/git-core/src/lib.rs` | C2 | `pub use worktree_guard::CaseCollision;` |
| `crates/git-core/src/{branch,merge,pick,rebase,stash,remote,submodule,reset,stage,undo,clone,worktree}.rs` | C3, C4 | `CaseCollision` variants; clone/worktree/submodule hooks |
| `crates/git-core/tests/case_collision.rs` (new) | C3, C4 | refusal per operation (portable) |
| `crates/repo-service/src/worker/remote.rs` | C3 | pass `CaseCollision` text through |
| `crates/git-core/src/undo.rs` | R4 | I/O through `read_beneath` / `remove_and_prune_beneath` / `replace_beneath` |
| `crates/git-core/src/launch.rs` | L1, L2 | extensions, trailing dot/space, desktop sniff |
| `.github/workflows/ci.yml` | C1, Z2 | temporary `--nocapture` probe step (added C1, removed Z2) |
| `CHANGELOG.md`, `docs/ARCHITECTURE.md`, `docs/LICENSE_COMPLIANCE.md`, spec §1.3 table | C1, Z1 | docs |

Workstreams (disjoint files, can run in parallel after C0):
- **C** (#156): C1 → C2 → C3 → C4
- **R** (#149 unix): R1 → R2 → R3 → R4
- **W** (#150 + #149 Windows): W1 → W2, after R1, because it mirrors R1's function set
- **L** (#131): L1 → L2

C3 and R2–R4 both edit `worktree_guard.rs`, in different functions. Run C and R sequentially
or rebase carefully. Z1/Z2 come last.

---

## Task C0: Pre-flight

- [ ] **Step 1:** `git status` clean on `fix/security-followups`; `git log -1` is the spec+plan commit.
- [ ] **Step 2:** Pre-push hook: `ln -sf "$(git rev-parse --show-toplevel)/scripts/hooks/pre-push" "$(git rev-parse --git-path hooks)/pre-push"`.
- [ ] **Step 3:** Baseline `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`. Note pre-existing failures in the PR description; don't fix unrelated ones.
- [ ] **Step 4:** Re-run the audit grep and compare with "Pre-flight notes". A new libgit2 work-tree writer or raw `std::fs` call below the work tree gets a row in C3/C4 or R4:
  `grep -rn -E "checkout_tree|checkout_head|checkout_index|\.worktree\(|submodule.*update|builder.clone|\.reset\(|stash_save|stash_apply|fs::(write|read|remove_file|remove_dir|create_dir_all|read_link|set_permissions)" crates/git-core/src`

---

## Task C1: Probe/regression binary for case-insensitive file systems (#156)

### Goal
A real-repo test that, on macOS and Windows CI, runs every work-tree-writing operation (raw
libgit2 and git-core) against trees holding `D -> outside` + `d/x` (and the reverse case). It
asserts no outside file changes, and prints a result table that fills spec §1.3.

### Depends on
C0.

### TDD requirement
This is the probe. Its assertion is "every outside file intact"; it is expected to pass
before the fix (spec §1.1 analysis), and a failing row is a confirmed vulnerability that
C3/C4 must turn green. Before pushing, check locally on Linux that the self-skip works
(`cargo test -p git-core --test case_insensitive_links -- --nocapture` prints
`SKIP case-sensitive file system`). Also check on Linux, with the skip temporarily inverted,
that every row runs and is intact (rows don't collide on a case-sensitive file system, which
proves the harness).

**Files:** Create `crates/git-core/tests/case_insensitive_links.rs`. Modify `.github/workflows/ci.yml` (temporary step), `CHANGELOG.md` (stub).

- [ ] **Step 1: Harness.**
  - `mod common;` (brings `hermetic_git!()`).
  - `fn case_insensitive(dir: &Path) -> bool`: write `probe`, check whether `PROBE` exists.
  - One `#[test] fn case_insensitive_links_never_write_outside()`:
    - Point `git2::opts::set_search_path(ConfigLevel::Global, …)` (unsafe in git2 0.21; add a
      `SAFETY:` comment saying this binary has one test, so no other thread uses libgit2) at
      `target/<profile>/browsitory-case-probe/` holding `.gitconfig` with
      `[core]\n\tsymlinks = true\n` (created, never deleted).
    - Try a file-system symlink in a temp dir. On failure print `SKIP links unsupported` and
      return.
- [ ] **Step 2: Fixtures.**
  - `fn evil_repo(order: Order) -> (TempDir, Repository, TempDir /*outside*/)`. Commits built
    with `TreeBuilder`:
    - `base` (`a.txt`);
    - `evil` = base + link entry `D` (`Order::LinkUpper`) or `d` (`LinkLower`) whose blob is
      the outside dir's absolute path, + `d/x` or `D/x` = `PWNED\n`;
    - `evil2` = evil with `x` = `PWNED2\n`.
  - Branches `main` = base, `evil`, `evil2`. A submodule fixture: a super-repo whose
    `.gitmodules` points (a plain local path, since `validate_remote_url_scheme` refuses
    `file://` but allows a path) at the evil repo with gitlink = `evil`.
  - `outside/x` = `OUTSIDE\n`.
- [ ] **Step 3: Rows** (spec §1.3 list).
  - Each row takes `(order, ignorecase: Probed | False)`, builds a fresh clone or copy, runs
    the op and records `Row { name, result: String, at_d: Kind, at_upper_d: Kind,
    outside_intact: bool }`.
  - `ignorecase=False` sets `core.ignorecase=false` in the repo config before the op. For
    clone, set it through the hermetic global `.gitconfig` of a **second** run: write the file,
    run the clone rows, restore.
  - git-core rows call the public API (`clone::clone_repo` with a no-op `CredentialProvider` /
    `TransferReporter` from `tests/clone.rs`, `branch::switch_branch`, `branch::checkout_commit`,
    `merge::start_merge` ff and `--no-ff`, `pick::cherry_pick`, `rebase::start_rebase`,
    `stash::apply_stash` of a stash holding `d/x` while `D` is in HEAD, `reset::reset_to(Hard)`,
    `undo::capture`/`restore` across the switch, `worktree::create_worktree`,
    `submodule::update_submodule`, `remote::pull_after_fetch` ff).
- [ ] **Step 4: Report.**
  - `eprintln!` a Markdown table row per row.
  - Assert `rows.iter().all(|r| r.outside_intact)`, with the failing rows in the message.
  - CHANGELOG stub under `## [Unreleased]` → `### Security`: `Hardening for case-insensitive
    file systems (macOS, Windows) against a repository holding a symbolic link and a folder
    whose names differ only in case (#156).`
  - Temporary CI step in the `rust` job after `Test`:
    `- name: Case-insensitive probe (temporary, #156)` /
    `run: cargo test -p git-core --test case_insensitive_links -- --nocapture`.
- [ ] **Step 5:** Commit `git-core: case-insensitive link probe (#156)`, push, and open the **draft** PR (`gh pr create --draft`, title "Security follow-ups: case-insensitive links, race-free work-tree I/O, Windows walk, runnable files", body lists #156 #149 #150 #131 and links #159). Copy the macOS and Windows tables from the CI logs into spec §1.3 and commit `docs: #156 probe results`. **If any row is not intact**, mark it in the spec. C3/C4 must make it pass, and Z1's CHANGELOG says "fixed" for that operation.

### Acceptance criteria
- [ ] The binary self-skips on Linux; CI macOS/Windows print full tables.
- [ ] Spec §1.3 table filled from CI.

### Out of scope
Fixing anything (C2–C4).

---

## Task C2: `find_case_collision` and `CaseCollision` (#156)

### Goal
A pure check over a tree or index that finds a link/gitlink entry and a folder of another entry
whose names fold to the same name (spec §1.4), and the `CaseCollision` error.

### Depends on
C0 (C1 may run in parallel).

### TDD requirement
Inline tests in `worktree_guard.rs`'s `mod tests`, portable (no file-system links: link entries
via `TreeBuilder::insert(name, blob, 0o120000)`):

```rust
#[test]
fn a_link_and_a_folder_differing_in_case_collide() {
    // tree: D -> "/x" (link), d/x (blob)
    let found = find_case_collision(&GuardSource::Tree(&tree), ALL).unwrap();
    assert_eq!(found, Some(CaseCollision { link: "D".into(), folder: "d".into() }));
}
#[test] fn the_reverse_order_collides() { /* d (link), D/x */ }
#[test] fn a_nested_link_collides_with_a_folder_of_another_case() { /* a/B (link), A/b/c */ }
#[test] fn a_gitlink_counts_as_a_link() { /* Sub (0o160000), sub/x */ }
#[test] fn unicode_case_and_hfs_ignorables_fold() { /* "Ä" link vs "ä/x"; "d\u{200c}" link vs "d/x" */ }
#[test] fn a_file_and_a_folder_differing_in_case_do_not_collide() { /* D (blob), d/x */ }
#[test] fn distinct_names_do_not_collide() { /* D (link), e/x; Dd/x */ }
#[test] fn paths_limit_the_check() { /* D link + d/x; paths ["d"] → None; ["D","d"] → Some */ }
#[test] fn an_index_source_is_checked_like_a_tree() {}
#[test] fn folds_case_reads_core_ignorecase() {
    // Linux: false by default, true after config.set_bool("core.ignorecase", true).
    // cfg(any(macos, windows)): always true.
}
#[test] fn case_collision_message_is_exact() { /* the Global Constraints copy */ }
```

**Files:** `crates/git-core/src/worktree_guard.rs`, `crates/git-core/src/lib.rs`.

- [ ] **Step 1:** Write the tests → `cargo test -p git-core worktree_guard` fails to compile.
- [ ] **Step 2: Implement.**
  - `fold_name` per spec §1.4.
  - `find_case_collision`: one pass over the index entries (stage 0; trees read through an
    in-memory `Index` as `Scanner::scan` does), with the same `selected(..., ignore_case=true)`
    paths filter.
  - Two maps `links: HashMap<Vec<u8>, Vec<u8>>` (folded → original) and
    `folders: HashMap<Vec<u8>, Vec<u8>>` (every strict prefix, folded → original). Insert, and
    look each new key up in the other map. The first hit wins, reported in git spelling
    (`String::from_utf8_lossy`).
  - `folds_case`: `cfg!(any(target_os = "macos", windows)) || core.ignorecase`.
  - `pub use worktree_guard::CaseCollision;` in `lib.rs`.
- [ ] **Step 3:** `cargo test -p git-core`; clippy. Commit `git-core: detect case-colliding link and folder entries (#156)`.

### Acceptance criteria
- [ ] All tests above pass on Linux and CI.

### Out of scope
Hooks (C3/C4). NFC/NFD (D4).

---

## Task C3: Refuse case collisions in safe and forced checkouts (#156)

### Goal
`refuse_linked_folders` checks each **incoming** source, and `prepare_forced_checkout` checks
its target (path-limited), when `folds_case(repo)`. Every operation's error enum carries
`CaseCollision`.

### Depends on
C2.

### TDD requirement
New `crates/git-core/tests/case_collision.rs` (`mod common;`), portable. Fixture:
`core.ignorecase=true` in the repo config; `main` = base; branch `evil` = base + link entry `D`
+ `d/x` built with `TreeBuilder` on top of base (no file-system link). Failing tests first:

- `switch_to_a_branch_with_a_case_collision_is_refused`: `branch::switch_branch(&repo, "evil")`
  → `Err(BranchError::CaseCollision(_))` with the exact message; HEAD and the work tree
  unchanged.
- `checkout_commit_…`, `merge_fast_forward_…`, `merge_commit_…` (main has an extra commit),
  `cherry_pick_of_a_commit_adding_the_collision_…`, `rebase_onto_…`, `stash_apply_…` (a stash
  whose tree holds the pair, built with `commit_tree`), `pull_fast_forward_…` (local remote):
  each refused with its enum's `CaseCollision`.
- `reset_hard_to_a_collision_commit_is_refused`; `discarding_d_x_alone_is_allowed` (index
  holds the pair; `stage::discard_file(&repo, "d/x")` path-limited → `Ok`).
- `a_repo_whose_head_has_a_collision_still_switches_to_a_clean_branch`. Commit the pair on
  `main` with `core.ignorecase=false` (Linux writes both), then set `ignorecase=true`. Switching
  to a branch without the pair → `Ok`; switching to `evil2` (still has it) → refused.
- `without_ignorecase_linux_does_not_refuse` (`#[cfg(target_os = "linux")]`):
  `core.ignorecase=false` → switch succeeds.

**Files:** `worktree_guard.rs` (hooks), `branch.rs`, `merge.rs`, `pick.rs`, `rebase.rs`,
`stash.rs`, `remote.rs`, `submodule.rs`, `reset.rs`, `stage.rs`, `undo.rs` (error variants),
`crates/repo-service/src/worker/remote.rs`, new `tests/case_collision.rs`.

- [ ] **Step 1:** Write the tests → compile failure (no variants).
- [ ] **Step 2: Implement.**
  - Add `#[error(transparent)] CaseCollision(#[from] crate::CaseCollision)` to `BranchError`,
    `MergeError`, `PickError`, `RebaseError`, `StashError`, `RemoteError`, `SubmoduleError`,
    `ResetError`, `StageError`, `UndoError`.
  - `refuse_linked_folders` gains `E: From<CaseCollision>`. When `folds_case`, it checks
    `find_case_collision` for each `incoming` source before the link scan.
  - `prepare_forced_checkout` gains the same bound and checks `target` with `paths`.
  - Fix every call site's turbofish.
  - Add `RemoteError::CaseCollision(_)` to the pass-through arm in `repo-service`.
  - Check `merge.rs`'s local `refuse_linked_folders` wrapper and `stash.rs`'s
    `WouldOverwrite` mapping: `CaseCollision` must surface as itself.
- [ ] **Step 3:** `cargo test --workspace`; clippy; `symlink_write_through.rs` unchanged and green. Commit `git-core: refuse case-colliding links in checkouts (#156)`.

### Acceptance criteria
- [ ] `cargo test -p git-core --test case_collision` passes on Linux and CI.
- [ ] Pull's refusal reaches the UI as the message, not "pull failed".

### Out of scope
Clone, worktree add, submodule update (C4).

---

## Task C4: Guard fresh checkouts: clone, worktree add, submodule update (#156)

### Goal
The three libgit2 writers without a guard check the tree before anything is written (spec §1.4
table, D2-A).

### Depends on
C3.

### TDD requirement
- `clone.rs` unit tests (in `src`, so they can pass the internal policy; the fresh clone's
  config can't be preset). Make `clone_repo` delegate to
  `pub(crate) fn clone_repo_with(opts, credentials, reporter, cancel, policy: CasePolicy)`,
  where `CasePolicy::{FromRepo, Always}` and `FromRepo` = `folds_case`:
  - `cloning_a_repo_with_a_case_collision_fails_and_cleans_up`: local source repo with the
    pair at HEAD, `CasePolicy::Always` → `Err(CloneError::CaseCollision(_))`; `dest` removed
    (`Created`) or emptied (`ExistingEmpty`).
  - `cloning_a_clean_repo_still_checks_out_and_reports_progress`: files present, at least one
    `TransferPhase::Updating` report, `CloneOutcome.path` correct.
  - `cancel_during_checkout_still_cancels`: the existing cancel test pattern.
- `tests/worktree.rs`: `creating_a_worktree_on_a_branch_with_a_collision_is_refused`
  (`core.ignorecase=true` in the main repo) → `Err(WorktreeError::CaseCollision(_))`; no
  worktree directory or `.git/worktrees/<name>` left behind.
- `submodule.rs` unit tests with `CasePolicy::Always` (internal `update_submodule_with`):
  `updating_a_submodule_whose_commit_has_a_collision_is_refused` (the sub work tree holds no
  `d/x`); `updating_a_clean_submodule_populates_it` (fresh and already-cloned); recursive
  update still works.

**Files:** `clone.rs`, `worktree.rs`, `submodule.rs` (+ their tests).

- [ ] **Step 1:** Write the tests → fail.
- [ ] **Step 2: Implement.**
  - **Clone:** `RepoBuilder::with_checkout(CheckoutBuilder::new().dry_run())`. After `clone`:
    `HEAD` tree → `find_case_collision` (policy) → on hit `cleanup` + `Err`. Otherwise
    `repo.checkout_head(Some(force + the existing notify/progress callbacks))`, mapped through
    `classify`, with cleanup on error and cancel. An empty remote (unborn HEAD) skips both.
  - **Worktree:** resolve the reference's tree and run `find_case_collision` before
    `repo.worktree`.
  - **Submodule:** `SubmoduleUpdateOptions::new().checkout(dry_run builder)`. Then
    `submodule.open()`, check the sub HEAD tree, then
    `refuse_linked_folders::<SubmoduleError>(&sub, &[Incoming::Checkout(&tree)])`, then
    `sub.checkout_head(Some(safe().recreate_missing(true)))`. Do the same in
    `update_nested_submodules`. **Check** with the existing `tests/submodule.rs` that an
    already-cloned submodule moved to a new commit updates its files. If `checkout_head` with
    the safe strategy doesn't apply the change (baseline = the new HEAD), use
    `checkout_tree(target, safe)` with `baseline(old HEAD tree)` instead.
- [ ] **Step 3:** `cargo test --workspace`; clippy. Commit `git-core: check clone, worktree and submodule trees for case collisions (#156)`.

### Acceptance criteria
- [ ] Existing clone/worktree/submodule tests unchanged and green.
- [ ] The C1 probe shows the git-core clone/worktree/submodule rows as refused on CI.

### Out of scope
Changing what libgit2 does without Browsitory (raw rows of C1).

---

## Task R1: `beneath_unix` walker (#149)

### Goal
The shared `openat` primitives and the `Step` seam (spec §2.2). `open_beneath` is rebuilt on
them with no behavior change.

### Depends on
C0.

### TDD requirement
`#[cfg(unix)]` unit tests in `beneath_unix.rs`, with the same fixture as `worktree_guard`'s
tests (move `repo()`/`outside()` into a shared `#[cfg(test)] pub(super) mod fixture` if
needed):
- `parent_beneath_refuses_a_link_on_the_way` (`Refuse` → `None`);
- `parent_beneath_creates_missing_folders` (`Create`);
- `parent_beneath_clears_a_link_on_the_way` (`Clear`: the link is gone, the outside dir and
  its files are intact, and a real folder now exists);
- `a_folder_swapped_for_a_link_after_it_was_opened_is_not_followed`: the `step` callback,
  on `Step::Opened("d")`, renames `d` → `d-moved` and symlinks `d` → outside. The returned
  parent fd is `d-moved` (check by `fstat` inode equal to `d-moved`'s), never outside.
- Every existing `open_beneath_*` test still passes.

**Files:** create `crates/git-core/src/worktree_guard/beneath_unix.rs`; modify `worktree_guard.rs` (`#[cfg(unix)] mod beneath_unix;`, `open_beneath` delegates).

- [ ] **Step 1:** Tests → fail.
- [ ] **Step 2: Implement.** `enum Step<'a> { Opened(&'a OsStr), Created(&'a OsStr), Cleared(&'a OsStr) }`; `parent_beneath` per spec §2.2. Wrappers return `io::Result` with `last_os_error`. `O_CLOEXEC` everywhere.
- [ ] **Step 3:** `cargo test -p git-core`; clippy. Commit `git-core: openat walker for work-tree I/O (#149)`.

### Acceptance criteria
- [ ] No behavior change in `open_beneath`; `directory_swap_race_never_reads_outside` (ignored) still 0 leaks.

### Out of scope
Writes and deletes (R2–R4).

---

## Task R2: Race-free `write_beneath` / `remove_beneath` (#149)

### Depends on
R1.

### TDD requirement
`#[cfg(unix)]` tests in `worktree_guard.rs` (seam via a private `write_beneath_with(…, step)`):
- `write_beneath_lands_in_the_opened_folder_when_it_is_swapped_mid_write`;
- `remove_beneath_does_not_follow_a_folder_swapped_mid_walk`;
- `write_beneath_keeps_the_exec_bit_of_an_existing_file` (passes before and after: a
  regression guard);
- `write_beneath_on_a_fifo_errors_without_blocking` (run in a thread with a 5 s
  `recv_timeout`);
- `remove_beneath_returns_false_for_a_leaf_that_became_a_folder`;
- `#[ignore]` stress `write_swap_race_never_writes_outside` (200k iterations, swapper thread
  as in `directory_swap_race_never_reads_outside`).

Existing `write_beneath_*`/`remove_beneath_*`/`remove_and_write_beneath_work_on_plain_files`
unchanged.

- [ ] **Step 1:** Tests → swap tests fail (the current implementation writes outside).
- [ ] **Step 2: Implement** per spec §2.2 table. Contracts unchanged (`Ok(false)` on a linked ancestor or non-plain path).
- [ ] **Step 3:** `cargo test -p git-core` (incl. `tests/merge.rs`, `tests/symlink_write_through.rs`); run the stress test once with `--ignored` and note the numbers in this plan's pre-flight notes. Commit `git-core: race-free write_beneath and remove_beneath (#149)`.

---

## Task R3: Race-free `clear_hazards` / `clear_symlinked_ancestors` (#149)

### Depends on
R1.

### TDD requirement
`#[cfg(unix)]`:
- `clear_hazards_removes_a_parent_that_became_a_link_instead_of_following_it`: hazard
  `Leaf("d/tc")`, and the step seam swaps `d` for a link to outside before the leaf. Afterwards
  `d` is gone, and outside's `tc`-named file is intact.
- `clear_hazards_leaves_a_leaf_that_became_a_folder`: `unlinkat` gives `EISDIR`, the folder
  is kept, the call returns `Ok`.
- `clear_symlinked_ancestors_unlinks_through_the_parent_descriptor`.

The existing `clear_hazards_*` tests and all of `symlink_write_through.rs` stay green.

- [ ] **Step 1–3** as above. `remove_link` keeps its Windows branch until W2. Commit `git-core: race-free hazard clearing (#149)`.

---

## Task R4: Undo's raw I/O through the walker (#149)

### Goal
`read_worktree_file`, `remove_existing` + `prune_empty_parents`, `write_worktree_file` +
`set_executable` call `worktree_guard::{read_beneath, remove_and_prune_beneath,
replace_beneath}` (spec §2.2). No `std::fs` by path below the work tree is left in `undo.rs`.

### Depends on
R1 (R2/R3 for shared helpers).

### TDD requirement
- `worktree_guard` unit tests (`#[cfg(unix)]`, seam): `read_beneath_does_not_follow_a_swapped_folder`,
  `read_beneath_returns_none_for_a_fifo_without_blocking`, `read_beneath_reads_a_link_target`,
  `replace_beneath_writes_into_the_opened_folder_when_swapped`,
  `replace_beneath_sets_and_clears_the_exec_bit`, `replace_beneath_creates_a_symlink_leaf`,
  `remove_and_prune_beneath_removes_emptied_parents_only_inside`.
- `tests/undo.rs`: new `#[cfg(unix)] undo_snapshot_skips_an_untracked_fifo` (it hangs today:
  write it with a thread + timeout so the failure is a timeout, not a hang). All existing undo
  tests unchanged.
- `tests/stash.rs` unchanged (`worktree_blob` uses `read_worktree_file`).

**Files:** `worktree_guard.rs`, `beneath_unix.rs`, `undo.rs`.

- [ ] **Step 1–2:** Tests → fail; implement; `undo.rs` keeps its mode logic (`core.filemode`, `core.symlinks`, `previous_mode`) and only swaps I/O. Non-unix keeps the current code until W2.
- [ ] **Step 3:** `cargo test --workspace`; clippy. Commit `git-core: undo reads and writes the work tree race-free (#149)`.

---

## Task W1: Windows pinned walker and `open_beneath` (#150)

### Goal
`beneath_windows.rs` implements the spec §3.2 walk (D3-A). `open_beneath` uses it: mid-path
links refused, nothing contacted through a name surrogate, non-surrogate reparse leaves
reopened with a file-ID check.

### Depends on
R1 (mirrors its function set).

### TDD requirement
- Portable unit test `reparse_tags_are_classified` for `fn is_name_surrogate(tag: u32) -> bool`
  and `fn leaf_action(attrs: u32, tag: u32) -> LeafAction { Plain, Refuse, Reopen }` (spec §3.3
  tag list). Compiled `#[cfg(any(windows, test))]` so it runs on Linux too.
- `#[cfg(windows)]` tests in `worktree_guard.rs`:
  - `open_beneath_refuses_a_junction_to_an_inside_folder` (**fails before**);
  - `open_beneath_refuses_a_junction_to_an_outside_folder`;
  - `open_beneath_refuses_a_file_symlink_leaf` (skip without privilege);
  - `open_beneath_reads_through_plain_folders`;
  - `a_junction_swapped_in_after_the_parent_was_opened_is_not_followed` (step seam; on
    Windows the swap **fails** with a sharing violation, so the test asserts that the rename
    fails while the walk holds the folder, which is the pinning guarantee);
  - helper `fn junction(link: &Path, target: &Path)` via `cmd /C mklink /J`.

**Files:** create `crates/git-core/src/worktree_guard/beneath_windows.rs`; modify `worktree_guard.rs`.

- [ ] **Step 1:** Tests; push and see the junction-inside test fail on `windows-latest`.
- [ ] **Step 2: Implement.**
  - `OpenOptionsExt::{access_mode, share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE),
    custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)}`.
  - Hand-declared `GetFileInformationByHandleEx` (`#[link(name = "kernel32")]`, next to
    `GetFinalPathNameByHandleW`) with `FILE_ATTRIBUTE_TAG_INFO { file_attributes: u32,
    reparse_tag: u32 }` (class 9) and `FILE_ID_INFO { volume_serial_number: u64,
    file_id: [u8; 16] }` (class 18), `#[repr(C)]`.
  - `SAFETY:` comments on every call. Keep the `final_path` assertion for reads.
- [ ] **Step 3:** Windows CI green; local `cargo clippy --target x86_64-pc-windows-gnu` (stubbed git2) or CI clippy. Commit `git-core: Windows open_beneath walks components and refuses links (#150)`.

### Acceptance criteria
- [ ] Junction mid-path (inside or outside) → `None`; plain nested file → `Some`.
- [ ] Manual (if a Windows machine with OneDrive is available, else listed in the PR as untested): preview of an online-only OneDrive file loads.

---

## Task W2: Windows raw writes on the walker (#149)

### Goal
`write_beneath`, `remove_beneath`, `clear_hazards`/`remove_link`, `clear_symlinked_ancestors`,
`read_beneath`, `remove_and_prune_beneath` and `replace_beneath` use the pinned walk on Windows
(spec §2.3). No Windows path keeps a check-then-act by path through an unpinned folder.

### Depends on
W1, R2–R4.

### TDD requirement
`#[cfg(windows)]`, junction-based:
- `write_beneath_refuses_a_junction_ancestor` (outside untouched);
- `remove_beneath_refuses_a_junction_ancestor`;
- `clear_hazards_removes_the_junction_not_its_target`;
- `replace_beneath_clears_a_junction_on_the_way`;
- `undo_restore_through_a_junction_writes_inside` (in `tests/undo.rs`, `#[cfg(windows)]`).

The existing portable tests pass on Windows.

- [ ] **Step 1–3** as W1. Commit `git-core: Windows work-tree writes through pinned folders (#149, #150)`.

---

## Task L1: Runnable extensions and trailing dot/space (#131)

### TDD requirement
In `launch.rs` `mod tests`:
- `may_execute_refuses_the_added_extensions`: `x.wsc`, `x.psc1`, `x.ps1xml`, `x.mht`,
  `x.mhtml`, `x.hlp`, `x.iso`, `x.img`, `x.vhd`, `x.vhdx`, `x.dmg`, on every `Os`.
- `trailing_dots_and_spaces_do_not_hide_the_extension`: `x.exe.`, `x.bat `, `x.cmd. .`,
  `X.EXE.` → runnable; `notes.txt.` → not.

- [ ] Tests → fail; add the extensions (one comment line on disk images mounting); `lower_extension` trims trailing `.`/` ` from the file name before `Path::extension` (operate on `file_name()` as `OsStr` → `to_str`; non-UTF-8 → empty as today). Commit `git-core: refuse more runnable files when opening with the default app (#131)`.

---

## Task L2: `[Desktop Entry]` sniff variants (#131)

### TDD requirement
`looks_runnable_detects_desktop_entry_variants`:
- `[KDE Desktop Entry]\n`, `[Desktop Action new]\n`, `\x0b[Desktop Entry]\n` → runnable;
- 2 KiB of `# comment\n` lines then `[Desktop Entry]` → runnable via `file_may_run` (file on
  disk);
- 70 KiB of comments with no header → runnable (cap reached, fail closed);
- 10 lines of comments then EOF → not runnable;
- existing `# Notes\n[Desktop Entry] is a header\n` → not runnable.

- [ ] Tests → fail; implement per spec §4: `is_desktop_entry` returns `Option<bool>` (undecided at the end of a truncated head); `file_may_run` reads on up to 64 KiB only when undecided. Commit `git-core: desktop launcher sniff covers KDE, action groups and long comment headers (#131)`.

---

## Task Z1: Docs and changelog

**Files:** `CHANGELOG.md`, `docs/ARCHITECTURE.md`, `docs/LICENSE_COMPLIANCE.md`, spec §1.3.

- [ ] **CHANGELOG** `## [Unreleased]` → `### Security` (replace C1's stub):
  - `On case-insensitive file systems (macOS, Windows), switching, merging, pulling, cherry-picking, rebasing, applying a stash, resetting, cloning, adding a worktree or updating a submodule refuses a commit that holds a symbolic link and a folder whose names differ only in case, instead of letting the checkout drop or misplace files (#156).` (Say "could write outside the repository" only if C1's CI table showed a write-through.)
  - `Conflict resolution, aborting a merge, undo/redo and the removal of symbolic links before a checkout act on the folders they checked, so a folder swapped for a link at the same moment can't redirect them outside the repository (#149).`
  - `Windows: the image preview no longer follows a junction or symbolic link in the middle of a path (including to a network share) and loads OneDrive online-only files (#150).`
  - `Opening a file with the system default app also refuses .wsc, .psc1, .ps1xml, .mht, .mhtml, .hlp and disk images (.iso, .img, .vhd, .vhdx, .dmg), names ending in a dot or space, and more desktop launcher variants (#131).`
- [ ] **ARCHITECTURE.md**, "Worktree writes and symlinks":
  - Add the case-collision rule (where it's checked, `folds_case`, links-last note).
  - Replace the raw-I/O residual-risk sentence: raw I/O is now race-free (unix `openat` walk,
    Windows pinned walk); what's left is libgit2's own writes and the check-then-libgit2
    refusals.
  - Replace the Windows `open_beneath` paragraph.
  - Note that undo's I/O lives in `worktree_guard`.
- [ ] **LICENSE_COMPLIANCE.md:** extend the `libc` row's usage text; no new row (D3-A/D4-A).
- [ ] Commit `Docs and changelog for the security follow-ups`.

## Task Z2: Verification and PR

- [ ] Remove the temporary CI probe step (`ci.yml`), keeping the test binary.
- [ ] `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
- [ ] Grep that no path-based `std::fs` call below the work tree remains in `worktree_guard.rs`/`undo.rs` outside `cfg(test)`: `grep -n "fs::" crates/git-core/src/worktree_guard.rs crates/git-core/src/undo.rs`.
- [ ] Stress tests once: `cargo test -p git-core -- --ignored swap_race` → 0 leaks; record the numbers in the PR.
- [ ] CI green on ubuntu/macOS/Windows, including `case_insensitive_links` on macOS/Windows.
- [ ] Mark the PR ready; body lists D1–D5 answers, the §1.3 table, the stress numbers, "Closes #156, #149, #150, #131" and "p4merge BASE split to #159".

---

## Decision hooks

| Decision | If the user picks… | Change |
|---|---|---|
| D1-B | only `core.ignorecase` | C2 `folds_case` drops the `cfg!` term; the C2 test becomes config-only on every OS |
| D1-C | always | `folds_case` returns `true`; `without_ignorecase_linux_does_not_refuse` flips to "refuses"; CHANGELOG wording "on every platform" |
| D2-B | keep the clone unchecked-out | C4 clone returns `Ok(CloneOutcome { checkout_refused: Some(msg), .. })` (new field) and doesn't clean up; DTO + frontend toast (new task C5: `repo-service`, `tauri-app`, `vscode-sidecar`, `RepoClient`, `CloneDialog`) |
| D2-C | don't check clone | drop the clone part of C4 (worktree and submodule stay) |
| D3-B | `windows-sys` | W1/W2 use `windows-sys` 0.61 (`NtCreateFile` + `RootDirectory`, delete by handle); Z1 adds the license row (`cargo info windows-sys@0.61.2`) in the dependency commit; drop the pinning test, add a relative-open test |
| D3-C | hand-written NT FFI | as D3-B without the dependency; add a layout test (`size_of`/`align_of` of each struct, `cfg(windows)`) |
| D4-B | `unicode-normalization` | C2 `fold_name` NFC-normalizes; new test `nfc_and_nfd_collide`; license row |
| D4-C | ASCII only | C2 drops the Unicode test; spec residual widens |
| D5-B | temporary workflow | C1 marks the test `#[ignore]`, adds `.github/workflows/probe-156.yml` (`workflow_dispatch` + push to branch), deleted in Z2 |
| D5-C | manual probe | C1 keeps the binary; the user runs it and pastes the tables; no CI step |

# Lock and Security Leftovers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close nine follow-ups in two PRs:

- PR 1 (`feat/lock-security-bundle`):
  - **#175:** the state lock is released for forked copies, waits briefly after a timeout, and
    `finish` no longer records an operation that ended outside (also the lock flakes of #182);
  - **#174:** stash save and worktree add hold the state lock;
  - **#166:** NFC/NFD and `ẞ`/`ß` fold together;
  - **#162:** worktree add probes the destination's case folding;
  - **#165:** race-free head reads, key-file desktop sniffing, `.kdelnk`.
- PR 2 (`feat/windows-hardening`):
  - **#168:** POSIX-semantics deletes;
  - **#164:** in-place junction tests;
  - **#163:** folders created relative to the held parent;
  - **#180:** hooks start suspended and join their job before running.

**Architecture:**

- `repo-service::worker`: `state_lock.rs` (`Drop`, `CONTENDED_WAIT`), `undo.rs` (`locks_state`,
  `settle`/`finish`), `mod.rs` (the acquire loop, tests).
- `git-core`: `worktree_guard.rs` (`fold_name`, `dir_folds_case`), `worktree.rs`, `launch.rs`
  (`read_head`, `is_desktop_entry`, `RUNNABLE_EXTENSIONS`), `merge.rs` (`mark_resolved`),
  `worktree_guard/beneath_windows.rs` (`delete_by_handle`, `create_dir_in`),
  `process.rs` + `process/job_windows.rs` (`CREATE_SUSPENDED`, `resume`).

**Tech Stack:** Rust (`git2` 0.21 / libgit2 1.9, std `File::lock` API, `libc` 0.2 on unix,
`unicode-normalization` 0.1, hand-written kernel32/ntdll FFI on Windows).

**Spec:** `docs/superpowers/specs/2026-10-31-lock-security-bundle-design.md`. This plan
implements the **provisional** choice of each decision D1–D12. "Decision hooks" at the end
lists what changes for the other options.

---

## Global Constraints

- **Branches:** PR 1 on `feat/lock-security-bundle` (Parts A–C, Z), PR 2 on
  `feat/windows-hardening` (Part W), both from `main`. Each PR is opened only after its gate
  task (Z2 / W5) passes.
- **Dependencies:** PR 1 adds `unicode-normalization` to `crates/git-core/Cargo.toml` (Task B2):
  verify with `cargo info unicode-normalization`, add a `docs/LICENSE_COMPLIANCE.md` row in the
  same commit, and keep `python3 scripts/check-license-compliance.py` green. PR 2 adds none:
  `#[link(name = "kernel32")]` / `#[link(name = "ntdll")] extern "system"` blocks, as in
  `beneath_windows.rs` and `job_windows.rs`. **Not** `windows-sys`.
- **Tests use real temp-dir repos** (`tempfile` + `git2::Repository::init`), never a mocked
  `Repository`:
  - worker tests: `state_lock_tests` in `worker/mod.rs` (`init_repo`, `write_file`,
    `commit_all`, `hold`, `conflicting_branches`) and `paused_sequence`/`side_commits_with_a_conflict`
    in the sequence tests module (~l. 3356);
  - git-core integration tests start with `mod common;` or `test_support::hermetic_git!();`.
- **Platform gating:** FIFO and fork tests `#[cfg(unix)]`; the probe-is-false test
  `#[cfg(target_os = "linux")]`; the probe-is-true and NFD tests `#[cfg(target_os = "macos")]`
  or `#[cfg(any(target_os = "macos", windows))]`; Part W `#[cfg(windows)]`.
- **Windows code can't be compiled here** (no mingw). Type-check it with a throwaway crate under
  `target/winprobe` (`#[path]`-includes of the file under test plus minimal stubs for its
  `super::` imports), `cargo clippy --target x86_64-pc-windows-gnu`, then delete it. CI is the
  authority (D12-A).
- **Never** touch `~/.local/share/com.browsitory.app`, the real config dir or `~/.gnupg`. Probes
  go under `target/` and are deleted afterwards.
- **No new user-visible string.** `UNDO_BUSY`/`DISCARD_BUSY` are unchanged.
- **CHANGELOG:** every push touching `crates/` needs a `CHANGELOG.md` change (pre-push hook).
  - PR 1: Task A1 adds a stub `### Lock and security follow-ups` at the top of
    `## [Unreleased]` (above `### Test hygiene`); Task Z1 writes the final text.
  - PR 2: Task W1 adds `### Windows hardening` at the top; Task W5 writes the final text.
- **Commits:** one per task unless noted, message `<area>: <what> (#NNN)`, each ending with the
  session's `Co-Authored-By`/`Claude-Session` trailers.

## Pre-flight

- [ ] `git status` is clean on the branch, rebased on `origin/main` if `main` moved. If it did,
  re-check the line references below.
- [ ] `cargo test -p git-core -p repo-service` passes before any change. Record the counts in
  "Pre-flight notes".
- [ ] Link the pre-push hook (CLAUDE.md "Git hooks") if this worktree doesn't have it.

Reconciled with the code at 5ae3d42e on 2026-10-31:

- `crates/repo-service/src/worker/state_lock.rs`: `LOCK_WAIT` l. 17–20, `StateGuard` l. 30,
  `acquire` l. 57, `open_file` l. 84, tests l. 96–242 (the 2 s retry l. 164–167).
- `crates/repo-service/src/worker/undo.rs`: `UNDO_BUSY`/`DISCARD_BUSY` l. 16–24, `plan_for`
  l. 32, `locks_state` l. 122, `reply_busy` l. 143, `UndoRecorder` l. 292, `exclusive` l. 321,
  `stale` l. 377, `settle` l. 391, `begin` l. 420, `finish` l. 440, tests (`hold` l. 785,
  `>= LOCK_WAIT` l. 888).
- `crates/repo-service/src/worker/mod.rs`: acquire loop l. 815–859, `track` ~l. 864, gate
  l. 881, `SaveStash` dispatch l. 1239, `CreateWorktree` l. 1134, pause l. 1622, `finish`
  l. 1625, `PauseHook`/`PauseControl` l. 1657–1730, `paused_sequence` l. 3356,
  `state_lock_tests` l. 6627 (`locks_state_covers_…` l. 6705, `a_locked_command_runs_after_…`
  l. 6848, `only_the_first_…` l. 6869, `undo_replies_busy_…` l. 6951, retry l. 6964).
- `crates/repo-service/src/worker/gate.rs`: `gate_during_operation` l. 36 (worktree/stash
  l. 64–67), `marker` l. 144.
- `crates/git-core/src/worktree_guard.rs`: `fold_name` l. 351, `folds_case` l. 366,
  `CasePolicy` l. 378, `find_case_collision` l. 491, `refuse_case_collision` l. 648,
  `read_beneath` l. 949, `open_beneath` l. 990, tests (`unicode_case_and_hfs_ignorables_fold`
  l. 1326, `folds_case_reads_core_ignorecase` l. 1623).
- `crates/git-core/src/worktree.rs`: `create_worktree` l. 77 (collision check l. 104).
- `crates/git-core/src/launch.rs`: `RUNNABLE_EXTENSIONS` l. 172 (`desktop` l. 237),
  `is_desktop_entry` l. 337, `file_may_run` l. 504, `read_head` l. 537, sniff tests l. 843–915.
- `crates/git-core/src/merge.rs`: `mark_resolved` l. 464 (read l. 479).
- `crates/git-core/src/worktree_guard/beneath_windows.rs`: `delete_by_handle` l. 140, `walk`
  l. 308 (`create_dir` l. 343), `create_leaf` l. 440, `remove_and_prune` l. 572, `replace` l. 598,
  tests l. 627 (`junction` l. 639).
- `crates/git-core/src/process.rs`: `detach` l. 207, `run_captured` l. 277 (`Job::assign` l. 302);
  `process/job_windows.rs` (72 lines).
- Tests: `crates/git-core/tests/worktree.rs` (collision test l. 133),
  `tests/case_collision.rs`, `tests/case_insensitive_links.rs` (`case_insensitive` l. 80).

## Pre-flight notes

(fill in: baseline counts, whether `main` moved, decisions confirmed by the user, deviations)

---

## Part A: the state lock (#175, #174), repo-service only

### Task A1: release the lock for every descriptor copy (#175, D1-A)

**Files:** `crates/repo-service/src/worker/state_lock.rs`; `crates/repo-service/src/worker/mod.rs`
(tests); `CHANGELOG.md` (stub).

- [ ] **Write the failing tests first** (`state_lock.rs` tests):
  - `fn a_copied_descriptor_does_not_keep_a_released_lock()`:
    - `let guard = held(a.try_acquire());`
    - `let copy = guard._file.try_clone().unwrap();` (stands in for a forked child's copy)
    - `drop(guard);`
    - `held(b.try_acquire());` (Busy today), then `drop(copy)`.
  - `#[cfg(unix)] fn a_child_forked_while_the_lock_was_held_does_not_keep_it()`:
    - hold the lock;
    - on a helper thread: `Command::new("sh").arg("-c").arg(":")` with
      `unsafe { pre_exec(|| { std::thread::sleep(Duration::from_secs(1)); Ok(()) }) }`
      (`nanosleep` is async-signal-safe), `.spawn()` (blocks until exec) then `.wait()`;
    - main thread: sleep 200 ms, `drop(guard)`, assert `try_acquire` is `Held` (Busy today),
      join.
  - Run `cargo test -p repo-service a_copied_descriptor a_child_forked` and confirm both fail on
    Linux.
- [ ] **Implement:** `impl Drop for StateGuard { fn drop(&mut self) { let _ = self._file.unlock(); } }`.
  Module doc: a child between fork and exec holds a copy of every descriptor; `flock`'s
  `LOCK_UN` on ours releases it for all copies (`flock(2)`); OFD locks wouldn't help (inherited
  across fork). Rename `_file` to `file` if clippy prefers.
- [ ] Make the #176 workarounds strict again:
  - `a_panicking_holder_thread_releases_the_lock`: `held(StateLock::open(&repo).try_acquire())`,
    delete the comment;
  - `undo_replies_busy_while_another_holder_keeps_the_lock` (`mod.rs` l. 6964–6974): a single
    `handle.undo()` after `drop(guard)`, delete the comment.
- [ ] `CHANGELOG.md`: add `### Lock and security follow-ups` at the top of `## [Unreleased]` with
  a stub bullet.
- [ ] `cargo test -p repo-service` (run it 3× to watch for flakes) and
  `cargo clippy -p repo-service --all-targets -- -D warnings`.
- [ ] Commit `state lock: release it for forked descriptor copies (#175, #182)`.

### Task A2: a short wait after a timeout (#175, D2-A)

**Files:** `state_lock.rs`; `mod.rs`.

- [ ] **Write the failing test first** (`state_lock_tests`):
  `fn after_a_timeout_a_locked_command_waits_briefly_for_the_lock()`:
  - hold the lock;
  - `stage_file("a.txt")` (times out after `LOCK_WAIT`);
  - spawn a thread that drops the guard after 30 ms;
  - `stage_file("b.txt")`;
  - assert `get_undo_state().undo == Some("Stage b.txt")`. Today it is `None`: it tried once
    and ran unrecorded.
- [ ] **Implement:**
  - `state_lock.rs`: `pub(super) const CONTENDED_WAIT` = 1 s (`cfg(test)`: 100 ms); `LOCK_WAIT`
    under `cfg(test)` 300 ms → 1 s.
  - `mod.rs` l. 817–831: `contended` selects `CONTENDED_WAIT` instead of `Duration::ZERO`;
    update the comment.
- [ ] Rewrite `only_the_first_locked_command_waits_while_the_lock_stays_held`: the second
  `stage_file` asserts `start.elapsed() < LOCK_WAIT` (was `< 200 ms`) and
  `>= CONTENDED_WAIT`. Keep the `Unstage b.txt` tail.
- [ ] `cargo test -p repo-service` and clippy.
- [ ] Commit `state lock: wait a second after a timeout instead of trying once (#175, #182)`.

### Task A3: stash save and worktree add hold the lock (#174)

**Files:** `undo.rs`; `mod.rs` (tests).

- [ ] **Write the failing tests first** (`state_lock_tests`):
  - extend `locks_state_covers_every_recordable_and_operation_ending_command` with
    `SaveStash { options: StashOptions::default(), reply }` and `CreateWorktree { .. }`;
  - `fn stash_save_waits_for_the_lock_and_sees_a_sequence_stored_meanwhile()`:
    - `side_commits_with_a_conflict()` (move or `pub(super)` it if it lives in another test
      module) plus a worker;
    - edit `b.txt`;
    - `let guard = hold(&repo);`
    - spawn `handle.save_stash(Default::default())`;
    - sleep 100 ms;
    - with git-core start the sequence (`git_core::pick::cherry_pick_many` or the fixture's
      start) and `super::sequence_store::SequenceStore::open(&repo).save(&state)`;
    - `drop(guard)`;
    - assert the reply is `Err(OperationInProgress.to_string())` and `refs/stash` is absent.
      Today the stash is saved during the sleep.
  - The same for `create_worktree(.., branch = the sequence's branch, ..)`: refused, the path
    doesn't exist.
- [ ] **Implement:** `locks_state` adds `| Command::SaveStash { .. } | Command::CreateWorktree { .. }`.
  Update its doc comment ("and the gated commands that start no operation: stash save,
  worktree add").
- [ ] `cargo test -p repo-service` and clippy.
- [ ] Commit `worker: hold the state lock for stash save and worktree add (#174)`.

### Task A4: don't record an entry that was stale before the command (#175, D3-A)

**Files:** `undo.rs`; `mod.rs` (tests).

- [ ] **Write the failing test first** (`state_lock_tests`):
  `fn a_merge_committed_outside_while_the_other_window_held_the_lock_is_not_recorded()`:
  - `conflicting_branches()`; `(worker, pause) = Worker::spawn_with_pause_before_finish(..)`;
  - `start_merge("feature")` → `Conflicted`;
  - outside: write the resolution, add it to the index, commit with parents HEAD +
    `MERGE_HEAD` (git2), `repo.cleanup_state()`;
  - `let guard = hold(&repo); pause.arm();`
  - thread `handle.get_status()`; `pause.wait_reached(); drop(guard); pause.go();` join;
  - assert `handle.get_undo_state().unwrap() == no_undo()`. Today it is
    `Some("Merge feature")`.
- [ ] **Implement** in `UndoRecorder`:
  - field `stale_unsettled: Option<String>`;
  - `settle`: set it to `None` first; when `stale(..)` and `exclusive(guard, ZERO)` is `None`,
    store the pending entry's `operation`;
  - `finish`, no-`active` branch, after `exclusive` and the second `marker().is_some()` check: if
    `self.stacks.pending.as_ref().map(|p| &p.operation) == self.stale_unsettled.as_ref()`, take
    the entry and `persist` without recording (as `aborted`);
  - doc comments on both.
- [ ] Must stay green: `a_rebase_continued_while_another_worker_reads_is_still_one_undo_entry`,
  `a_merge_restarted_outside_with_the_same_heads_does_not_reuse_the_old_pending_entry`, the
  `undo.rs` recorder tests.
- [ ] `cargo test -p repo-service` and clippy.
- [ ] Commit `undo: drop a pending entry that ended outside before the command (#175)`.

---

## Part B: case folding (#166, #162), git-core

### Task B1: fold to a fixed point (`ẞ`) (#166)

**Files:** `crates/git-core/src/worktree_guard.rs`.

- [ ] **Write the failing tests first:**
  - `fn sharp_s_capital_and_small_fold_together()`:
    `fold_name("ẞ") == fold_name("ß") == fold_name("ss")`; `first_collision([("ẞ", LINK),
    ("ß/x", FILE)])` is `Some`.
  - `fn folding_is_idempotent_for_every_scalar_value()`: for every `char` in
    `'\0'..=char::MAX`: `fold(fold(c)) == fold(c)` and `fold(upper(c)) == fold(c)`, collecting
    failures and asserting the list is empty (print the first 20).
- [ ] **Implement:** per character lower → upper → lower (keep the ignorable filter). Update the
  doc comment.
- [ ] `cargo test -p git-core worktree_guard` (existing `ſ/σ/µ/ı` and `Straße` tests stay green).
- [ ] Commit `case collisions: fold ẞ with ß and ss (#166)`.

### Task B2: NFD normalization (#166, D7-A)

**Files:** `crates/git-core/Cargo.toml`; `worktree_guard.rs`; `crates/git-core/tests/case_collision.rs`;
`docs/LICENSE_COMPLIANCE.md`.

- [ ] **Write the failing tests first:**
  - unit: `fold_name("\u{e9}".as_bytes()) == fold_name("e\u{301}".as_bytes())`;
    `first_collision([("\u{e9}", LINK), ("e\u{301}/x", FILE)])` and the reverse order are
    `Some`; the idempotence test from B1 now includes normalization.
  - `#[cfg(target_os = "macos")] fn a_link_and_a_folder_equal_after_normalization_are_refused()`
    in `tests/case_collision.rs`: a branch with link `\u{e9}` → outside and file
    `e\u{301}/x`; `branch::switch_branch` → `CaseCollision`; outside untouched (macOS CI only).
- [ ] `cargo info unicode-normalization` (record version and license); add
  `unicode-normalization = "0.1"` under `[dependencies]`; add the LICENSE_COMPLIANCE row
  (MIT OR Apache-2.0, `git-core`, "NFD before case folding in `worktree_guard::fold_name`
  (#166)", source and date).
- [ ] **Implement:** `use unicode_normalization::UnicodeNormalization;`
  `text.nfd().filter(..).flat_map(lower).flat_map(upper).flat_map(lower).nfd().collect()`.
  Doc comment: NFD as APFS/HFS+ compare; an over-approximation on NTFS.
- [ ] `cargo test -p git-core`, clippy, `python3 scripts/check-license-compliance.py`.
- [ ] Commit `case collisions: fold NFC/NFD-equivalent names (#166)`.

### Task B3: probe the worktree destination (#162, D6-A)

**Files:** `worktree_guard.rs`; `crates/git-core/src/worktree.rs`; `crates/git-core/tests/worktree.rs`.

- [ ] **Write the failing tests first** (`worktree_guard.rs` tests):
  - `#[cfg(target_os = "linux")] fn a_linux_temp_dir_does_not_fold_case()`:
    `dir_folds_case(tmp) == Ok(false)`;
  - `#[cfg(any(target_os = "macos", windows))] fn the_temp_dir_folds_case()`: `Ok(true)`;
  - `fn the_probe_leaves_nothing_behind()`: the folder is empty afterwards;
  - `fn the_probe_uses_the_nearest_existing_ancestor()`: `nearest_existing(tmp/a/b/c) == tmp`.
  - `tests/worktree.rs` `fn a_collision_is_refused_when_the_destination_folds_case()`: Linux,
    `core.ignorecase=false`, the collision fixture of l. 133, through
    `worktree::create_worktree_checking(&repo, .., dest_folds = true)` (a `#[doc(hidden)] pub`
    variant taking the probe result) → `CaseCollision`, nothing created. With `false` it
    succeeds (today's behaviour).
- [ ] **Implement:**
  - `pub(crate) fn dir_folds_case(dir: &Path) -> io::Result<bool>`: `tempfile::Builder::new()
    .prefix(".browsitory-case-probe-").tempfile_in(dir)?`, flip the ASCII case of its file name,
    `symlink_metadata(dir.join(flipped)).is_ok()`;
  - `fn nearest_existing(path) -> &Path`;
  - `create_worktree` computes `folds_case(repo) || dir_folds_case(nearest).unwrap_or(false)`
    and calls `find_case_collision` itself when true (or extend `refuse_case_collision` with an
    `applies: bool` variant).
- [ ] `cargo test -p git-core --test worktree --test case_insensitive_links` and the unit tests;
  clippy.
- [ ] Commit `worktree add: check case collisions when the destination folds case (#162)`.

---

## Part C: launch and mark-resolved (#165, D8-A)

### Task C1: open first, then check the handle

**Files:** `crates/git-core/src/launch.rs`; `crates/git-core/src/merge.rs`; tests.

- [ ] **Write the tests first** (regression guards; the swap race isn't deterministic, see the
  spec):
  - `launch.rs` `#[cfg(unix)] fn a_fifo_is_not_read_and_does_not_block()`: `mkfifo` at
    `x`; `file_may_run` on a thread, `recv_timeout(5 s)` returns;
  - `fn a_symlink_to_a_launcher_is_still_refused()`: link `l` → `launcher` (`[Desktop Entry]`),
    `file_may_run(l)` is true (guards the `O_NOFOLLOW` change);
  - `tests/merge.rs` (or the conflicts test file) `#[cfg(unix)] fn
    mark_resolved_does_not_block_on_a_fifo()`: a conflicted `a.txt` replaced by a FIFO,
    `mark_resolved(.., false)` returns within 5 s.
- [ ] **Implement:**
  - `read_head`: `OpenOptions::new().read(true)`, `#[cfg(unix)] custom_flags(libc::O_NONBLOCK |
    libc::O_NOFOLLOW)`, open, `file.metadata().ok()?.is_file()` on the handle, then
    `take(limit + 1)`;
  - `mark_resolved`: `let content = crate::worktree_guard::open_beneath(workdir,
    Path::new(path))?.map(read_to_end)`; check markers only on `Some`; keep `meta` for
    add-vs-remove.
- [ ] `cargo test -p git-core` and clippy.
- [ ] Commit `launch, merge: read regular files through the opened handle (#165)`.

### Task C2: key-file desktop sniffing, `# Config File`, `.kdelnk`

**Files:** `launch.rs`.

- [ ] **Write the failing tests first** (`launch.rs` tests):
  - `looks_runnable(b"[Settings]\nA=b\n\n[Desktop Entry]\nExec=x\n")` is true;
  - `looks_runnable(b"# Config File\nfoo\n")` and `b"# KDE Config File\n"` are true;
  - `may_execute(Path::new("x.kdelnk"), Os::Linux, false)` is true;
  - stay false: `b"[metadata]\nname = x\n"`, `b"[Settings]\nplain text\n[Desktop Entry]\n"`
    (not a key file), `b"# Notes\n[Desktop Entry] is a header\n"`, `comments + "plain text"`;
  - `file_may_run` on a 70 KiB `[a]\nk=v\n…` file with no desktop group is true (documents
    D8-A's limit).
- [ ] **Implement** `is_desktop_entry` per spec §7 (comment the GLib and shared-mime-info rules),
  and `"kdelnk"` after `"desktop"` in `RUNNABLE_EXTENSIONS`.
- [ ] `cargo test -p git-core launch` and clippy.
- [ ] Commit `launch: sniff desktop entries in any group, Config File magics, .kdelnk (#165)`.

---

## Part Z: docs, changelog, gate (PR 1)

### Task Z1: Docs and changelog

- [ ] `CHANGELOG.md` → `## [Unreleased]` → `### Lock and security follow-ups`, replacing the
  stub:
  - `Fixed: with the desktop app and the VSCode extension open on one repository, Undo, Redo, discards and Reset hard could briefly refuse with "Another Browsitory window is changing this repository's undo history" just after the other window finished, and other actions could run without an undo entry, when the other window was starting a hook or git at that moment (#175).`
  - `After the other window kept the undo history busy for the full 10 seconds once, later actions wait up to a second for it instead of giving up at once (#175).`
  - `Fixed: a merge, cherry-pick or revert committed or aborted outside Browsitory while the other window was busy could still be listed as an undo step (#175).`
  - `Saving a stash and adding a worktree wait for the other window's undo-history lock, so they can't slip past a rebase or cherry-pick sequence it is starting (#174).`
  - `Adding a worktree on a drive or folder that ignores case (a USB stick, a casefold folder on Linux) now refuses a symbolic link and a folder whose names differ only in case, even when the repository itself is on a case-sensitive drive (#162).`
  - `A symbolic link and a folder whose names are equal apart from how an accent is encoded (é as one or two code points), or apart from ẞ/ß, are now recognised as the same name, as on macOS (#166).`
  - `Open in editor (no editor configured) also refuses files starting with "# Config File" or "# KDE Config File", .kdelnk files, and settings files with a [Desktop Entry] group after other groups, as the desktop may run them; a file replaced by a named pipe at that moment no longer hangs it or Mark resolved (#165).`
  - In `### Rebase/undo safety`, delete the sentence `A paused merge or pick from the previous version is not undoable as one step after upgrading (#172).` (no release stored pending entries; see the spec).
- [ ] `docs/ARCHITECTURE.md`:
  - "Undo stack" l. 205–246: explicit unlock (fork copies; why not OFD locks); `CONTENDED_WAIT`
    replaces "try only once"; stash save and worktree add in the `locks_state` list; the
    stale-at-settle rule; drop "a pending entry from a build before this marker format is
    dropped once";
  - "Worktree writes and symlinks" l. 584–596: NFD plus the lower→upper→lower fold; worktree add
    probes the destination (`dir_folds_case`);
  - l. 714–716: the key-file sniff, the magics, `.kdelnk`.
- [ ] `docs/USER_GUIDE.md`:
  - l. 663–667: "…for more than about 10 seconds… After that, other actions wait about a second
    for it.";
  - l. 467–475: "Adding a worktree also refuses this when the worktree's drive or folder ignores
    case. Names that differ only in how an accent is encoded count as the same name.".
- [ ] Commit `Docs and changelog for lock and security follow-ups (#162, #165, #166, #174, #175)`.

### Task Z2: Full gate (CLAUDE.md order), then PR

- [ ] `cargo build --workspace`
- [ ] `cargo test --workspace` (run repo-service 3×: `cargo test -p repo-service` ×3, no flakes)
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo fmt --all -- --check`
- [ ] `cd frontend && pnpm install && pnpm build && pnpm lint && pnpm test -- --run && cd ..` (no
  frontend change; sanity only)
- [ ] Tauri E2E and VSCode E2E in the exact CLAUDE.md order (`xvfb-run -a` when headless). The
  worker's lock changes run under every command.
- [ ] `python3 scripts/check-changelog.py --base origin/main` and `python3
  scripts/check-license-compliance.py` pass.
- [ ] Manual smoke (`cargo tauri dev` from `crates/tauri-app` and the VSCode extension on one
  scratch repo under `/tmp`, never the real data dir):
  1. Commit in one window with a `pre-commit` that runs `sleep 3`. Undo in the other right
     after: it works, no busy message.
  2. Open in editor on an extensionless file starting with `# Config File`: refused.
- [ ] Push. Open the PR with:
  - D1–D8 and their answers;
  - "Closes #162, closes #165, closes #166, closes #174, closes #175";
  - "Refs #182 (the lock flakes; the E2E items remain)";
  - the obsolete #175 bullet and the CHANGELOG correction;
  - the spec's unrelated findings (a follow-up issue for `mark_resolved` beyond a link is
    suggested; the user files it).

---

## Part W: Windows hardening (PR 2, `feat/windows-hardening`)

All tests in this part are `#[cfg(windows)]` and run only on `windows-latest` CI. Per D12-A:
commit the tests of W1–W4 first (`tests: Windows walker and job tests, red`), push, and record
the red CI run in "Pre-flight notes" before the implementation commits. Type-check each FFI file
through `target/winprobe` before pushing.

### Task W1: POSIX-semantics delete (#168)

**Files:** `crates/git-core/src/worktree_guard/beneath_windows.rs`; `CHANGELOG.md` (stub
`### Windows hardening`).

- [ ] **Failing tests** (`beneath_windows.rs` tests):
  - `fn replace_succeeds_while_another_handle_keeps_the_old_file_open()`: write `d/f`; open it
    with `share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)` and keep the
    handle; `replace(dir, "d/f", Content::File { bytes: b"new", executable: false },
    &mut no_step)` is `Ok`; `d/f` reads `"new"`. Access denied today.
  - `fn remove_and_prune_removes_a_folder_whose_file_another_handle_keeps_open()`: the same
    handle on `d/f`; `remove_and_prune(dir, "d/f")`; `d` no longer exists. It stays today.
- [ ] **Implement:**
  - `FILE_DISPOSITION_INFO_EX_CLASS: i32 = 21`, `#[repr(C)] struct DispositionInfoEx { flags: u32 }`,
    `FILE_DISPOSITION_FLAG_DELETE = 0x1`, `FILE_DISPOSITION_FLAG_POSIX_SEMANTICS = 0x2`;
  - `delete_by_handle` tries Ex first and falls back to class 4 on 87/50/1;
  - layout test: `size_of::<DispositionInfoEx>() == 4`;
  - module doc: POSIX semantics, the fallback.
- [ ] Commit `windows walker: delete with POSIX semantics (#168)`.

### Task W2: in-place junction tests (#164)

**Files:** `beneath_windows.rs` (tests).

- [ ] Test helper `junction_in_place(dir: &Path, target: &Path)`: `CreateFileW`-equivalent
  `OpenOptions` (access `FILE_WRITE_DATA | FILE_WRITE_ATTRIBUTES`, share `READ | WRITE`,
  `FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT`); build the mount-point buffer
  (tag `0xA000_0003`, header 8 bytes + 8 bytes of offsets/lengths, then `\??\<target>\0<target>\0`
  in UTF-16, the target taken from `canonicalize` with `\\?\` replaced by `\??\` for the
  substitute name); `DeviceIoControl(h, 0x000900A4, buf, len, null, 0, &mut n, null)` declared
  in the test module. Return `io::Result<()>`.
- [ ] Tests (each converts `d` from the `Step::Opened("d")` callback):
  - `fn a_held_folder_turned_into_a_junction_is_not_written_through()`: `d` empty, `write(dir,
    "d/f", b"x", step)` is `Err`, and `outside` has no `f` (regression: passes today);
  - `fn a_held_folder_turned_into_a_junction_is_not_read_through()`: `outside/f` = `OUTSIDE`;
    `read(dir, "d/f", step)` is not `Some(File { bytes: OUTSIDE })`;
  - `fn no_folder_is_created_in_a_junction_target_during_the_walk()`: `write(dir, "d/e/f", ..)`
    with `OnMissing::Create`; afterwards `outside` is empty. **Red today** (`outside\e` exists)
    until W3.
  - If `junction_in_place` fails with a sharing violation while `d` is held, change all three to
    assert that refusal, record it in "Pre-flight notes", and in W5 reword the ARCHITECTURE
    residual as "not reachable: a held folder can't be converted".
- [ ] Commit `windows walker: test junctions made in place during a walk (#164)` (with W3 if
  the red test would otherwise break the build; tests may be committed red only on this
  branch per D12-A).

### Task W3: create folders relative to the held parent (#163, D10-A)

**Files:** `beneath_windows.rs`.

- [ ] **Implement** `create_dir_in(parent: &File, name: &OsStr) -> io::Result<()>` per spec §10:
  - `#[repr(C)]` `UnicodeString { length: u16, maximum_length: u16, buffer: *mut u16 }` (byte
    lengths, no NUL);
  - `ObjectAttributes { length: u32, root_directory: *mut c_void, object_name: *mut
    UnicodeString, attributes: u32, security_descriptor: *mut c_void,
    security_quality_of_service: *mut c_void }`;
  - `IoStatusBlock { status: usize, information: usize }`;
  - the `ntdll` block; `NTSTATUS < 0` → `io::Error::from_raw_os_error(RtlNtStatusToDosError(s)
    as i32)`; success → `File::from_raw_handle(h)` dropped;
  - names longer than `u16::MAX / 2` → `InvalidInput`;
  - layout tests: `size_of::<ObjectAttributes>() == 48`, `size_of::<UnicodeString>() == 16` on
    x86_64.
- [ ] `walk` l. 343: `match create_dir_in(&at.dirs[at.dirs.len() - 1], &name)` in place of
  `std::fs::create_dir(&path)`; the `ERROR_ALREADY_EXISTS` arm and `continue` stay. Module doc:
  drop the "Residual" paragraph.
- [ ] W2's create test turns green; all walker tests and `tests/symlink_write_through.rs` stay
  green on `windows-latest`.
- [ ] Commit `windows walker: create missing folders relative to the held parent (#163)`.

### Task W4: start hooks suspended (#180, D9-A)

**Files:** `crates/git-core/src/process/job_windows.rs`; `crates/git-core/src/process.rs`.

- [ ] **Failing tests** (`job_windows.rs` `#[cfg(test)]`):
  - `fn a_suspended_program_runs_only_after_resume()`: `Command::new("cmd").args(["/C", "echo",
    "ran"])`, stdout piped, `creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED)`, spawn;
    sleep 300 ms; `try_wait()` is `None`; `resume(&child)` is `Ok`; `wait_with_output` succeeds
    with `"ran"`;
  - `fn a_hook_is_in_its_job_before_it_runs()` (`process.rs` tests): `let (child, job) =
    spawn_in_job(&mut command)?` (`command` = `cmd /C exit 0`, suspended inside), assert
    `IsProcessInJob(child, job) == TRUE`, then resume and wait (test-only kernel32
    declaration of `IsProcessInJob`);
  - existing: `timeout_kills_a_hooks_background_grandchild_on_windows`,
    `cancel_kills_…`, `a_normal_exit_leaves_…` stay green.
- [ ] **Implement:**
  - `job_windows.rs`: `pub(super) const CREATE_SUSPENDED: u32 = 0x4;` and `pub(super) fn
    resume(child: &Child) -> io::Result<()>` (`CreateToolhelp32Snapshot(0x4, 0)`, invalid handle
    `-1isize as *mut c_void`, `THREADENTRY32 { dw_size: 28, .. }`, `Thread32First/Next`,
    `OpenThread(0x0002, 0, tid)`, `ResumeThread` (`u32::MAX` = error), `CloseHandle`; zero threads
    resumed → `Err`). Layout test `size_of::<ThreadEntry32>() == 28`. Module doc: the race is
    closed; delete "Residual race".
  - `process.rs`: `#[cfg(windows)] fn spawn_in_job(command) -> Result<(Child, Option<Job>),
    RunError>`: `creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED)`, `spawn_retrying`,
    `Job::assign`, `resume` (on error: `kill_and_reap`, `RunError::Spawn`). `run_captured` uses
    it on Windows. `run_captured_split` and `spawn` are unchanged.
- [ ] Commit `hooks: start a Windows hook suspended until it is in its job (#180)`.

### Task W5: docs, changelog, gate (PR 2)

- [ ] `CHANGELOG.md` → `### Windows hardening`:
  - `Fixed (Windows): a hook that starts a program right away could leave it running after Cancel or the time limit; hooks now start only once they are in their job (#180).`
  - `Fixed (Windows): undo, redo and discards no longer fail when an antivirus scanner or the search indexer has the file open (#168).`
  - `Windows: a missing folder is created through the folder Browsitory already holds, so a folder swapped for a junction at that moment can't get an empty folder created outside the repository (#163). Tests cover folders turned into junctions during a write (#164).`
- [ ] `docs/ARCHITECTURE.md`: "Git hooks" l. 455–465 (suspended start, no residual race);
  "Worktree writes and symlinks" l. 545–556 (`FileDispositionInfoEx` POSIX with fallback,
  `NtCreateFile` relative creation, the residual sentence removed or "not reachable" per W2,
  ntdll alongside kernel32).
- [ ] Gate: `cargo build/test/clippy/fmt --workspace` on Linux; CI `windows-latest` green
  (the `rust` job runs `cargo test --workspace` and clippy there); `python3
  scripts/check-changelog.py --base origin/main`.
- [ ] Push. Open the PR with D9, D10, D12 and their answers; the red-then-green CI runs (or
  "written test-first, red not shown" per D12-C); "Closes #163, closes #164, closes #168,
  closes #180".

---

## Decision hooks

| Decision | If the user picks… | Change |
|---|---|---|
| D1-B | OFD locks | Don't: they are inherited across fork. If insisted on, A1's tests stay red; report back. |
| D1-C | retries only | Drop A1's implementation; keep the #176 retries; A1's tests become `#[ignore]` with a reason. |
| D2-B | wait only for refusing commands | A2: `let wait = if contended && !refuses(&command) { ZERO } else if contended { CONTENDED_WAIT } else { LOCK_WAIT }` with `undo::refuses` extracted from `reply_busy`'s match. The A2 test uses `undo` instead of `stage_file`. |
| D2-C | keep try-once | Drop A2's implementation and test; only rewrite `only_the_first_…` to `< LOCK_WAIT` with test `LOCK_WAIT` = 1 s. |
| D3-B | record with a label | A4: in place of the drop, `record(.., format!("{} (finished outside Browsitory)", label), ..)`; the test expects that label. |
| D3-C | leave it | Drop A4; close the bullet as "mislabel only, Undo verifies". |
| D4-B | fetch outside the lock | New task A5: split `PullCurrentUpstream` in `worker/remote.rs` into fetch (no lock) + integrate (`locks_state` via a new `Command::IntegrateUpstream` queued by the worker itself). Worker test: the other window's locked command doesn't wait during the fetch phase (a slow local `file://` remote). Recommend a separate PR. |
| D5-B | process-wide registry | New task A5: `static HELD: Mutex<HashSet<PathBuf>>` + `Condvar` in `state_lock.rs`, keyed by `canonicalize(lock file)`, taken before `try_lock` within the same `wait`, released in `Drop`. Unit test: two `StateLock`s of one process exclude each other through the registry alone (a registry-only constructor). |
| D6-B | read-only probe | B3: `dir_folds_case` reads the first entry with an ASCII letter, compares `symlink_metadata` of the flipped name (and on unix the inode) and falls back to the temp file when there is none. |
| D6-C | always check | B3: `create_worktree` uses `CasePolicy::Always` semantics; drop the probe; the Linux test expects refusal with `core.ignorecase=false`. USER_GUIDE: "On Linux, adding a worktree also refuses it." |
| D7-B | hand table | B2: no dependency; a `const DECOMPOSE: &[(char, &str)]` for U+00C0–U+017F plus U+1E00–U+1EFF generated once (keep the generator under `target/`, delete it), applied before folding; the doc names the residual. |
| D7-C | icu_normalizer | B2: `icu_normalizer` with `DecomposingNormalizerBorrowed::new_nfd()`; add a documented license exception to `docs/LICENSE_COMPLIANCE.md` (like libgit2) and to CLAUDE.md's policy line. |
| D8-B | magics only | C2: keep the first-group rule; add only the two magics and `kdelnk`; drop the later-group tests. |
| D9-B | NtResumeProcess | W4: `#[link(name = "ntdll")] fn NtResumeProcess(process: *mut c_void) -> i32`; `resume` = one call on `child.as_raw_handle()`; drop Toolhelp. |
| D9-C | own CreateProcessW | Recommend a separate PR: a `process/spawn_windows.rs` with `STARTUPINFOEXW`, `InitializeProcThreadAttributeList`, `PROC_THREAD_ATTRIBUTE_JOB_LIST`, std-compatible argument quoting and pipe handles. |
| D10-B | handle-relative walker | Recommend a separate PR: every component through `NtCreateFile(FILE_OPEN)` relative to the held parent; `in_parent` checks removed; all walker tests re-run. |
| D11-B | one PR | Parts A–C, W and Z on `feat/lock-security-bundle`; one CHANGELOG section with both bullet sets; Z2 also waits for windows-latest. |
| D12-B | install mingw | Add to Global Constraints: `cargo clippy --target x86_64-pc-windows-gnu -p git-core --all-targets -- -D warnings` before every Part W commit; winprobe not needed. |
| D12-C | CI only | Skip the red test-first push; the PR says the tests were written first but only CI ran them. |

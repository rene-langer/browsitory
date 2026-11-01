# Security Follow-ups Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close five follow-ups in two PRs:

- PR 1 (`feat/security-followups`):
  - **#199:** Mark resolved stages exactly the bytes it read and checked (no second read by
    path);
  - **#201:** the desktop-launcher sniff runs on Linux only and not for document extensions;
  - **#197:** while contended, locked commands wait once per burst, not once each.
- PR 2 (`feat/windows-walker-handles`):
  - **#194:** POSIX delete ignores the read-only attribute; the classic fallback clears it;
  - **#193:** every walker component and leaf is opened relative to the held parent.

**Architecture:**

- `git-core`: `merge.rs` (`mark_resolved`, seam `mark_resolved_with_pause`), `launch.rs`
  (`DOCUMENT_EXTENSIONS`, `names_a_document`, `file_may_run`),
  `worktree_guard/beneath_windows.rs` (`nt_open`, `link_target`, `delete_by_handle`,
  `delete_classic`).
- `repo-service::worker`: `state_lock.rs` (`Contention`), `mod.rs` (the acquire loop, tests).

**Tech Stack:** Rust (`git2` 0.21 / libgit2 1.9: `blob_writer`, `Index::add`/`remove_path`;
`libc` on unix; hand-written ntdll/kernel32 FFI on Windows).

**Spec:** `docs/superpowers/specs/2026-11-01-security-followups-design.md`. This plan implements
the **provisional** choice of each decision D1–D8. "Decision hooks" at the end lists what
changes for the other options.

---

## Global Constraints

- **Branches:** PR 1 on `feat/security-followups` (Parts A–C, Z), PR 2 on
  `feat/windows-walker-handles` (Part W), both from `main`. Each PR is opened only after its
  gate task (Z2 / W4) passes.
- **Parallel batch:** the robustness batch edits the worker's hooks/status code, `pick.rs`,
  `stash.rs`, `stage.rs`, Tauri commands and E2E. Don't touch those files. In `worker/mod.rs`
  only the acquire loop (l. 815–860) and `state_lock_tests` change. If `main` moves, rebase and
  re-check those lines.
- **Dependencies:** none. Windows FFI is `#[link(name = "ntdll")]` / `#[link(name =
  "kernel32")] extern "system"`, as in `beneath_windows.rs`. **Not** `windows-sys`.
- **Tests use real temp-dir repos** (`tempfile` + `git2::Repository::init`), never a mocked
  `Repository`:
  - git-core integration tests start with `mod common;` (`init_repo`, `write_file`,
    `commit_all`);
  - worker tests live in `state_lock_tests` (`init_repo`, `write_file`, `commit_all`, `hold`).
- **Platform gating:**
  - FIFO and symlink tests `#[cfg(unix)]`;
  - the globs2 cross-check `#[cfg(target_os = "linux")]`;
  - Part W `#[cfg(windows)]`;
  - junction tests for `mark_resolved` `#[cfg(windows)]` (in PR 1; CI runs them).
- **Windows code can't be compiled here** (no mingw). Type-check through a throwaway
  `target/winprobe` crate (`#[path]`-include of `beneath_windows.rs` plus minimal stubs for its
  `super::` imports), `cargo clippy --target x86_64-pc-windows-gnu`, then delete it. CI is the
  authority (D8-A).
- **Never** touch `~/.local/share/com.browsitory.app`, the real config dir or `~/.gnupg`.
  Probes go under `target/` and are deleted afterwards.
- **No new user-visible string.**
- **CHANGELOG:** every push touching `crates/` needs a `CHANGELOG.md` change (pre-push hook).
  - PR 1: Task A1 adds a stub `### Security follow-ups` at the top of `## [Unreleased]` (above
    `### Lock and security follow-ups`); Task Z1 writes the final text.
  - PR 2: Task W1 adds `### Windows walker` at the top; Task W4 writes the final text.
- **Commits:** one per task unless noted, message `<area>: <what> (#NNN)`, each ending with the
  session's `Co-Authored-By`/`Claude-Session` trailers.

## Pre-flight

- [ ] `git status` is clean on the branch, rebased on `origin/main` if `main` moved. If it did,
  re-check the line references below.
- [ ] `cargo test -p git-core -p repo-service` passes before any change. Record the counts in
  "Pre-flight notes".
- [ ] Link the pre-push hook (CLAUDE.md "Git hooks") if this worktree doesn't have it.

Reconciled with the code at 25040598 on 2026-11-01:

- `crates/git-core/src/merge.rs`:
  - `NotAFile` l. 30–34, `ensure_inside_workdir` l. 63, `find_conflict` l. 294;
  - `mark_resolved` l. 470 (type check l. 482–489, marker read l. 490–501, `add_path`
    l. 502–508);
  - `has_conflict_markers` l. 515.
- `crates/git-core/src/undo.rs`: `MODE_*` l. 17–19, `WorktreeConfig` l. 110, `worktree_config`
  l. 115, `read_worktree_file` l. 165, `index_entry` l. 192.
- `crates/git-core/src/stash.rs`: `worktree_blob` l. 440, `undo_error` l. ~460 (read only, not
  changed).
- `crates/git-core/tests/merge.rs`: `make_conflicted_repo` l. 108, FIFO test l. 278,
  symlinked-folder test l. 1179.
- `crates/git-core/src/launch.rs`:
  - `Os` l. 19, `RUNNABLE_EXTENSIONS` l. 172, `lower_extension` l. 290, `may_execute` l. 303;
  - `looks_runnable` l. 311, `DESKTOP_SNIFF_LIMIT` l. 332, `is_desktop_entry` l. 348;
  - `file_may_run` l. 540, `read_head` l. 578, `prepare_editor` l. 603;
  - sniff tests l. 893–1125 (`a_document_quoting_a_launcher_opens` l. 999).
- `crates/repo-service/src/worker/state_lock.rs`: `LOCK_WAIT` l. 25–28, `CONTENDED_WAIT`
  l. 34–37, `acquire` l. ~80.
- `crates/repo-service/src/worker/mod.rs`: acquire loop l. 815–860; `state_lock_tests`
  l. 6628 (`hold` l. 6670, `a_locked_command_runs_after_…` l. 6970, `only_the_first_…` l. 6991,
  `after_a_timeout_…` l. 7018, `work_tree_destroying_…` l. 7043).
- `crates/git-core/src/worktree_guard/beneath_windows.rs`:
  - module doc l. 1–29, constants l. 41–75;
  - `delete_by_handle` l. 166, `set_information` l. 188, NT structures l. 207–250,
    `create_dir_in` l. 263;
  - `in_parent` l. 325, `final_path` l. 335, `open_pinned` l. 358;
  - `open_leaf_at` l. 419, `reopen_same` l. 450, `walk` l. 473 (component open l. 499);
  - `open_leaf_for_reading` l. 576, `create_leaf` l. 605, `write` l. 632, `remove` l. 675;
  - `read` l. 700 (`read_link` ~l. 716), `remove_and_prune` ~l. 736, `replace` ~l. 762;
  - tests ~l. 793 (`junction_in_place`, `no_folder_is_created_…` with the 1921 pin).
- Docs: ARCHITECTURE l. 222–245 (lock), l. 557–580 (Windows walker), l. 746–760 (launch, Mark
  resolved); USER_GUIDE l. 121, l. 666–671, l. 698–705.

## Pre-flight notes

(fill in: baseline counts, whether `main` moved, decisions confirmed by the user, deviations,
the red CI run of Part W)

---

## Part A: Mark resolved stages what it read (#199, D1-A)

### Task A1: the seam and the red tests

**Files:** `crates/git-core/src/merge.rs`; `crates/git-core/tests/merge.rs`; `CHANGELOG.md`
(stub).

- [ ] Add `#[doc(hidden)] pub fn mark_resolved_with_pause(repo, path, force, after_check: &mut
  dyn FnMut()) -> Result<MarkResolvedOutcome, MergeError>`. For now it is today's body with
  `after_check()` right after the marker check (before `repo.index()`). `pub fn mark_resolved`
  calls it with `&mut || {}`.
- [ ] **Write the failing tests first** (`tests/merge.rs`):
  - `fn mark_resolved_stages_the_content_it_checked()`:
    - `make_conflicted_repo`;
    - write `shared.txt` = `"resolved\n"`;
    - `mark_resolved_with_pause(.., false, &mut || write(shared.txt, "<<<<<<< x\nours\n"))`
      → `Resolved`;
    - the stage-0 blob reads `"resolved\n"`. **Red today** (markers staged).
  - `#[cfg(unix)] fn mark_resolved_does_not_block_on_a_fifo_swapped_in_after_the_check()`:
    - the swap removes the file and `mkfifo`s it;
    - run on a thread with `recv_timeout(5 s)` (unblock with a writer open on timeout, as in the
      l. 278 test);
    - expect `Ok(Resolved)`, stage 0 = the read content. **Red today** (hangs).
  - `#[cfg(unix)] fn mark_resolved_does_not_stage_a_link_swapped_in_after_the_check()`:
    - the swap replaces the file with a symlink to `outside/secret`;
    - the stage-0 mode is `0o100644` and the blob is the read content. **Red today** (mode
      `0o120000`).
  - Run `cargo test -p git-core --test merge mark_resolved` and confirm the three fail on Linux.
- [ ] `CHANGELOG.md`: `### Security follow-ups` at the top of `## [Unreleased]`, stub bullet.
- [ ] Commit `merge: test that Mark resolved stages what it checked, red (#199)`.

### Task A2: stage through the handle

**Files:** `merge.rs`; `tests/merge.rs`.

- [ ] **Implement** per spec §1:
  - `previous_mode`: `index.get_path(p, 0).or(index.get_path(p, 2)).map(|e| e.mode)`;
  - `crate::undo::read_worktree_file(workdir, path, &worktree_config(repo)?, previous_mode)?`;
    map `UndoError` like `stash::undo_error`;
  - markers checked on those bytes (not for `MODE_LINK`);
  - `after_check()`;
  - `fn staged_blob(repo, path, bytes, mode) -> Result<Oid, MergeError>`: `repo.blob` for a
    link, else `blob_writer(Some(Path::new(path)))` + `write_all` + `commit`;
  - `index.remove_path(path)?; index.add(&crate::undo::index_entry(path, id, mode))?;
    index.write()?`;
  - `None`:
    - `symlink_metadata` `NotFound` → `remove_path` + `write`;
    - `is_dir()` → today's `add_path`;
    - else `NotAFile`.
  - Doc comment: why no `add_path` (libgit2 re-reads by path); the same filters through the
    hint path; REUC via `remove_path`; zeroed stat; the folder residual.
- [ ] **Guards** (`tests/merge.rs`, green before and after):
  - `mark_resolved_applies_the_clean_filter()`: `core.autocrlf=true`, `"a\r\nb\r\n"` → blob
    `"a\nb\n"`;
  - `#[cfg(unix)] mark_resolved_keeps_the_exec_bit()`: `core.filemode=true`, `chmod 755` →
    `0o100755`;
  - `mark_resolved_keeps_ours_mode_without_filemode()`: `core.filemode=false`, ours `0o100755`
    in the fixture → `0o100755`;
  - `mark_resolved_records_resolve_undo()`: `std::fs::read(repo.path().join("index"))` contains
    `b"REUC"`;
  - `#[cfg(unix)] mark_resolved_stages_a_link_as_a_link()`: a conflicted path replaced by a
    link before the call → `0o120000`, blob = target;
  - `#[cfg(windows)] mark_resolved_refuses_a_path_below_a_junction()`: `sub/shared.txt`
    conflict, `sub` → junction (`mklink /J`) → `SymlinkedFolder`; index still conflicted.
- [ ] Existing `mark_resolved_*` tests, `conflict_resolution_never_writes_through_…` and
  `tests/symlink_write_through.rs` stay green.
- [ ] `cargo test -p git-core` and `cargo clippy -p git-core --all-targets -- -D warnings`.
- [ ] Commit `merge: stage exactly the bytes Mark resolved checked (#199)`.

---

## Part B: glob before magic, Linux only (#201, D3-A)

### Task B1: document extensions and the Linux gate

**Files:** `crates/git-core/src/launch.rs`.

- [ ] **Write the failing tests first** (`launch.rs` tests):
  - `fn a_document_quoting_a_launcher_near_its_start_opens_on_linux()`: `notes.txt` = `"Some
    notes\n[Desktop Entry]\nExec=x\n"`; `README.md` = `"[Desktop Entry]\nExec=x\n"` →
    `!file_may_run(.., Os::Linux)`. **Red today.**
  - `fn linux_launchers_are_not_sniffed_on_macos_or_windows()`: `LAUNCH` = `"[Desktop
    Entry]\nExec=x\n"` → `file_may_run` true on Linux, false on `MacOs`/`Windows`. **Red
    today.**
  - Guards:
    - `x.foo` and `notes.txt.` with that content → true on Linux;
    - `x.desktop`, `x.kdelnk` (any content) → true on every OS;
    - `#!/bin/sh` in `notes.txt` → true on every OS.
  - `#[cfg(unix)] fn a_links_name_and_targets_name_both_count()`, all on `Os::Linux`:
    - `notes.txt` → `LAUNCH` (desktop content): true;
    - `LAUNCH` → `notes.txt` (desktop content): true;
    - `a.md` → `b.txt` (desktop content): false.
- [ ] **Implement:**
  - `const DOCUMENT_EXTENSIONS` (spec §2), with a comment: shared-mime-info glob types, glob
    before magic (GLib never sniffs named files, Qt only without a glob match);
  - `fn names_a_document(path: &Path) -> bool` on the raw extension;
  - `file_may_run`: `check(path, sniff_desktop)`; the direct check passes
    `os == Os::Linux && !names_a_document(file)`, the target check
    `os == Os::Linux && !(names_a_document(file) && names_a_document(&target))`.
  - Update the `file_may_run` and `is_desktop_entry` doc comments.
- [ ] Existing sniff tests stay green (they use `Os::Linux` and extensionless names);
  `a_document_quoting_a_launcher_opens` stays green.
- [ ] `cargo test -p git-core launch` and clippy.
- [ ] Commit `launch: sniff desktop launchers only on Linux and not for documents (#201)`.

### Task B2: keep the list honest

**Files:** `launch.rs`.

- [ ] `#[cfg(target_os = "linux")] fn document_extensions_have_a_shared_mime_info_glob()`:
  - read `/usr/share/mime/globs2`;
  - if absent: `eprintln!("SKIP …")` and return, unless `std::env::var_os("CI")` is set
    (then panic);
  - for each entry, some line `weight:type:*.ext[:flags]` (case-insensitive match) exists and
    no matched type is `application/x-desktop`;
  - collect failures, assert empty.
- [ ] Run it locally (this machine has `globs2`).
- [ ] Commit `launch: check the document extensions against shared-mime-info (#201)`.

---

## Part C: wait once per burst (#197, D4-A)

### Task C1: `Contention`

**Files:** `crates/repo-service/src/worker/state_lock.rs`; `crates/repo-service/src/worker/mod.rs`.

- [ ] **Write the failing tests first:**
  - `state_lock.rs` unit tests (synthetic `let t0 = Instant::now()`):
    - `Contention::default().wait(t0) == LOCK_WAIT`;
    - after `busy(t0)`: `wait(t0) == CONTENDED_WAIT`;
    - after a second `busy(t1)`: `wait(t1 + 10 ms) == ZERO` and
      `wait(t1 + CONTENDED_WAIT) == CONTENDED_WAIT`;
    - a third `busy(t2)` extends the window;
    - `held()` → `LOCK_WAIT`.
  - `state_lock_tests`:
    - `fn a_burst_of_locked_commands_waits_once_while_the_lock_stays_held()`:
      - files `a`…`h`, worker, `let _guard = hold(&repo)`;
      - `stage_file(a)` (`LOCK_WAIT`), `stage_file(b)` (`CONTENDED_WAIT`);
      - `start`; `stage_file(c..=h)` sequentially;
      - `elapsed < 3 * CONTENDED_WAIT` (today ≥ `6 * CONTENDED_WAIT`, **red**);
      - all staged, `no_undo()`.
    - `fn after_a_quiet_moment_a_locked_command_waits_again()`:
      - as above through `b`;
      - `sleep(CONTENDED_WAIT + 100 ms)`;
      - `stage_file(c)` takes `>= CONTENDED_WAIT`.
- [ ] **Implement:**
  - `pub(super) struct Contention` per spec §3, with docs (first Busy only marks contention;
    later Busy opens or extends a `CONTENDED_WAIT` window of try-once; `held` resets);
  - `mod.rs` l. 815–860:
    - `let mut contention = state_lock::Contention::default();`
    - `let wait = contention.wait(Instant::now());`
    - `Held` → `contention.held()`;
    - `Busy` → `contention.busy(Instant::now())`;
    - update the comment.
- [ ] Must stay green: `only_the_first_…`, `after_a_timeout_…`, `a_locked_command_runs_after_…`,
  `work_tree_destroying_…`, `undo_replies_busy_…`.
- [ ] `cargo test -p repo-service` (3× for flakes) and clippy.
- [ ] Commit `state lock: wait once per burst while another window holds it (#197)`.

---

## Part Z: docs, changelog, gate (PR 1)

### Task Z1: Docs and changelog

- [ ] `CHANGELOG.md` → `## [Unreleased]` → `### Security follow-ups`, replacing the stub:
  - `Fixed: Mark resolved stages exactly the content it checked for conflict markers, read without following a symbolic link or waiting on a named pipe; a file swapped at that moment can no longer hang it or get other content staged (#199).`
  - `Fixed: Open in editor (no editor configured) no longer refuses a document such as notes.txt or README.md that quotes a "[Desktop Entry]" launcher near its start. On Linux, files whose extension marks them as a document are not checked for launcher contents, as the Linux desktops do; macOS and Windows no longer check for Linux launchers at all (#201).`
  - `While the other window keeps the undo history busy, a quick run of actions (for example staging files one by one) waits about a second once, not a second per action (#197).`
  - Wrap at about 100 columns like the section below it.
- [ ] `docs/ARCHITECTURE.md`:
  - l. 232–236: the per-burst rule (`Contention`);
  - l. 746–760: the Linux-only, glob-before-magic sniff (`DOCUMENT_EXTENSIONS`); Mark resolved
    stages the bytes it read (`read_worktree_file` + `blob_writer` hint path +
    `remove_path`/`add`, REUC kept, zeroed stat, the folder residual).
- [ ] `docs/USER_GUIDE.md`:
  - l. 670–671: "…each action waits about a second for it instead of 10; a quick run of actions
    waits only once.";
  - l. 702–705: "…and files whose contents start like a script, program or installer, or, on
    Linux, like a desktop launcher (unless the name ends in a document extension such as .txt
    or .md)."
- [ ] Commit `Docs and changelog for security follow-ups (#197, #199, #201)`.

### Task Z2: Full gate (CLAUDE.md order), then PR

- [ ] `cargo build --workspace`
- [ ] `cargo test --workspace` (`cargo test -p repo-service` ×3, no flakes)
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo fmt --all -- --check`
- [ ] `cd frontend && pnpm install && pnpm build && pnpm lint && pnpm test -- --run && cd ..`
  (no frontend change; sanity only)
- [ ] Tauri E2E and VSCode E2E in the exact CLAUDE.md order (`xvfb-run -a` when headless).
- [ ] `python3 scripts/check-changelog.py --base origin/main` and `python3
  scripts/check-license-compliance.py` pass.
- [ ] Manual smoke (`cargo tauri dev` from `crates/tauri-app` on a scratch repo under `/tmp`,
  never the real data dir):
  1. A conflicted file, edit it, Mark resolved: staged, conflict gone; `git status` is clean
     for it; `git ls-files --resolve-undo` lists it.
  2. With no editor set, Open on `notes.txt` = `Some notes\n[Desktop Entry]\n`: opens.
     Extensionless `LAUNCH` with the same content: refused.
- [ ] Push. Open the PR with:
  - D1, D3, D4, D7 and their answers;
  - "Closes #197, closes #199, closes #201";
  - #199 part 2 was already fixed (e43c236a; test at `tests/merge.rs:1179`), plus the Windows
    junction test added;
  - the spec's unrelated findings.

---

## Part W: Windows walker (PR 2, `feat/windows-walker-handles`)

All tests in this part are `#[cfg(windows)]` and run only on `windows-latest` CI. Per D8-A:
commit W1's red tests first (using only today's functions, so the crate compiles), push, and
record the red CI run in "Pre-flight notes" before the implementation commits. Type-check each
FFI change through `target/winprobe` before pushing.

### Task W1: red tests (#194, #193)

**Files:** `beneath_windows.rs` (tests); `CHANGELOG.md` (stub `### Windows walker`).

- [ ] #194 (`beneath_windows.rs` tests):
  - helper `make_read_only(path)` (`set_permissions` with `readonly(true)`);
  - `fn remove_deletes_a_read_only_file()`: `d/f` read-only; `remove(dir, "d/f")` is
    `Ok(true)`, `d/f` gone (access denied today);
  - `fn replace_replaces_a_read_only_file()`: `replace(.., Content::File { b"new" })` → `d/f`
    reads `new`;
  - `fn remove_and_prune_removes_a_read_only_file_and_its_folder()`: `d` gone.
- [ ] #193:
  - test-only kernel32 declarations: `FindFirstChangeNotificationW(path, watch_subtree: i32,
    filter: u32) -> *mut c_void`, `WaitForSingleObject(h, ms: u32) -> u32`,
    `FindCloseChangeNotification(h) -> i32`;
  - `fn a_converted_folder_never_has_its_target_opened_for_reading()`:
    - `outside/f` = `OUTSIDE`, held open by the test with `share_mode(0)`; `d` empty;
    - `read(dir, "d/f", junction_d_when_opened(..))`;
    - the conversion succeeded; the error's `raw_os_error()` is not `Some(32)`
      (ERROR_SHARING_VIOLATION: the target was opened). **Red today.**
  - `fn a_converted_folder_never_gets_a_name_in_its_target_even_briefly()`:
    - `FindFirstChangeNotificationW(outside, 0, FILE_NOTIFY_CHANGE_FILE_NAME = 0x1)`;
    - `write(dir, "d/f", b"x", junction_d_when_opened(..))` is `Err`;
    - `WaitForSingleObject(h, 0) != WAIT_OBJECT_0 (0)`;
    - `outside` is empty. **Red today** (`create_leaf` creates `outside\f`, then deletes it).
- [ ] `CHANGELOG.md`: `### Windows walker` at the top of `## [Unreleased]`, stub bullet.
- [ ] winprobe type-check, push, record the red run.
- [ ] Commit `tests: Windows walker read-only and converted-folder tests, red (#193, #194)`.

### Task W2: ignore the read-only attribute (#194, D6-A)

**Files:** `beneath_windows.rs`.

- [ ] **Implement:**
  - constants:
    - `FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE: u32 = 0x10`;
    - `FILE_BASIC_INFO_CLASS: i32 = 0`;
    - `FILE_ATTRIBUTE_READONLY: u32 = 0x1`, `FILE_ATTRIBUTE_NORMAL` (exists);
    - `FILE_WRITE_ATTRIBUTES: u32 = 0x100`;
    - `ERROR_ACCESS_DENIED: i32 = 5`;
  - `#[repr(C)] struct BasicInfo { creation_time: i64, last_access_time: i64, last_write_time:
    i64, change_time: i64, file_attributes: u32 }` (40 bytes with padding);
  - `delete_by_handle`: the three tiers of spec §5;
  - `fn delete_classic(file)`: class 4; on access denied with the read-only bit, clear it
    (`set_information(.., FILE_BASIC_INFO_CLASS, ..)`, times 0), retry, restore on failure;
  - `open_leaf_at` adds `FILE_WRITE_ATTRIBUTES`.
- [ ] Unit tests:
  - `fn delete_classic_deletes_a_read_only_file()`: open a read-only file with
    `DELETE | FILE_READ_ATTRIBUTES | FILE_WRITE_ATTRIBUTES`; `delete_classic` is `Ok`; gone
    after drop;
  - layout `size_of::<BasicInfo>() == 40`.
- [ ] Real-repo tests:
  - `tests/undo.rs` `#[cfg(windows)] fn undo_restores_a_read_only_file()`: commit `a.txt`;
    `before`; edit and make it read-only; `after`; `restore(after → before)` is `Ok`; content
    restored;
  - `tests/stage_commit.rs` `#[cfg(windows)] fn discarding_a_read_only_untracked_file_deletes_it()`.
- [ ] W1's #194 tests turn green.
- [ ] Commit `windows walker: delete read-only files (#194)`.

### Task W3: open every component relative to the held parent (#193, D5-A)

**Files:** `beneath_windows.rs`.

- [ ] **Implement** per spec §4:
  - `fn nt_open(parent: &File, name: &OsStr, access: u32, share: u32, disposition: u32, options:
    u32, attributes: u32) -> io::Result<File>`:
    - `create_dir_in`'s body, returning `File::from_raw_handle`;
    - `ERROR_CANT_RESOLVE_FILENAME` (1921) → `redirected()`;
    - `create_dir_in` = `nt_open(.., FILE_CREATE, FILE_DIRECTORY_FILE | ..)` then drop;
  - constants: `FILE_OPEN: u32 = 1`, `FILE_NON_DIRECTORY_FILE: u32 = 0x40`;
  - callers:
    - `walk` l. 499 (component, and the reopen after `Created`);
    - `open_leaf_at(at, k)` taking the component index (the `Clear` branch and
      `remove_and_prune` pass theirs);
    - `reopen_same`;
    - `create_leaf`.
    The `in_parent` checks stay;
  - `fn link_target(file: &File) -> io::Result<Vec<u8>>`:
    - `DeviceIoControl(FSCTL_GET_REPARSE_POINT = 0x000900A8)` into a 16 KiB buffer
      (`MAXIMUM_REPARSE_DATA_BUFFER_SIZE`);
    - parse tags 0xA000000C (symlink: flags `u32` after the four `u16` offsets/lengths;
      relative = 1) and 0xA0000003 (mount point) using the substitute name;
    - `\??\UNC\` → `\\`, `\??\` → strip;
    - `\` → `/`;
    - other tags → `Err`;
    - `read` uses it on the handle from `open_leaf` (keep the handle: `Ok(Some((leaf, found)))`);
  - module doc: rewrite the "opened by full path" and `create_dir_in` paragraphs (every
    component and leaf through `nt_open`; `in_parent` kept for SMB; 1921 means the held parent
    became a reparse point).
- [ ] Unit tests:
  - `fn nt_open_relative_to_a_converted_folder_is_refused()`: hold `d` via `nt_open(root,
    "d", ..)`, `junction_in_place(d, outside)`, then `nt_open(&d, "f", .., FILE_OPEN, ..)` →
    the `redirected()` message (pins the 1921 mapping);
  - `fn link_target_matches_read_link()`: a junction, plus `symlink_file` absolute and relative
    (`SKIP` without privilege); `link_target(open_leaf…)` equals `std::fs::read_link` after
    `\` → `/`.
  - Update `no_folder_is_created_in_a_junction_target_during_the_walk`: `created == 0` and the
    error message is `redirected()`'s (was raw 1921).
- [ ] W1's #193 tests turn green; every walker test, `worktree_guard.rs` Windows tests,
  `tests/undo.rs` `undo_restore_never_writes_through_a_junction` and
  `tests/case_insensitive_links.rs` stay green on `windows-latest`.
- [ ] Commit `windows walker: open every component relative to the held parent (#193)`.

### Task W4: docs, changelog, gate (PR 2)

- [ ] `CHANGELOG.md` → `### Windows walker`:
  - `Fixed (Windows): undo, redo and discards no longer fail with "Access is denied" on read-only files in the working tree (#194).`
  - `Windows: every file and folder of a working-tree read, write or delete is opened through the folder Browsitory already holds, so a folder swapped for a junction or symbolic link at that moment can't make Browsitory open a file in the link's target, briefly create one there, or contact a network share it points to (#193).`
- [ ] `docs/ARCHITECTURE.md` l. 557–580:
  - every component and leaf via `NtCreateFile` relative to the held parent, `in_parent` kept
    for SMB, 1921 → "changed";
  - link targets read from the handle (`FSCTL_GET_REPARSE_POINT`);
  - `IGNORE_READONLY_ATTRIBUTE`, the tiered fallback and the classic fallback clearing
    read-only.
- [ ] Gate:
  - `cargo build/test/clippy/fmt --workspace` on Linux;
  - CI `windows-latest` green;
  - `python3 scripts/check-changelog.py --base origin/main`.
- [ ] Push. Open the PR with:
  - D5, D6, D8 and their answers;
  - the red-then-green CI runs;
  - "Closes #193, closes #194";
  - unrelated finding 1 (read-only write) suggested as a follow-up issue.

---

## Decision hooks

| Decision | If the user picks… | Change |
|---|---|---|
| D1-B | verify after `add_path` | A2: keep `add_path`; afterwards compare the stage-0 id with `staged_blob(bytes)`; on mismatch `index.read(true)` (discard) and return `NotAFile`. The FIFO test stays `#[ignore]` with a reason. |
| D1-C | document only | Drop A1/A2; the ARCHITECTURE residual sentence stays; close #199 with the obsolete note. |
| D3-B | system glob database | B1: `fn desktop_globs() -> Option<Vec<(String, String, bool)>>` parsing `$XDG_DATA_HOME` and `$XDG_DATA_DIRS` `mime/globs2` once (`OnceLock`); a minimal fnmatch (`*`, `?`, `[…]`, the `cs` flag); sniff only when no glob matches the name; `None` → sniff. B2 becomes a parser test on a fixture string. |
| D3-C | GLib rule | B1: drop `is_desktop_entry` from `file_may_run` (keep it for tests or delete it with its tests); only `RUNNABLE_EXTENSIONS` covers `.desktop`/`.kdelnk`. B2 dropped. The CHANGELOG says launchers are recognised by name only. |
| D4-B | refusing commands only | C1: `Contention::wait(now, refuses: bool)`; contended and `!refuses` → `ZERO`. `undo::refuses(&Command)` extracted from `reply_busy`'s match (touches `undo.rs`). The burst test uses `stage_file` (≈0 s); a second test shows `discard_file` waits `CONTENDED_WAIT`. |
| D4-C | N strikes | C1: `Contention { strikes: u8 }`; after 3 contended Busy outcomes `ZERO` until `held`. Tests: the 4th contended command takes < 100 ms. |
| D5-B | drop `in_parent` | W3: remove `in_parent`/`final_path` and their calls; ARCHITECTURE notes SMB is only narrowed. |
| D5-C | leaves only | W3: only `open_leaf_at`, `reopen_same`, `create_leaf` and `link_target`; components stay `open_pinned`. W1's read test may stay red if the component reopen is reached; adjust it to a leaf conversion. |
| D6-B | fail clearly | W2: no attribute clearing; a new `io::Error` "'…' is read-only; clear its read-only flag and try again." on class-4 access denied with the bit set; USER_GUIDE mentions it; `delete_classic` test expects that error. |
| D6-C | flag only | W2: only add 0x10 to the first tier (keep the tier without it); no `FILE_WRITE_ATTRIBUTES`; drop the `delete_classic` test. |
| D7-B | three PRs | Part C becomes PR `feat/state-lock-bursts` with its own `### Undo lock` CHANGELOG section and gate; Parts A, B, Z stay PR 1. |
| D7-C | one PR | Everything on `feat/security-followups`; one CHANGELOG section with both bullet sets; Z2 also waits for `windows-latest`. |
| D8-B | install mingw | Global Constraints: `cargo clippy --target x86_64-pc-windows-gnu -p git-core --all-targets -- -D warnings` before every Part W commit; no winprobe. Closes #196 if documented in CLAUDE.md. |
| D8-C | CI only | Skip the red push; the PR says the tests were written first but only CI ran them. |

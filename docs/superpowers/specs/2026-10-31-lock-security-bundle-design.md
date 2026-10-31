# Lock and security leftovers

## Goal

Close nine follow-ups from the security follow-ups (#160), rebase/undo safety (#173) and hooks
(#176) batches:

| Issue | Area | Problem |
|---|---|---|
| #175 | undo lock | A forked child keeps a released state lock held; after one timeout the worker only tries once; `finish` can record an operation that ended outside; pre-upgrade marker; lock held across signing/network. |
| #174 | undo lock | `SaveStash` and `CreateWorktree` check the gate without holding the state lock. NFS and coarse-mtime notes. |
| #162 | case folding | Worktree add decides case folding from the main repository, not the destination volume. |
| #166 | case folding | NFC/NFD-equivalent names and `ẞ`/`ß` are not folded together. |
| #165 | launch | `read_head` and `mark_resolved` read by path after a type check (FIFO swap hangs); `.desktop` sniffing misses later groups, `# Config File` magics and `.kdelnk`. |
| #168 | Windows walker | Plain `FileDispositionInfo` leaves a name delete-pending while another process has it open; the following create fails. |
| #164 | Windows walker | No test converts a held empty folder into a junction during the walk. |
| #163 | Windows walker | A missing folder is created by path (`CreateDirectoryW`). |
| #180 | Windows hooks | A hook can start a process before `AssignProcessToJobObject`. |

The lock items of #182 (`only_the_first_locked_command_waits_while_the_lock_stays_held`, both
failures) are fixed here; its two E2E items are not.

## Shipping

Two PRs (D11-A), one CHANGELOG section each under `## [Unreleased]`:

- **PR 1** `feat/lock-security-bundle`: #175, #174, #166, #162, #165 (Parts A–C of the plan).
  `### Lock and security follow-ups`. Changes `repo-service` (`worker/state_lock.rs`,
  `worker/undo.rs`, `worker/mod.rs`) and `git-core` (`worktree_guard.rs`, `worktree.rs`,
  `launch.rs`, `merge.rs`). One new dependency: `unicode-normalization` (D7-A).
- **PR 2** `feat/windows-hardening`: #168, #164, #163, #180 (Part W). `### Windows hardening`.
  Changes `git-core/src/worktree_guard/beneath_windows.rs` and `git-core/src/process.rs` +
  `process/job_windows.rs`. Hand-written kernel32/ntdll FFI, no new dependency. Verified on
  `windows-latest` CI only.

No IPC, DTO or frontend change. No new user-visible string.

## Current state (main at 5ae3d42e, checked 2026-10-31)

### The state lock (#172/#173)

- `worker/state_lock.rs`: `StateLock::acquire(wait)` (l. 57) opens
  `<gitdir>/browsitory/state.lock` and polls std's `File::try_lock` (`flock` on Linux/macOS,
  `LockFileEx` on Windows). `StateGuard { _file: File }` (l. 30) releases by closing the file.
  `LOCK_WAIT` is 10 s, 300 ms under `cfg(test)` (l. 17–20).
- `worker/mod.rs` l. 815–859: `locks_state` commands take the lock with `LOCK_WAIT`; after one
  `Busy`, `contended` makes later ones try once (`Duration::ZERO`) until one succeeds. On
  `Busy`, `reply_busy` refuses Undo/Redo/discards/hard reset, everything else runs unrecorded.
  The gate (`gate_during_operation`) runs after the lock (l. 881).
- `undo::locks_state` (`undo.rs` l. 122): everything `plan_for` records, plus RebaseContinue,
  AbortRebase, ContinueSequence, AbortSequence, AbortMerge, Undo, Redo. `SaveStash` and
  `CreateWorktree` are not in it but are gated (`gate.rs` l. 64–67).
- `UndoRecorder::settle` (l. 391) and `finish` (l. 440) take the lock themselves (zero wait)
  when the worker didn't. `track = plan.is_some() || has_pending()`, so while an entry is
  pending every command, reads included, runs `settle` and `finish`.

### Why a released lock stays busy (#175 comment, #182)

`flock(2)`: locks belong to the open file description; "duplicate file descriptors (created
by, for example, fork(2) or dup(2)) refer to the same lock … the lock is released either by an
explicit LOCK_UN operation on any of these duplicate file descriptors, or when all such file
descriptors have been closed." std opens with `O_CLOEXEC`, but between `fork` and `exec` a
child holds a copy of every descriptor. Since #126, hooks and signers spawn with fork+exec
(`pre_exec(setsid)`), and any thread of the process (another repository's worker, a parallel
test) may be forking. So `drop(guard)` doesn't release the lock until that child execs. This is
the cause of the 2 s retries added in #176 (`state_lock.rs` l. 164–167, `mod.rs` l. 6964–6974),
and of #182's macOS failure (`mod.rs:6887`, "running the command without recording it").

The issue comment's option "open-file-description locks (`F_OFD_SETLK`)" would not help:
`fcntl_locking(2)` says OFD locks "are inherited across fork(2) … and are only automatically
released on the last close of the open file description", the same as `flock`, and they are
Linux-only. Classic POSIX record locks are not inherited, but they don't exclude two threads of
one process and are dropped by any `close` of the file.

### `finish` records an operation that ended outside (#175, first bullet)

Pending merge entry, merge committed in a terminal, the other window holds the lock. A file
watcher `GetStatus` comes in: `settle` sees the entry stale, can't get the lock (zero wait), and
leaves it. The other window releases. `finish` (no `active` plan, marker `None`, pending
present) gets the lock and records the pending entry as finished. Undo still verifies the
repository first, so this is a mislabel, not data loss.

### The pre-upgrade marker (#175, third bullet): obsolete

Pending entries are persisted since #169 (6044d7bc, 2026-10-29). 0.3.0 (2026-10-21) kept them in
memory (`UndoRecorder.pending: Option<(String, RepoSnapshot)>`), so an upgrade (a restart)
always lost them. No released build wrote a marker without `@<mtime>`. Only the CHANGELOG
sentence "A paused merge or pick from the previous version is not undoable as one step after
upgrading (#172)." and ARCHITECTURE "a pending entry from a build before this marker format is
dropped once" (l. 210) need correcting.

### Lock across signing/network (#175, fourth bullet), NFS and mtime (#174)

All three are documented already (ARCHITECTURE "Undo stack" l. 205–243). See D4 and D5.

### #162

`worktree::create_worktree` (`worktree.rs` l. 77) checks the target tree with
`refuse_case_collision` (l. 104), which applies when `folds_case(repo)` (`worktree_guard.rs`
l. 366): `core.ignorecase`, or always on macOS/Windows. libgit2 probes the file system when it
initializes a repository (clone sets `core.ignorecase` for the clone), but a worktree shares the
main repository's config. On Linux, a worktree on a vfat/exFAT stick or in an ext4 `+F`
(casefold) folder folds case while the repository's config says it doesn't.

### #166

`fold_name` (`worktree_guard.rs` l. 351) maps each character upper → lower, without HFS+
ignorables. `ẞ` (U+1E9E) → `ẞ` → `ß`, but `ß` → `SS` → `ss`, so link `ẞ` + folder `ß/x` passes.
No normalization: link `é` (U+00E9) + folder `e\u{301}/x` passes, while APFS and HFS+ compare
them equal (NTFS doesn't; folding there only refuses more). `icu_normalizer` is already in
Cargo.lock (via `url` → `idna`), but its license is Unicode-3.0, outside the policy.

### #165

- `read_head` (`launch.rs` l. 537): `fs::metadata(path)?.is_file()`, then `File::open(path)`.
  A FIFO swapped in between blocks `open`.
- `mark_resolved` (`merge.rs` l. 464): `symlink_metadata(full).is_file()`, then
  `fs::read(&full)` (l. 479). Same race.
- `is_desktop_entry` (l. 337) decides on the first non-blank, non-comment line. shared-mime-info
  (`freedesktop.org.xml`, checked locally) types `application/x-desktop` by magic
  `[Desktop Entry]` (offset 0:32), `[Desktop Action` / `[KDE Desktop Entry]` / `# Config File` /
  `# KDE Config File` (offset 0) and globs `*.desktop`, `*.kdelnk`. GLib accepts
  `[Desktop Entry]` in any group position. `kdelnk` is not in `RUNNABLE_EXTENSIONS` (l. 172).

### The Windows walker (#163, #164, #168)

`worktree_guard/beneath_windows.rs`:

- `walk` (l. 308) holds each folder without `FILE_SHARE_DELETE` and checks each opened
  component with `in_parent` (`GetFinalPathNameByHandleW`). A missing folder under
  `OnMissing::Create` is created with `std::fs::create_dir(&path)` (l. 343), by path, then
  reopened and checked. If the held parent became a junction in that instant, the folder is
  created in the junction's target (it stays empty; the reopen fails `in_parent`).
- `delete_by_handle` (l. 140) sets class 4 `FileDispositionInfo`. While another handle
  (antivirus, indexer) has the file open with delete sharing, the name stays delete-pending:
  `replace` (l. 598) then fails in `create_leaf` with access denied, and `remove_and_prune`
  (l. 572) can't remove the parent (not empty).
- Tests (l. 627–720) cover a held folder that can't be renamed and a pre-existing junction. No
  test converts a held empty folder in place.

### #180

`process::run_captured` (`process.rs` l. 277) spawns, then `job_windows::Job::assign(&child)`
(l. 302). Anything the hook starts before the assignment escapes the job. std closes the
primary thread handle after `CreateProcessW`. `run_captured_split` (signing) has no job, by
design. `KILL_ON_JOB_CLOSE` stays off (D6-A of the hooks bundle).

### Toolchain

rustc 1.97.1. The `x86_64-pc-windows-gnu` std is installed but there is no
`x86_64-w64-mingw32-gcc`, so `libgit2-sys` can't build for Windows and `cargo check --target
x86_64-pc-windows-gnu -p git-core` fails. CI runs `cargo clippy`/`cargo test --workspace` on
ubuntu-, macos- and windows-latest (`ci.yml` l. 55–88), and the case-insensitive probe table on
macOS/Windows.

## Design

### 1. #175: release the lock explicitly (D1-A)

`impl Drop for StateGuard { fn drop(&mut self) { let _ = self._file.unlock(); } }`. `LOCK_UN`
on our descriptor releases the lock for every duplicate, so a child between fork and exec no
longer holds it. Windows: `UnlockFile` on the handle (std handles aren't inheritable, so this is
for symmetry). A panicking holder unwinds through `Drop` too. The module docs say why. The two
#176 retries go back to strict checks, as regression tests.

### 2. #175: a short wait after a timeout (D2-A)

`CONTENDED_WAIT` = 1 s (100 ms under `cfg(test)`) replaces `Duration::ZERO` for `contended`.
`LOCK_WAIT` under `cfg(test)` goes from 300 ms to 1 s, so the timing tests have a margin of
900 ms instead of 200 ms. A long holder in the other window (a pull, a pinentry prompt) still
costs one 10 s wait, then 1 s per locked command until the lock is taken.

### 3. #175: don't record an entry that was stale before the command (D3-A)

`UndoRecorder` gains `stale_unsettled: Option<String>`. `settle` resets it, and when it finds
the pending entry stale but can't take the lock it stores that entry's `operation`. In
`finish`'s no-`active` branch, after `exclusive` + reload, if the pending entry's `operation`
equals `stale_unsettled` it is dropped and persisted (as for `aborted`), not recorded. A
command that ended the operation itself saw a matching marker at `settle`, so it still records.
If the other window recorded the entry meanwhile, the reload shows no pending entry.

### 4. #174: stash save and worktree add hold the lock

`locks_state` adds `Command::SaveStash { .. } | Command::CreateWorktree { .. }`. They have no
plan, so nothing is recorded. With the lock held, the gate check and the operation are atomic
with respect to the other window's rebase or sequence start. On `Busy` they run as today
(unrecorded, gate checked unlocked), consistent with D6-A of #173.

### 5. #162: probe the destination (D6-A)

`worktree_guard::dir_folds_case(dir: &Path) -> io::Result<bool>`: a `tempfile::Builder`
(`.browsitory-case-probe-` prefix) file in `dir`, then `symlink_metadata` of its name with ASCII
case flipped. The file is removed on drop. `create_worktree` probes the nearest existing
ancestor of `path` (libgit2 creates the missing ones, and new folders inherit ext4 casefold)
and checks collisions when `folds_case(repo) || probe == Ok(true)`. A probe error falls back to
`folds_case(repo)`.

### 6. #166: normalize and fold to a fixed point (D7-A)

`fold_name` for valid UTF-8: `nfd()` → drop HFS+ ignorables → per character lower → upper →
lower → `nfd()` again. For `ẞ`: `ß` → `SS` → `ss`. For `ß`: `ß` → `SS` → `ss`. Non-UTF-8 stays
ASCII-lowercased. It stays an over-approximation (NTFS doesn't normalize; HFS+ uses a
Unicode 3.2 decomposition). Collisions found as a result are only refused, never written. The
exempt-collision logic compares exact spellings and doesn't change.

### 7. #165: open first, then check the handle; a key-file sniff (D8-A)

- `read_head`: `OpenOptions::read(true)` plus `custom_flags(O_NONBLOCK | O_NOFOLLOW)` on Unix,
  then `file.metadata()?.is_file()`. A link fails `ELOOP` (`None`), and `file_may_run` already
  checks its canonical target. Windows unchanged.
- `mark_resolved`: the marker check reads through `worktree_guard::open_beneath(workdir, path)`.
  `None` (not a regular file, missing, beyond a link) skips the check as for non-files today.
- `is_desktop_entry(head, complete)`:
  - `# Config File` / `# KDE Config File` at offset 0 (BOM stripped) → `Some(true)`;
  - skip blank and `#` lines;
  - the first other line must be a group header `[…]`, else `Some(false)` (not a key file);
  - a desktop group header (`[Desktop Entry]`, `[KDE Desktop Entry]`, `[Desktop Action …]`) at
    any later position → `Some(true)`;
  - another group header or a line with `=` → keep going;
  - any other line → `Some(false)`;
  - end of a complete head → `Some(false)`; a partial head → `None` (read on to
    `DESKTOP_SNIFF_LIMIT`, then count it as runnable, as today).
- `"kdelnk"` joins `RUNNABLE_EXTENSIONS`.

### 8. #168: POSIX delete

`delete_by_handle` first sets class 21 `FileDispositionInfoEx` with
`FILE_DISPOSITION_FLAG_DELETE | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS` (`struct { flags: u32 }`).
On `ERROR_INVALID_PARAMETER` (87), `ERROR_NOT_SUPPORTED` (50) or `ERROR_INVALID_FUNCTION` (1)
(older Windows, FAT/exFAT, some shares) it falls back to class 4, as std's `remove_dir_all`
does. The name leaves the namespace when our handle closes, even if others keep the file open.
`IGNORE_READONLY_ATTRIBUTE` is not added (a read-only file still fails, as today).

### 9. #164: tests for in-place junction conversion

A test helper `junction_in_place(dir, target)`: open `dir` with
`FILE_WRITE_DATA | FILE_WRITE_ATTRIBUTES`, share `READ | WRITE`,
`FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT`, then
`DeviceIoControl(FSCTL_SET_REPARSE_POINT = 0x000900A4)` with a mount-point
`REPARSE_DATA_BUFFER` (tag `0xA000_0003`, substitute name `\??\<target>`, print name
`<target>`). It runs from the walk's `Step::Opened("d")` callback. If NTFS refuses the
conversion of a held folder, the tests assert that refusal instead, and the "residual" notes
say the race is not reachable.

### 10. #163: create relative to the held parent (D10-A)

`create_dir_in(parent: &File, name: &OsStr) -> io::Result<()>` with ntdll FFI:

```rust
#[link(name = "ntdll")]
extern "system" {
    fn NtCreateFile(handle: *mut *mut c_void, access: u32, attributes: *mut ObjectAttributes,
        io_status: *mut IoStatusBlock, allocation: *mut i64, file_attributes: u32, share: u32,
        disposition: u32, options: u32, ea: *mut c_void, ea_len: u32) -> i32;
    fn RtlNtStatusToDosError(status: i32) -> u32;
}
```

`ObjectAttributes { length, root_directory: parent handle, object_name: &UNICODE_STRING(name),
attributes: OBJ_CASE_INSENSITIVE (0x40), .. null }`, access
`FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | SYNCHRONIZE`, share `READ | WRITE`, disposition
`FILE_CREATE` (2), options `FILE_DIRECTORY_FILE (0x1) | FILE_SYNCHRONOUS_IO_NONALERT (0x20) |
FILE_OPEN_FOR_BACKUP_INTENT (0x4000) | FILE_OPEN_REPARSE_POINT (0x20_0000)`. On success the
handle is wrapped in a `File` and closed; a failure maps through `RtlNtStatusToDosError`
(`STATUS_OBJECT_NAME_COLLISION` → `ERROR_ALREADY_EXISTS`, kept as today's `continue`). The
reopen by path and the `in_parent` check stay. Names come from `plain_relative` components and
the root is a `\\?\` path, so trailing dots/spaces behave as with `create_dir` today.

### 11. #180: start the hook suspended (D9-A)

- `run_captured` (Windows): `creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED (0x4))` after
  `detach`, spawn, `Job::assign`, then `job_windows::resume(&child)`.
- `resume`: `CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0)`, `Thread32First`/`Thread32Next`
  over `THREADENTRY32` (28 bytes); for each thread owned by `child.id()`,
  `OpenThread(THREAD_SUSPEND_RESUME, 0, tid)`, `ResumeThread`, `CloseHandle`. If nothing was
  resumed, that is an error.
- On a resume error, `kill_and_reap(child, job)` and `RunError::Spawn`: a suspended hook must
  never wait out the 10-minute timeout.
- If `assign` fails, the hook is still resumed (today's fallback: only the child is killed).
- Signing (`run_captured_split`) and `process::spawn` are unchanged.

## Open decisions (provisional choice in **bold**; the plan implements it)

**D1. How the lock stops being held by a forked copy (#175). Engineering.**
- **A. Explicit unlock in `StateGuard::drop`.** Works for every duplicate, on every platform.
- B. OFD locks, as the issue comment suggests. They don't help (inherited across fork, released
  on last close) and are Linux-only.
- C. Only retries/short waits. The spurious refusals and unrecorded commands stay, just less
  often.
Recommendation **A**.

**D2. After a timed-out wait, how long later locked commands wait (#175). User-facing.**
- **A. 1 s for every locked command until the lock is taken again.**
- B. 1 s only for the commands that refuse (Undo, Redo, discards, hard reset); others try once.
  Others then still run unrecorded on a brief hold.
- C. Keep try-once; D1-A already removes the fork-caused cases. A brief hold by the other
  window's staging still causes a refusal.
Recommendation **A**: at most 1 s extra per click while the other window holds the lock for a
long time.

**D3. A pending entry already stale before the command (#175). Engineering.**
- **A. `finish` drops it.**
- B. Record it, labelled "(finished outside Browsitory)". Needs a label rule and may record an
  abort as an entry.
- C. Leave it (Undo verifies the state first).
Recommendation **A**: matches what `settle` would have done with the lock.

**D4. The lock across signing and network (#175). User-facing.**
- **A. Keep it by design and document it (ARCHITECTURE and USER_GUIDE already say so).**
- B. Pull fetches outside the lock and integrates under it. This splits `PullCurrentUpstream` in
  the worker.
- C. Commit releases the lock around the signer. `finish`'s invariant ("ended but not
  recorded") would need rework.
Recommendation **A**. Close the bullet; open a follow-up only if users report freezes.

**D5. NFS and coarse mtime (#174). Engineering.**
- **A. Document only (already in ARCHITECTURE l. 208–210, 242–243).**
- B. A process-wide registry (mutex + condvar keyed by the canonical lock path) in front of
  `flock`, so two workers of one process exclude each other on NFS too.
- C. B plus extra marker data. Inode numbers are reused at once on ext4, so this is unreliable.
Recommendation **A**.

**D6. How worktree add learns the destination's case folding (#162). Engineering.**
- **A. A temp-file probe in the nearest existing ancestor, combined with `folds_case(repo)`.**
- B. A read-only probe: look up an existing entry with its case flipped. False positives when
  both spellings exist, and empty folders need A anyway.
- C. Always check on worktree add (`CasePolicy::Always`). No probe, but a Linux repository whose
  HEAD holds such a pair can't add worktrees anywhere.
Recommendation **A**.

**D7. Unicode normalization (#166). User-facing (dependency policy).**
- **A. `unicode-normalization` (MIT OR Apache-2.0; pulls `tinyvec`, Zlib OR Apache-2.0 OR MIT).**
- B. A hand-written decomposition table for Latin-1/Latin Extended-A. No dependency, but other
  scripts remain a residual.
- C. `icu_normalizer` (already transitive). Unicode-3.0 license, a second policy exception.
Recommendation **A**.

**D8. Desktop entry sniffing (#165). User-facing.**
- **A. Key-file rule (first group header, a desktop group anywhere, stop on a non-key line),
  plus `# Config File` / `# KDE Config File` and `.kdelnk`.** A key file over 64 KiB with no
  desktop group counts as runnable (Open in editor asks for an editor).
- B. Only the magics and `.kdelnk`; the first group must still be the desktop one.
Recommendation **A**.

**D9. Resuming the suspended hook (#180). Engineering.**
- **A. Toolhelp32 thread walk (documented kernel32).**
- B. `NtResumeProcess` (ntdll, undocumented but stable; one call).
- C. Our own `CreateProcessW` with `PROC_THREAD_ATTRIBUTE_JOB_LIST`. Reimplements std's stdio,
  environment and quoting.
Recommendation **A**.

**D10. #163 scope. Engineering.**
- **A. Only folder creation goes through `NtCreateFile` relative to the held parent.**
- B. Every component opened handle-relative (`NtCreateFile` with `FILE_OPEN`), dropping the
  `GetFinalPathNameByHandleW` checks. A rewrite of the walker.
Recommendation **A**.

**D11. One PR or two. Process.**
- **A. Two PRs: Linux-testable (Parts A–C) and Windows (Part W).**
- B. One PR.
Recommendation **A**: Part W can only iterate on `windows-latest`, and its unsafe FFI deserves
its own review.

**D12. Windows verification. Process.**
- **A. CI is the authority, plus a type-check through a throwaway `target/winprobe` crate
  (deleted afterwards), plus a test-first commit pushed on the PR 2 branch so CI shows the new
  tests red before the fixes land.**
- B. Install mingw-w64 (`sudo apt install gcc-mingw-w64-x86-64`), so local `cargo clippy --target
  x86_64-pc-windows-gnu --workspace` works.
- C. CI only, saying in the PR that the tests were written first but not shown red.
Recommendation **A**.

## Testing

Real temp-dir repositories, never a mocked `Repository`.

- **#175/#174** (`repo-service`):
  - `state_lock.rs` tests:
    - a cloned descriptor doesn't keep a dropped guard's lock (all platforms);
    - `#[cfg(unix)]` a child forked while the lock was held doesn't keep it after release;
    - the panicking-holder test is strict again.
  - `mod.rs` `state_lock_tests`:
    - `locks_state` covers `SaveStash`/`CreateWorktree`;
    - stash save and worktree add wait for the lock and are then refused by a sequence stored
      meanwhile;
    - after a timeout a locked command waits at most about `CONTENDED_WAIT`;
    - `only_the_first_…` rewritten with margins;
    - `undo_replies_busy_…` strict;
    - a merge committed outside while the other window held the lock leaves no undo entry
      (`spawn_with_pause_before_finish`);
    - `a_rebase_continued_while_another_worker_reads_is_still_one_undo_entry` stays green.
- **#166/#162** (`git-core`):
  - `worktree_guard.rs` unit tests (NFC/NFD, `ẞ`, exhaustive idempotence over every scalar
    value, the probe on each platform);
  - `tests/worktree.rs`;
  - a `#[cfg(target_os = "macos")]` test in `tests/case_collision.rs`.
- **#165** (`git-core`):
  - `launch.rs` sniffing tests (red today);
  - `#[cfg(unix)]` FIFO tests for `file_may_run` and `mark_resolved` (regression guards; the swap
    race can't be made deterministic).
- **Part W** (all `#[cfg(windows)]`, `windows-latest` only):
  - `beneath_windows.rs`: POSIX delete under a foreign handle (two tests), in-place junction
    during write, read and create;
  - `job_windows.rs` / `process.rs`: suspended until resumed, in the job before it runs, and the
    #122 tests.

## Docs

- `docs/ARCHITECTURE.md`:
  - "Undo stack" (l. 199–246): explicit unlock and why (fork copies); `CONTENDED_WAIT`; stash
    save and worktree add hold the lock; the stale-at-settle rule; drop the "build before this
    marker format" clause;
  - "Git hooks" (l. 455–465): suspended start, no residual race;
  - "Worktree writes and symlinks" (l. 545–556): POSIX delete, `NtCreateFile` creation, the
    residual removed (or "not reachable", per W2); case folding (l. 584–596): NFD and the
    fixed-point fold, the destination probe for worktree add;
  - l. 714–716: the sniff rule.
- `docs/USER_GUIDE.md`:
  - l. 663–667: "about 10 seconds … then about a second per action";
  - l. 467–475: worktrees on a case-insensitive drive, accents in either form.
- `docs/LICENSE_COMPLIANCE.md`: a `unicode-normalization` row (PR 1).
- `CHANGELOG.md`: remove the sentence "A paused merge or pick from the previous version is not
  undoable as one step after upgrading (#172)."

## Risks

- **D2-A** adds up to 1 s per locked command while the other window holds the lock for a long
  time.
- **D3-A** may drop an entry that an *unrecorded* command in the other window ended. That is the
  same outcome as `settle` holding the lock.
- **#162 probe** writes and deletes a hidden temp file in the destination's parent.
- **#166** refuses a few more trees on Windows/Linux (normalization-equal names), consistent
  with the existing over-approximation.
- **D8-A**: a key file over 64 KiB with no desktop group counts as runnable.
- **Part W** is unverifiable locally. `NtCreateFile` and `FSCTL_SET_REPARSE_POINT` behaviour on
  a held folder is confirmed only by CI. `CREATE_SUSPENDED` misuse could hang a hook until the
  timeout, so a failed resume kills it.

## Unrelated findings (not fixed here)

1. `mark_resolved` used to read *through* a symlinked folder (`full = workdir.join(path)`); with
   `open_beneath` it doesn't, and stages as for a non-file. `stage_file` refuses paths beyond a
   link ("beyond a symbolic link"); `mark_resolved` doesn't. Worth a follow-up issue.
2. Worktree add doesn't set anything about case folding for the new worktree (the config is
   shared). Libgit2 probes only on init.
3. The CHANGELOG/ARCHITECTURE "pre-upgrade marker" claims (see Current state) are corrected here.

## Out of scope

- D4-B/C (lock-free fetch, lock-free signing), D5-B (process-wide registry).
- OneDrive/Cloud Files placeholder tests (no placeholder provider on CI).
- `IGNORE_READONLY_ATTRIBUTE` deletes; a handle-relative walker (D10-B).
- #182's E2E items (Tauri session start, line-staging stale element).

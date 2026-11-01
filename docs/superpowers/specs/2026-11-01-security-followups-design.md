# Security follow-ups

## Goal

Close five follow-ups from the lock and security bundle (#191) and the Windows hardening PR (#190):

| Issue | Area | Problem |
|---|---|---|
| #199 | mark resolved | The type check and libgit2's `add_path` read the file separately: a FIFO, link or new content swapped in between blocks the worker or stages unchecked content. Symlinked-folder paths are "not refused" (obsolete, see below). |
| #201 | launch | The `[Desktop Entry]` sniff applies Linux launcher rules on every OS and ignores the file name, so `notes.txt` quoting a launcher near its start is refused "Open". |
| #197 | undo lock | While contended, every queued locked command waits `CONTENDED_WAIT` (1 s): staging 20 files costs about 20 s. |
| #193 | Windows walker | Components and leaves are opened by path and checked afterwards; a converted held folder lets a leaf be briefly created in, or opened from, the link's target (or a UNC server contacted). |
| #194 | Windows walker | The POSIX delete lacks `FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE`: undo and discard fail on read-only files. |

## Shipping

Two PRs (D7-A), one CHANGELOG section each at the top of `## [Unreleased]`:

- **PR 1** `feat/security-followups`: #199, #201, #197 (Parts A–C, Z). `### Security follow-ups`.
  Changes `git-core` (`merge.rs`, `launch.rs`) and `repo-service` (`worker/state_lock.rs`, the
  acquire loop in `worker/mod.rs`). No new dependency.
- **PR 2** `feat/windows-walker-handles`: #193, #194 (Part W). `### Windows walker`. Changes
  `git-core/src/worktree_guard/beneath_windows.rs` only (plus Windows tests in `tests/undo.rs`,
  `tests/stage_commit.rs`). Hand-written ntdll/kernel32 FFI, no new dependency. Verified on
  `windows-latest` CI only.

No IPC, DTO or frontend change. No new user-visible string.

The parallel "robustness" batch (#179, #181–#183, #185–#187) touches the worker's hooks and
status code, `pick.rs`, `stash.rs`, `stage.rs`, the Tauri commands and E2E. This batch doesn't
touch those files; in `worker/mod.rs` only the acquire loop (l. 815–860) and `state_lock_tests`
change.

## Current state (main at 25040598, checked 2026-11-01)

### #199

- `merge::mark_resolved` (l. 470): `is_safe_relative`, `find_conflict`, `ensure_inside_workdir`
  (l. 480 → `stage::ensure_no_symlinked_parent`), then `symlink_metadata` (l. 482) refuses a
  FIFO/socket/device (`NotAFile`), the marker check reads through `open_beneath` (l. 494), and
  `index.add_path` (l. 504) or `remove_path` stages.
- libgit2 `git_index_add_bypath` → `index_entry_init` → `git_fs_path_lstat` →
  `git_blob__create_from_paths`, which opens with `O_RDONLY` (blocks on a FIFO, follows a link)
  and reads the content a second time. Between our check and that open the path can become a
  FIFO (worker hangs), a link (the link is staged, as of libgit2's own `lstat`), or new content
  (staged without the marker check).
- `Repository::blob_writer(Some(hint))` ends in `git_blob__create_from_paths(.., hint_path,
  try_load_filters = true)`, the same filter load as `add_path`; `stash::worktree_blob`
  (stash.rs l. 440) already hashes handle-read bytes this way.
- `add_bypath` moves conflict stages to REUC; `git_index_add` doesn't; `git_index_remove_bypath`
  (`Index::remove_path`) does.
- **Part 2 is obsolete**: `ensure_inside_workdir` has refused a path through a symlinked folder
  since e43c236a (2026-10-28), tested by `tests/merge.rs`
  `conflict_resolution_never_writes_through_a_symlinked_folder` (l. 1179, unix only). The
  lock bundle's "Unrelated finding 1" was mistaken.

### #201

- `launch::file_may_run` (l. 540) refuses on `may_execute` (name, exec bit), `has_magic`, or
  `is_desktop_entry` (l. 348: shared-mime-info's x-desktop magics, then the GLib key-file rule),
  for every `Os`.
- Probes, run in memory only:

  | Name and content | GLib 2.88 `g_content_type_guess` | Qt `mimeTypeForFileNameAndData` |
  |---|---|---|
  | `notes.txt` with `[Desktop Entry]` at offset 11 | text/plain | text/plain |
  | `LAUNCH` (extensionless) | text/plain | application/x-desktop |
  | `x.foo` (unknown extension) | text/plain | application/x-desktop |
  | `README` | text/x-readme | text/x-readme |
  | `x.ts` (two globs) | text/vnd.trolltech.linguist | text/vnd.trolltech.linguist |
  | `x.kdelnk` | application/x-desktop | application/x-desktop |

  GLib never types a named file as x-desktop from its contents. Qt sniffs only when no glob
  matches the name (and then a link is typed by the link's name). `file --mime-type` says
  text/plain for all of them.
- macOS and Windows have no `.desktop` launchers.
- `a_document_quoting_a_launcher_opens` (l. 999) passes only because its header is at offset
  39.

### #197

- `worker/mod.rs` l. 815–860: `contended` (a `bool`) selects `state_lock::CONTENDED_WAIT` (1 s,
  300 ms under `cfg(test)`, `state_lock.rs` l. 34–37) for every locked command after one waited
  `LOCK_WAIT` (10 s; 1 s in tests) in vain, until one takes the lock. Staging is locked, so N
  queued stages behind the other window's pull cost about N seconds, reads queued behind them
  included.
- Tests: `only_the_first_…` (l. 6991) and `after_a_timeout_…` (l. 7018) rely on the first
  command after the timeout waiting `CONTENDED_WAIT`.

### #193

`beneath_windows.rs`:

- Opened by full path, then checked with `in_parent` (`GetFinalPathNameByHandleW`, l. 325):
  - `walk` (l. 473) components via `open_pinned` (l. 358);
  - `open_leaf_at` (l. 419);
  - `reopen_same` (l. 450);
  - `create_leaf` (l. 605).
- `read` reads a link's target with `std::fs::read_link(at.leaf_path())` (~l. 716).
- Only `create_dir_in` (l. 263) is handle-relative (#163).
- A held folder that is empty (or emptied: its children aren't held) can be converted in place.
  The open by path then opens or creates the leaf in the target, or contacts a UNC target of a
  directory symlink, before `in_parent` refuses it.
- `no_folder_is_created_in_a_junction_target_during_the_walk` pins that `NtCreateFile` relative
  to a held parent that became a reparse point fails with `ERROR_CANT_RESOLVE_FILENAME` (1921):
  relative opens fail closed.

### #194

- `delete_by_handle` (l. 166): class 21 with `DELETE | POSIX_SEMANTICS`; on 87/50/1 class 4.
  A read-only file fails both with access denied.
- std's `remove_file` docs: "On Windows, DeleteFile is used or CreateFileW and
  SetInformationByHandle for readonly files."
- `IGNORE_READONLY_ATTRIBUTE` (0x10) exists since Windows 10 1809. Older builds with
  `FileDispositionInfoEx` answer 87 for the unknown flag.

### Toolchain

rustc 1.97.1, no `x86_64-w64-mingw32-gcc` (so no local Windows build; #196 open). CI runs fmt,
clippy and `cargo test --workspace` on ubuntu-, macos- and windows-latest. ubuntu-latest has
shared-mime-info (pulled in by the GTK dev packages).

## Design

### 1. #199: stage what was read (D1-A)

`mark_resolved` after the existing checks:

1. `previous_mode` = the stage-0 entry's mode, else stage 2 ("ours"), as libgit2's
   `index_merge_mode`.
2. `undo::read_worktree_file(workdir, path, &undo::worktree_config(repo)?, previous_mode)?`:
   - `Some((bytes, mode))`:
     - unless `force` or `mode == MODE_LINK`, `has_conflict_markers(&bytes)` →
       `MarkersRemain`;
     - test seam `after_check()`;
     - `id` = `repo.blob(&bytes)` for a link, else `blob_writer(Some(path))` + `write_all` +
       `commit`;
     - `index.remove_path(path)` (conflicts to REUC, as `add_bypath`);
     - `index.add(&undo::index_entry(path, id, mode))`;
     - `index.write()`.
   - `None`: `symlink_metadata(full)`:
     - `NotFound` → `remove_path` (deletion, as today);
     - a folder → `add_path` (a nested repository becomes a gitlink, as today; a folder swapped
       for a FIFO in that instant is a documented residual);
     - anything else (FIFO, socket, device, a regular file `read_beneath` refused because a
       folder became a link) → `NotAFile`.
3. Zeroed stat data (as undo's entries): status re-hashes the file once.

`UndoError` maps to `MergeError` as `stash::undo_error` does. `pub fn mark_resolved` delegates
to `#[doc(hidden)] pub fn mark_resolved_with_pause(repo, path, force, after_check: &mut dyn
FnMut())`. `stash::worktree_blob` stays as is (the robustness batch edits `stash.rs`); dedupe
later.

### 2. #201: glob before magic, Linux only (D3-A)

- `const DOCUMENT_EXTENSIONS: &[&str]`: long-standing shared-mime-info globs whose types aren't
  `application/x-desktop`:

  `txt md markdown rst org tex log csv tsv json yaml yml toml xml html htm xhtml svg css diff
  patch c h cpp hpp rs go java pdf rtf odt docx ipynb srt vtt`

- `fn names_a_document(path) -> bool`: the raw `Path::extension()`, ASCII-lowercased, in the
  list. Not `lower_extension`: Linux keeps trailing dots, so `notes.txt.` matches no glob.
- `file_may_run(file, os)`: `check(path, sniff_desktop)` applies `is_desktop_entry` only when
  `sniff_desktop`:
  - the file itself: `os == Os::Linux && !names_a_document(file)`;
  - a symlink's target: `os == Os::Linux && !(names_a_document(file) &&
    names_a_document(target))` (Qt types a link by the link's name and the target's content).
- `has_magic`, `RUNNABLE_EXTENSIONS` (`.desktop`/`.kdelnk` on every OS) and the exec bit are
  unchanged. `looks_runnable` (test-only) keeps its meaning.

### 3. #197: wait once per burst (D4-A)

`state_lock.rs`:

```rust
/// When a locked command waits for the lock, and how long (#175, #197).
#[derive(Debug, Default)]
pub(super) struct Contention { contended: bool, quick_until: Option<Instant> }
impl Contention {
    pub fn wait(&self, now: Instant) -> Duration;   // LOCK_WAIT | ZERO in window | CONTENDED_WAIT
    pub fn held(&mut self);                          // reset
    pub fn busy(&mut self, now: Instant);            // 1st: contended; later: quick_until = now + CONTENDED_WAIT
}
```

- The first Busy (after `LOCK_WAIT`) sets `contended` only, so the next command still waits
  `CONTENDED_WAIT`: a brief hold is still waited out right after a long one.
- Every later Busy opens or extends a `CONTENDED_WAIT` window in which locked commands try once.
- `Unavailable` changes nothing.
- `mod.rs` l. 815–860 swaps the `bool` for it. Comments are updated.

Cost:

| Situation | Today | With D4-A |
|---|---|---|
| N queued commands behind a long holder | 10 s + (N−1) × 1 s | 10 s + 1 s |
| A burst while already contended | N × 1 s | about 1 s |
| A lone click after a quiet second | up to 1 s | up to 1 s |

A refusing command (Undo, discard) inside a window refuses at once; the lock was seen busy
under a second before.

### 4. #193: open everything relative to the held parent (D5-A)

- `fn nt_open(parent: &File, name: &OsStr, access, share, disposition, options, attributes) ->
  io::Result<File>`: the body of today's `create_dir_in`, returning
  `File::from_raw_handle`.
  - 1921 maps to `redirected()`;
  - other `NTSTATUS` → `RtlNtStatusToDosError` (2, 3, 123, 267, 5, 32, 183 keep today's
    handling);
  - `create_dir_in` becomes a wrapper.
- Exact CreateFileW equivalents:
  - always `| SYNCHRONIZE`, `FILE_SYNCHRONOUS_IO_NONALERT`, `OBJ_CASE_INSENSITIVE`;
  - backup semantics → `FILE_OPEN_FOR_BACKUP_INTENT`, else `FILE_NON_DIRECTORY_FILE`;
  - `FILE_FLAG_OPEN_REPARSE_POINT` → `FILE_OPEN_REPARSE_POINT`;
  - `OPEN_EXISTING` → `FILE_OPEN` (1), `CREATE_NEW` → `FILE_CREATE` (2).
- Callers:
  - `walk` components: `nt_open(held, name, FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES, R|W,
    FILE_OPEN, BACKUP | REPARSE)`; the created-folder path reopens relatively too.
  - `open_leaf_at(at, name)`: `DELETE | FILE_READ_ATTRIBUTES | FILE_WRITE_ATTRIBUTES` (#194),
    R|W|D, `FILE_OPEN`, BACKUP | REPARSE. The `Clear` branch and `remove_and_prune`'s upward
    loop pass `names[i]` relative to `dirs[i]`.
  - `reopen_same`: `GENERIC_READ` (and `GENERIC_WRITE`), R|W|D, `FILE_OPEN`, NON_DIRECTORY
    (+ REPARSE unless `reopen`); the file-ID check stays.
  - `create_leaf`: `GENERIC_READ | GENERIC_WRITE | DELETE | FILE_READ_ATTRIBUTES`, R|W,
    `FILE_CREATE`, NON_DIRECTORY | REPARSE, `FILE_ATTRIBUTE_NORMAL`.
  - `read`'s link: `link_target(&leaf_handle)` with `FSCTL_GET_REPARSE_POINT` (0x000900A8) on
    the handle `open_leaf` returned:
    - symlink (0xA000000C): substitute name, kept as-is when `SYMLINK_FLAG_RELATIVE`;
    - mount point (0xA0000003): substitute name;
    - `\??\UNC\` → `\\`, `\??\X:` → `X:`, as std's `read_link`;
    - then `\` → `/` as today.
- `in_parent` stays after every open: on SMB the redirector resolves relative opens by path on
  the server.
- `open_pinned` stays test-only.
- Names are single `plain_relative` components; the root stays a `\\?\` path, so trailing
  dots/spaces and device names behave as now.

### 5. #194: ignore the read-only attribute (D6-A)

`delete_by_handle`:

1. Class 21 with `DELETE | POSIX | IGNORE_READONLY_ATTRIBUTE` (0x10).
2. On 87: class 21 with `DELETE | POSIX` (1607–1803).
3. On 87/50/1: `delete_classic`, which is class 4. On access denied with
   `FILE_ATTRIBUTE_READONLY` (from `attribute_tag`):
   - `FileBasicInfo` (class 0, `FILE_BASIC_INFO`: four `i64` times (0 = unchanged) +
     `u32` attributes, 40 bytes) with the bit cleared (`FILE_ATTRIBUTE_NORMAL` if nothing is
     left);
   - retry class 4;
   - restore the attributes if the retry fails.

Leaf handles carry `FILE_WRITE_ATTRIBUTES`. A replaced file is created afresh, so it is no
longer read-only, as after `git checkout`.

## Open decisions (provisional choice in **bold**; the plan implements it)

**D1. #199: how Mark resolved stages. Engineering.**
- **A. Stage the bytes read through the handle (`blob_writer` hint path, `remove_path` +
  `add`).** Same filters and REUC as `add_path`; no second read.
- B. Keep `add_path`; afterwards compare the staged id with the checked bytes' blob and undo on
  mismatch. A FIFO swapped in still hangs the worker.
- C. Document the residual; close #199.
Recommendation **A**.

**D3. #201: which names skip the launcher sniff. User-facing.**
- **A. A built-in list of document extensions, Linux only; CI checks it against
  shared-mime-info.** Deterministic; an unknown or unlisted extension is still sniffed (fails
  closed); on a desktop with older shared-mime-info lacking a listed glob, KDE could type such a
  file as a launcher (then KIO asks before running a non-executable one).
- B. Read the system's `mime/globs2` at run time and skip when any glob matches (Qt's exact
  rule). Mirrors each machine; needs a glob matcher and depends on the installed database.
- C. GLib's rule: never sniff a named file; only `.desktop`/`.kdelnk` by name. Simplest;
  reverts most of #165's sniffing and relies on KIO's "untrusted program" prompt on KDE.
Recommendation **A**.

**D4. #197: how long locked commands wait while another window holds the lock. User-facing.**
- **A. Once per burst: the first command after a timeout waits up to 1 s; while the lock keeps
  being found busy, later ones try once; after a quiet second, the next waits again.**
- B. Only Undo, Redo, discards and Reset hard wait 1 s; others try once (the lock bundle's
  D2-B). Twenty single-file discards still cost 20 s; other actions run unrecorded on a brief
  hold.
- C. Stop waiting after N (3) contended timeouts until the lock is taken. A brief hold after
  that refuses or runs unrecorded.
Recommendation **A**.

**D5. #193: scope. Engineering.**
- **A. Every component and leaf through `NtCreateFile` relative to the held parent, the link
  target from the handle, `in_parent` kept, 1921 reported as "a folder of the working tree
  changed".**
- B. As A, dropping `in_parent`. Fewer calls; SMB loses its only check.
- C. Leaves only; folders stay by path + `in_parent`.
Recommendation **A**.

**D6. #194: read-only files where POSIX delete is unavailable. Engineering (B is
user-facing).**
- **A. Tiered flags; the classic fallback clears the read-only attribute and retries.**
- B. Tiered flags; the fallback fails with a new message ("'x' is read-only; clear its
  read-only flag and try again.").
- C. Only add the flag; FAT/exFAT and shares still fail with access denied.
Recommendation **A**.

**D7. PRs. Process.**
- **A. Two PRs: PR 1 (#199, #201, #197), PR 2 (#193, #194).**
- B. Three: #197 alone, so it can land around the robustness batch's worker changes.
- C. One PR.
Recommendation **A**.

**D8. Windows verification. Process.**
- **A. CI is the authority, plus a throwaway `target/winprobe` type-check (deleted afterwards)
  and a test-first commit pushed on PR 2 so CI shows the new tests red.**
- B. Install mingw-w64 (#196) for local `cargo clippy --target x86_64-pc-windows-gnu`.
- C. CI only; the PR says the tests were written first.
Recommendation **A**.

## Testing

Real temp-dir repositories, never a mocked `Repository`.

- **#199** (`tests/merge.rs`):
  - red today, through the `mark_resolved_with_pause` seam:
    - content rewritten with markers after the check isn't staged;
    - `#[cfg(unix)]` a FIFO swapped in doesn't block;
    - `#[cfg(unix)]` a link swapped in isn't staged as a link;
  - guards:
    - CRLF with `core.autocrlf=true` stages LF;
    - `#[cfg(unix)]` exec bit → 100755;
    - `core.filemode=false` keeps ours' mode;
    - REUC is written (the index file contains `REUC`);
    - a link conflict stages a link;
    - deletion, markers and the FIFO tests stay green;
    - `#[cfg(windows)]` a junction ancestor is refused (`SymlinkedFolder`).
- **#201** (`launch.rs` unit tests):
  - red today:
    - `notes.txt` with `[Desktop Entry]` at offset 11 opens on Linux;
    - `LAUNCH` with a desktop entry is refused on Linux and not on macOS/Windows;
  - guards:
    - `x.foo`, `notes.txt.` and `LAUNCH` are still refused on Linux;
    - `.desktop`/`.kdelnk` are refused everywhere;
    - links: `notes.txt` → `LAUNCH` and `LAUNCH` → `notes.txt` refused, `a.md` → `b.txt` not;
  - `#[cfg(target_os = "linux")]` every `DOCUMENT_EXTENSIONS` entry has a glob in
    `/usr/share/mime/globs2` and none is `application/x-desktop` (skipped with a message when
    the file is absent, required when `CI` is set).
- **#197** (`repo-service`):
  - `Contention` unit tests with synthetic instants;
  - worker: after the timeout and one contended wait, 6 queued stages take
    < 3 × `CONTENDED_WAIT` (red today: ≥ 6 ×);
  - after a quiet `CONTENDED_WAIT` the next locked command waits ≥ `CONTENDED_WAIT` again;
  - `only_the_first_…`, `after_a_timeout_…` and `work_tree_destroying_…` stay green.
- **Part W** (all `#[cfg(windows)]`, `windows-latest` only):
  - red today:
    - `remove`, `replace` and `remove_and_prune` on a read-only leaf;
    - a read through a held folder converted to a junction never opens the target (the target
      is held with share 0, so today's error is a sharing violation, 32);
    - a write through it never makes a name appear in the target (`FindFirstChangeNotificationW`
      on the target, `WaitForSingleObject(h, 0)`);
  - with the implementation:
    - `nt_open` pins 1921 relative to a converted folder;
    - `delete_classic` deletes a read-only file on NTFS;
    - `link_target` equals `std::fs::read_link` for a junction and for absolute and relative
      symlinks (skipped without symlink privilege);
    - `FILE_BASIC_INFO` layout (40 bytes);
    - `tests/undo.rs`: undo restores a read-only file;
    - `tests/stage_commit.rs`: discarding a read-only untracked file deletes it;
    - all walker tests and `tests/symlink_write_through.rs` stay green;
    - `no_folder_is_created_…` asserts the "changed" message instead of the raw 1921.

## Docs

- `docs/ARCHITECTURE.md`:
  - "Undo stack" (l. 232–236): the per-burst wait;
  - "Worktree writes and symlinks" (l. 557–580): every component and leaf through
    `NtCreateFile` relative to the held parent (`in_parent` kept for SMB), link targets read
    from the handle, `IGNORE_READONLY_ATTRIBUTE` with the tiered fallback and the classic
    fallback clearing read-only;
  - "Launch" (l. 746–760): the x-desktop sniff is Linux-only and skipped for document
    extensions (glob before magic, as GLib and Qt); Mark resolved stages the bytes it checked
    (`read_worktree_file` + `blob_writer` + `remove_path`/`add`).
- `docs/USER_GUIDE.md`:
  - l. 670–671: "…each action waits about a second for it instead of 10; a quick run of
    actions waits only once.";
  - l. 702–705: "…and files whose contents start like a script, program or installer, or, on
    Linux, like a desktop launcher (unless the name ends in a document extension such as .txt
    or .md)."

## Risks

- **D1-A**: the index entry has zeroed stat data (status re-hashes once); a folder at the
  conflicted path still goes through `add_path` (documented residual).
- **D3-A**: on an old shared-mime-info lacking a listed glob, KDE may type a listed file as a
  launcher (KIO asks before running it unless executable, and the exec bit is refused anyway).
- **D4-A**: Undo or a discard inside a burst window refuses without waiting.
- **Part W** is unverifiable locally. The `NtCreateFile` option mapping and reparse-buffer
  parsing are confirmed by CI only. `FILE_WRITE_ATTRIBUTES` on leaf handles could fail under
  an unusual ACL that grants `DELETE` but not write-attributes.

## Unrelated findings (not fixed here)

1. Windows `write` to an existing read-only file (resolve conflict, add to `.gitignore`) fails
   with access denied (`reopen_same` asks `GENERIC_WRITE`). git replaces such files on checkout;
   worth an issue if users hit it.
2. `stash::worktree_blob` duplicates the new staging helper; dedupe after the robustness batch.
3. The lock bundle spec's "Unrelated finding 1" (`mark_resolved` beyond a link) was already
   fixed in e43c236a.

## Out of scope

- A handle-staged *folder* (gitlink) in `mark_resolved`.
- Reading the system MIME database at run time (D3-B).
- Local Windows linting (#196).

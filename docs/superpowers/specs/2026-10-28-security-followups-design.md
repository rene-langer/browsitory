# Security Follow-ups — Design

Date: 2026-10-28. Branch: `fix/security-followups` (from `main` 3dabf144).
Plan: `docs/superpowers/plans/2026-10-28-security-followups.md`.
Context: `docs/superpowers/specs/2026-10-28-security-bug-wave-design.md` (#114/#138) and
`docs/ARCHITECTURE.md` "Worktree writes and symlinks".

| # | Kind | Summary |
|---|---|---|
| #156 | security | Case-insensitive file systems: a tree with a link `D -> /outside` and a file `d/x` might make a checkout write `d/x` through `D` (CVE-2021-21300 class) |
| #149 | security, low | `write_beneath`, undo's raw reads/writes and `clear_hazards` check a path and then act on it by path; a local process can swap in a link in between |
| #150 | security, low | Windows `open_beneath` follows links in the middle of the path and only checks the final path; non-symlink reparse leaves (OneDrive placeholders) are opened as the reparse point |
| #131 | security, low | Runnable-file refusal gaps in `launch.rs`: missing extensions, trailing dot/space, `[Desktop Entry]` sniff variants |

The p4merge add/add BASE item from #131 is not a security issue and not trivial (it needs git's
"virtual base": LOCAL minus the lines REMOTE lacks). It moved to its own issue, **#159**.

Decisions that need the user are in [Product decisions](#product-decisions). The plan
implements the **recommended** option of each. A different answer changes only the tasks
listed under the plan's "Decision hooks".

---

## 1. #156: case-colliding link and folder

### 1.1 What libgit2 1.9.6 already does

Read from the vendored source (`libgit2-sys 0.18.7+1.9.6`, `src/libgit2/checkout.c`,
`repository.c`, `path.c`, `util/futils.c`, `util/win32/*`):

| Protection | Where | What it covers | What it does not |
|---|---|---|---|
| **Links are written last.** `checkout_create_the_new` makes two passes over the deltas: every non-link blob first, then every link. | every checkout: tree, index, head, so also clone, merge, cherry-pick, rebase, stash apply, submodule update | Within *one* checkout, `d/x` is written before `D` exists, so it can't go through `D`. When `D` is created afterwards it meets the real folder `d` (see next row). | A link that is **already** in the work tree before the checkout starts, for example one left by an earlier operation. |
| **`core.ignorecase` ⇒ remove what's in the way.** `mkpath2file` uses `GIT_MKDIR_REMOVE_FILES \| GIT_MKDIR_REMOVE_SYMLINKS` and `git_futils_rmdir_r`s an existing leaf of another case, unless `GIT_CHECKOUT_DONT_REMOVE_EXISTING`. | every checkout in a repo with `core.ignorecase=true` | Creating the link `D` after `d/x` removes the real folder `d` (with `x`) and then makes the link. Nothing goes outside, but the checkout silently drops `d/x`. A link met while *creating parent folders* is unlinked rather than followed. | Forced checkouts. The Linux probe below shows a forced checkout under `ignorecase=true` **deleting** an outside file through the link. Repos where `core.ignorecase` is wrong (a repo copied between file systems, or the setting changed by hand). |
| **No delayed checkout.** libgit2 has no long-running process filters (`filter.<driver>.process`, Git LFS delay). | all | CVE-2021-21300 needed git's *delayed* checkout to write `A/x` after the link `a` had replaced the folder. libgit2 never writes a file after the links. | n/a |
| **`core.ignorecase` probed at init/clone.** `repo_init_fs_configs` tests whether `<gitdir>/CoNfIg` exists and sets `core.ignorecase=true` if so. | `Repository::init`, clone, submodule clone | Clones and new repos on APFS/NTFS get `ignorecase=true`. | Repos created elsewhere and moved. |
| **Windows links are opt-in.** `are_symlinks_supported` only enables links when the **global/system** config has `core.symlinks=true` (Git for Windows' installer option) *and* the file system allows it. Otherwise `core.symlinks=false` and links are checked out as small text files. | Windows init/clone | With the Git for Windows default, a clone creates no links at all. | Users who enabled links in Git for Windows, or repos cloned by a git that did. |
| **Path validation** `checkout_verify_paths` with `GIT_PATH_REJECT_WORKDIR_DEFAULTS` | every checkout | `.`/`..`/empty components; `.git` literally; NTFS `.git` aliases (`git~1`, `.git::$INDEX_ALLOCATION`, trailing dots/spaces) on **every** OS (`core.protectNTFS` defaults to true); HFS `.git` aliases (ignorable code points) **always on macOS** (`protectHFS` forced) and elsewhere when set. On Windows also trailing dot/space/colon, DOS device names (`CON`, `NUL`, …), NT-invalid characters and backslashes. | Nothing about case collisions between two ordinary names. |
| **Windows `lstat` treats junctions as links.** `git_win32__file_attribute_to_stat` maps `IO_REPARSE_TAG_SYMLINK` and `IO_REPARSE_TAG_MOUNT_POINT` to `S_IFLNK`. | Windows | The rules above apply to junctions too. | Other name-surrogate tags (WSL `LX_SYMLINK`, …). |

How Browsitory's own guard behaves on a case-insensitive file system: `worktree_guard` decides
with `symlink_metadata`, and the file system answers `lstat("d")` with the link `D`. So a link
**already** in the work tree is found by `refuse_linked_folders`, `prepare_forced_checkout`,
`has_symlinked_ancestor`, `clear_symlinked_ancestors` and `refuse_linked_path` whatever its case.
`tracked_as_link` (the HEAD-tracked-link exemption) compares exact case, so the exemption never
applies across cases. That errs on the side of refusing.

**Analysis.** With libgit2 1.9.6 and the existing guard, no write-through path is known:
- One checkout can't create `D` and then write through it (links come last).
- A `D` that is already there is found by the guard before the next operation.

The remaining unknowns, which only a real case-insensitive file system can answer:
(a) libgit2's work-tree iterator and action planning under `ignorecase` when the tree holds both
`D` and `d/…`; (b) what clone and submodule update (no Browsitory guard at all) leave behind;
(c) Windows with `core.symlinks=true`; (d) `core.ignorecase=false` on a case-insensitive file
system, where `REMOVE_EXISTING` is off.

### 1.2 Linux probe (done)

A throwaway binary linked against `git2 0.21.0` (source kept in the session scratchpad, not
the repo) ran on ext4 (case-sensitive) to check the `ignorecase` mechanism above. An untracked
`d -> outside` (holding `f` and `g`) was set up, then a checkout of a tree adding `d/…` ran.

| Checkout | `core.ignorecase=false` | `core.ignorecase=true` |
|---|---|---|
| safe, adds `d/new` and `d/f` | refused ("1 conflict prevents checkout") | refused |
| safe, adds only `d/new` | refused | refused |
| forced, adds `d/new` and `d/f` | **writes** outside `f` and `new` | link replaced by a folder, but outside `f` **deleted** |
| forced, adds only `d/new` | **writes** outside `new` | link replaced by a folder, outside untouched |
| forced, HEAD has `d/f`, target drops it | outside `f` **deleted** | outside `f` **deleted** |

So `core.ignorecase` is not a protection for forced checkouts. Browsitory's
`prepare_forced_checkout` (which clears links first) is still needed and stays as is.

### 1.3 Probe on macOS and Windows (Task C1)

A permanent test binary, `crates/git-core/tests/case_insensitive_links.rs`:

- **Self-skips** unless the temp directory is case-insensitive (create `probe`, check that
  `PROBE` exists). On the CI matrix it therefore runs on `macos-latest` (APFS) and
  `windows-latest` (NTFS) and is a no-op on `ubuntu-latest`.
- Uses `test_support::hermetic_git!()`. On Windows, libgit2 only enables links from the
  **global** config. All rows therefore run inside **one** `#[test]` function that first points
  `git2::opts::set_search_path(ConfigLevel::Global, …)` at a directory of its own (under
  `target/`, never deleted) whose `.gitconfig` sets `core.symlinks=true`. If a link can't be
  created (no privilege), it prints `SKIP links unsupported` and returns.
- Evil source repo (built with `TreeBuilder`, so the link is a tree entry and no file-system
  link is needed to build it): commit `base` = `a.txt`; commit `evil-upper` = base + link
  `D -> <outside abs path>` + `d/x = "PWNED\n"`; commit `evil-lower` = link `d` + `D/x`;
  commit `evil2` = `evil-upper` with `d/x` changed (the two-step case). An `outside/` TempDir
  holds `x = "OUTSIDE\n"`.
- Rows (each in a fresh repo, for `ignorecase` = as probed **and** forced to `false`, for both
  orders):
  1. raw git2 `RepoBuilder::clone` (default checkout);
  2. raw safe `checkout_tree` base → evil, then evil → `evil2` (two-step);
  3. raw forced `checkout_tree` base → evil, then → `evil2`;
  4. raw `merge_commits` + safe `checkout_index`;
  5. raw `submodule.update` of a submodule whose commit is evil;
  6. git-core (before the fix): `clone::clone_repo` (local path URL), `branch::switch_branch`,
     `checkout_commit`, `merge::start_merge` (fast-forward and merge commit), `pick::cherry_pick`,
     `rebase::start_rebase`, `stash::apply_stash`, `reset::reset_to(Hard)`, `undo::restore`,
     `worktree::create_worktree`, `submodule::update_submodule`, `remote::pull_after_fetch` (fast-forward from a
     local remote).
- Each row records the result (Ok / error text), what is at `D`/`d` afterwards and whether
  `outside/x` is intact. It `eprintln!`s one table line. At the end the test asserts **only**
  "every outside file intact", listing every failing row in the panic message.
- After Task C3/C4 land, git-core rows that are now refused print `refused: CaseCollision`. The
  raw rows stay as a canary for a libgit2 upgrade.

The CI run of this test fills the table below (Task C1, Step 5). **Expected**, from §1.1: every
row intact; raw clone and merge leave `D` as a link and `d/x` missing (dropped by `rmdir_r`);
`ignorecase=false` rows fail the checkout with "file exists".

| Row | macOS ic=true | macOS ic=false | Windows ic=true | Windows ic=false |
|---|---|---|---|---|
| *(to be filled from CI, Task C1 Step 5)* | | | | |

If any row writes outside, Tasks C3/C4 fix it anyway. The table and the CHANGELOG wording then
say "fixed" rather than "hardened".

### 1.4 Fix: refuse case-colliding link/folder entries (defense in depth)

Done either way, per the issue. It doesn't depend on the probe outcome, and it removes the
silent data loss (libgit2 dropping `d/x`).

**Rule.** A source (tree or index) is refused when a **link entry's** path, folded, equals a
**folder** on the way to another entry (a strict prefix of its path), folded. Example:
`D` (link) and `d/x`, `a/B` (link) and `A/b/c`. The same path with exactly the same spelling is
refused too (a crafted tree with `d` as both link and folder). Submodule (gitlink) entries
are checked like links: libgit2 clones into them. File-vs-folder collisions (`D` file, `d/x`)
are **not** refused: no link, so nothing can redirect a write.

**Fold.** One shared `fold_name(&[u8]) -> Vec<u8>`. Valid UTF-8 is lowercased per character
(`char::to_lowercase`), dropping the HFS+ ignorable code points (U+200C–U+200F,
U+202A–U+202E, U+206A–U+206F, U+FEFF, the list git's `is_hfs_dotgit` uses). Anything else is
lowercased as ASCII. NFC/NFD normalization (APFS treats `é` and `é` as one name) is
**not** folded: no new dependency (decision D4). It's documented as a residual risk.
Cross-checkout safety still comes from libgit2's links-last rule and the guard's `lstat`.

**When the rule applies (D1-A).** `core.ignorecase=true`, **or** the build targets macOS or
Windows. Linux with `ignorecase=false` (a case-sensitive file system) is never refused, so
Linux users of the rare repos that hold both `D` and `d/` keep working.

**API** (`worktree_guard`):

```rust
/// A link (or submodule) entry and a folder of another entry name the same file once case is
/// ignored.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("'{link}' is a symbolic link and '{folder}' a folder whose names differ only in case. \
On this file system they are the same name, so Browsitory won't check them out. Rename one of \
them in the repository.")]
pub struct CaseCollision { pub link: String, pub folder: String }

/// Whether case collisions matter for `repo`: `core.ignorecase`, or always on macOS/Windows.
pub(crate) fn folds_case(repo: &git2::Repository) -> bool;

/// The first case collision among `source`'s entries (optionally limited to the literal
/// `paths`, as `scan` does), or `None`. Pure: never touches the work tree.
pub(crate) fn find_case_collision<P: AsRef<[u8]>>(
    source: &GuardSource, paths: Option<&[P]>,
) -> Result<Option<CaseCollision>, git2::Error>;
```

`find_case_collision` makes one pass over the index entries (a tree is read through an
in-memory `Index`, as `Scanner::scan` does) and builds two `HashMap<Vec<u8>, String>`: folded
link/gitlink paths and folded strict-prefix folders. It looks each new key up in the other
map. Cost is O(entries × depth), the same order as the existing scan (a 100k-file repo stays
well under the guard's 0.24 s).

`pub use worktree_guard::CaseCollision;` from `lib.rs`, next to `LinkedFolder`.

**Hook points.**

| Call site | Source checked | Notes |
|---|---|---|
| `refuse_linked_folders` (switch, checkout commit, merge ff/non-ff, squash, pull ff, cherry-pick, revert, sequences, rebase start/steps, stash apply/pop) | each `incoming` source only, **not** HEAD/index | A repo whose HEAD already holds a collision isn't locked: unrelated operations still work. An operation that brings the collision in (again) is refused. |
| `prepare_forced_checkout` (reset hard, discard, undo/redo checkout, abort merge/rebase, resolve whole file) | `target`, limited to `paths` | Path-limited discards only refuse when both colliding entries are in scope. |
| `clone::clone_repo` | HEAD tree of the fresh clone | Clone with `CheckoutBuilder::dry_run()` (`GIT_CHECKOUT_NONE`). Then check `folds_case(&repo)` + `find_case_collision(HEAD tree)`. Then a forced `checkout_head` with the same progress/cancel callbacks (the work tree is empty, so forced is what the clone did anyway). On a collision: `CloneError::CaseCollision`, cleanup as for any failed clone (D2-A). |
| `worktree::create_worktree` | the reference's tree, before `repo.worktree(...)` | git2 0.21 exposes no checkout options for worktree add. The tree is known up front, so check it first. |
| `submodule::update_submodule` and nested updates | the submodule's target commit tree | `SubmoduleUpdateOptions::checkout(CheckoutBuilder::dry_run())` (libgit2 clones with `GIT_CHECKOUT_NONE`, then `set_head_detached`, then `checkout_head(opts)`). Then open the sub-repo, check `folds_case(&sub)` + its HEAD tree. Then `refuse_linked_folders`-style guard and a safe `checkout_head` with `recreate_missing` so a fresh clone is populated and an existing one updated. |

**Errors.** Every enum that wraps `LinkedFolder` gets `#[error(transparent)]
CaseCollision(#[from] crate::CaseCollision)`: `BranchError`, `MergeError`, `PickError`,
`RebaseError`, `StashError`, `RemoteError`, `SubmoduleError`. The forced-checkout enums
(`ResetError`, `StageError`, `UndoError`) and `CloneError` and `WorktreeError` get one too.
`refuse_linked_folders` and `prepare_forced_checkout` gain the bound
`E: From<CaseCollision>`. `repo-service/src/worker/remote.rs` classifies
`RemoteError::CaseCollision` like `LinkedFolder` (its `Display` text reaches the user instead of "pull failed"). Other errors reach the UI as their `Display`
text, so no DTO or frontend change is needed.

**Not changed.** Undo/redo's own raw writes (`write_worktree_file`). Its sorted order writes
`D` before `d/x`, and `clear_symlinked_ancestors` (now race-free, §2) removes the link before
`d/x` is written, so they can't write through. `undo::restore`'s forced `checkout_from_index`
is covered by the `prepare_forced_checkout` hook.

---

## 2. #149: race-free raw work-tree writes (unix)

### 2.1 Today

| Function | Check | Act |
|---|---|---|
| `write_beneath` | `has_symlinked_ancestor` (lstat per component), `is_link(leaf)` | `remove_link`, `create_dir_all(parent)`, `fs::write(full)` by path |
| `remove_beneath` | same + `symlink_metadata(leaf)` | `remove_file(full)` by path |
| `clear_hazards` / `remove_link` | `is_link(full)` | `remove_file(full)` (Windows: `remove_dir`) by path |
| `clear_symlinked_ancestors` | `entry_kind` per component | `remove_link` by path |
| `undo::read_worktree_file` | `has_symlinked_ancestor`, `symlink_metadata` | `fs::read_link` / `fs::read(full)`, which follows and **blocks on a FIFO** |
| `undo::remove_existing` / `prune_empty_parents` | `has_symlinked_ancestor`, `symlink_metadata` | `remove_dir`/`remove_file`/`remove_dir` upwards by path |
| `undo::write_worktree_file` | `clear_symlinked_ancestors` | `remove_*`, `create_dir_all`, `fs::write` / `symlink`, `set_permissions` by path |

A directory on the way that is swapped for a link between check and act redirects the act
outside the repository.

### 2.2 Design: one `openat` walker in `worktree_guard`

All raw work-tree I/O moves into `worktree_guard`, which already owns every "never through a
symlink" rule. A new child module `crates/git-core/src/worktree_guard/beneath_unix.rs`
(`#[cfg(unix)] mod beneath_unix;` in `worktree_guard.rs`; `worktree_guard.rs` stays a file) holds
the primitives on `libc` (already a unix dependency):

```rust
/// How a walk treats a folder on the way.
enum OnLink { Refuse, Clear }      // Clear: unlinkat the link, then continue as OnMissing says
enum OnMissing { Stop, Create }    // Create: mkdirat(0o777 & umask)

/// The open parent folder of `rel`'s last component and that component's name. `None` when a
/// component is a link (Refuse), not a folder, or missing (Stop). Each step is
/// `openat(dir, name, O_RDONLY|O_DIRECTORY|O_NOFOLLOW|O_CLOEXEC)` from the previous
/// descriptor, so a component swapped at any moment fails with ELOOP/ENOTDIR instead of
/// being followed.
fn parent_beneath(workdir: &Path, rel: &Path, on_link: OnLink, on_missing: OnMissing,
                  step: &mut dyn FnMut(Step)) -> io::Result<Option<(Vec<OwnedFd>, OsString)>>;
```

The chain of descriptors (root..parent) is kept so `prune_empty_parents` can `unlinkat(…,
AT_REMOVEDIR)` upwards without resolving a path again. `step` is a test seam: production
passes a no-op, and unit tests swap a folder for a link at a chosen step. That makes the race
deterministic (see §2.4). Small wrappers: `stat_at(fd, name)` (`fstatat` with
`AT_SYMLINK_NOFOLLOW`), `unlink_at(fd, name, dir: bool)`, `readlink_at`, `symlink_at`,
`open_at(fd, name, flags, mode)`. Each `unsafe` block has a `SAFETY:` comment like
`open_beneath`'s.

Rebuilt on it (same signatures, same return contracts, so call sites don't change):

| Function | New act |
|---|---|
| `open_beneath` | `parent_beneath(Refuse, Stop)` + `openat(O_RDONLY\|O_NOFOLLOW\|O_NONBLOCK)` + `fstat` is regular. (Same behavior, shared code.) |
| `write_beneath` | `parent_beneath(Refuse, Create)`. Leaf: `openat(O_WRONLY\|O_CREAT\|O_TRUNC\|O_NOFOLLOW\|O_NONBLOCK, 0o666)`. On `ELOOP` (leaf is a link): `unlinkat` it and retry with `O_CREAT\|O_EXCL\|O_NOFOLLOW`. `fstat` must be a regular file (a FIFO/device is an error, never written). Write through the descriptor. Overwriting a regular file in place keeps its mode, like `fs::write` did. |
| `remove_beneath` | `parent_beneath(Refuse, Stop)`. Leaf: `stat_at`, where a folder gives `Ok(false)`. Otherwise `unlinkat(fd, name, 0)`. `EISDIR`/`EPERM` from a leaf swapped to a folder in between gives `Ok(false)`. `ENOENT` gives `Ok(true)`. |
| `clear_hazards` / `remove_link` | `parent_beneath(Clear, Stop)` to the hazard's parent: a folder on the way that *became* a link is itself removed (it would be a hazard too) and the walk stops. Leaf: `stat_at` is a link, then `unlinkat(fd, name, 0)`, which removes the link and never its target. If a folder appeared in between, `unlinkat` fails with `EISDIR` and is left alone. |
| `clear_symlinked_ancestors` | `parent_beneath(Clear, Stop)`: the first link on the way is unlinked through its parent's descriptor. |
| new `read_beneath(workdir, rel) -> io::Result<Option<Leaf>>` (`Leaf::File { bytes, executable }` / `Leaf::Link(target_bytes)`), used by `undo::read_worktree_file` | `parent_beneath(Refuse, Stop)`. `stat_at` tells a link (`readlinkat`), a folder (`None`) or other. Other: `openat(O_RDONLY\|O_NOFOLLOW\|O_NONBLOCK)` + `fstat` on the descriptor (regular, else `None`, so a FIFO no longer hangs the snapshot), exec bit from that same `fstat`. `read_worktree_file` keeps its mode logic and only swaps its I/O. |
| new `remove_and_prune_beneath(workdir, rel)` for `undo::remove_existing` + `prune_empty_parents` | `parent_beneath(Refuse, Stop)`. Leaf: folder gets `unlinkat(AT_REMOVEDIR)` (empty only, as `remove_dir`), else `unlinkat(0)`. Then walk the descriptor chain upwards with `unlinkat(parent, name, AT_REMOVEDIR)` until one fails or the work tree root is reached. |
| new `replace_beneath(workdir, rel, Content)` (`Content::File { bytes, executable }` / `Content::Link(target)`) for `undo::write_worktree_file` | `parent_beneath(Clear, Create)` (undo clears a linked folder on the way, as today). Leaf: `stat_at`, folder gets `unlinkat(AT_REMOVEDIR)`, other gets `unlinkat(0)`. Then `symlinkat` or `openat(O_WRONLY\|O_CREAT\|O_EXCL\|O_NOFOLLOW, 0o666)` + write + `fchmod` (exec bits added/removed on the descriptor, from its `fstat` mode, as `set_executable` did). |

**Not race-free and documented** (unchanged residual): libgit2's own checkout writes (no
`openat` API), and the refusal checks before libgit2 operations (`has_symlinked_ancestor` in
stage/stash, `refuse_linked_folders`, `scan`). The residual-risk paragraph in ARCHITECTURE
shrinks to those.

### 2.3 Windows counterpart

Same functions, same contracts, on the Windows walker of §3 (pinned handles). Delete and
create act by full path under pinned parents. `DeleteFileW`/`RemoveDirectoryW` never follow a
final reparse point, and every folder on the path is held open without `FILE_SHARE_DELETE`,
so it can't be renamed or replaced. Undo still writes a link's target as a plain file on
Windows (unchanged).

### 2.4 Tests

- Unit tests in `worktree_guard` (unix): every existing test keeps passing. **New, deterministic
  race tests** through the `step` seam:
  - for `write_beneath`, after `d` is opened, rename it away and put a link to outside in its
    place: the write lands in the renamed folder, and outside is untouched;
  - the same for `remove_beneath`, `read_beneath` and `replace_beneath`;
  - for `clear_hazards`, a parent turned link mid-walk is removed, never followed;
  - `write_beneath` on a FIFO leaf returns an error without blocking;
  - `write_beneath` over an executable file keeps the exec bit;
  - `read_beneath` on a FIFO returns `None` without blocking.
- `#[ignore]` stress test like `directory_swap_race_never_reads_outside`: `write_beneath` in a
  loop while a thread swaps `d`, asserting the outside file never changes. Run manually and
  record the numbers in the plan's pre-flight notes.
- `tests/undo.rs` and `tests/symlink_write_through.rs` unchanged and green.

---

## 3. #150: Windows `open_beneath` walks components

### 3.1 Today

`open_beneath` opens `workdir.join(rel)` once with `FILE_FLAG_OPEN_REPARSE_POINT` (only the
**final** component isn't followed). It then checks `GetFinalPathNameByHandleW` lies under the
canonical work tree. That causes three problems:
- A junction in the middle of the path to an inside folder is allowed.
- A junction or symlink to `\\server\share` is contacted (SMB authentication, NTLM hash leak)
  before the refusal.
- Any reparse leaf (OneDrive/Cloud Files placeholder, dedup) is opened as the reparse point.

### 3.2 Design (D3-A: pinned walk, no new dependency)

Win32 has no `openat`. The kernel's `NtCreateFile` with `RootDirectory` does, and std's
`remove_dir_all` uses it. But hand-declaring `NtCreateFile`, `OBJECT_ATTRIBUTES`,
`UNICODE_STRING` and `IO_STATUS_BLOCK` correctly is the riskiest FFI in the crate. Instead:

- Walk `workdir` and each component **by full path** with std's `OpenOptions`:
  `access_mode(FILE_READ_ATTRIBUTES | SYNCHRONIZE)` (plus `FILE_LIST_DIRECTORY` for
  folders), `share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)` (**no `FILE_SHARE_DELETE`**),
  `custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)`.
- On each handle, `GetFileInformationByHandleEx(FileAttributeTagInfo)` (one more hand-declared
  `kernel32` function next to `GetFinalPathNameByHandleW`; the struct is two `u32`s):
  - not a reparse point, and a folder for every non-leaf component: continue;
  - a **name-surrogate** reparse point (`tag & 0x2000_0000`: symlink, junction, WSL link,
    …): refuse (`None` for reads, `Ok(false)` for writes, a link to clear for
    `clear_hazards`). The handle was opened *as* the reparse point, so its target, including
    a UNC share, is never contacted;
  - another reparse point (Cloud Files, dedup, …): a folder continues (the file system filter
    resolves it); a **leaf** is reopened by full path **without**
    `FILE_FLAG_OPEN_REPARSE_POINT`, so the placeholder hydrates. The reopened handle's
    `FileIdInfo` (same function) must equal the first handle's, and the first handle is still
    held without `FILE_SHARE_DELETE`, so the name can't have been swapped in between.
- **Why this is race-free:** a folder opened without `FILE_SHARE_DELETE` can't be renamed,
  deleted or replaced while the handle is open (rename and delete need `DELETE` access, which
  the share mode denies). So once components `0..k` are held, the full path to component `k+1`
  resolves through exactly those folders. A process that already holds `DELETE` access makes
  our open fail with a sharing violation, which fails closed.
- `GetFinalPathNameByHandleW` stays as a final assertion for reads (cheap, belt and braces).

Cost: one handle per component, all closed when the walk ends. Other processes can't rename
those folders for the few milliseconds the walk takes.

The same walker backs `write_beneath`, `remove_beneath`, `clear_hazards`, `read_beneath`,
`remove_and_prune_beneath` and `replace_beneath` on Windows (§2.3).

### 3.3 Tests (run on `windows-latest` CI only, `#[cfg(windows)]`)

- Junctions need no privilege. A test helper creates them with
  `Command::new("cmd").args(["/C", "mklink", "/J", link, target])`. Symlink tests try
  `std::os::windows::fs::symlink_dir`/`symlink_file` and return early (printing `SKIP`) on
  `ERROR_PRIVILEGE_NOT_HELD`.
- `open_beneath_refuses_a_junction_to_an_inside_folder` (**fails before**, passes after),
  `…_to_an_outside_folder`, `…_a_file_symlink_leaf`, plus the existing portable tests.
- `write_beneath`/`remove_beneath`/`clear_hazards` with a junction ancestor: outside untouched,
  junction (not target) removed by `clear_hazards`.
- `is_name_surrogate(tag)` and the leaf decision are pure functions with a unit test that runs
  on every platform (no FFI): symlink `0xA000000C`, junction `0xA0000003`, LX symlink
  `0xA000001D` are surrogates; Cloud Files `0x9000001A`, dedup `0x80000013` and AppExecLink
  `0x8000001B` are not.
- Not testable in CI: a real OneDrive placeholder. That's a manual check, listed in the plan.

The dev box has no mingw. Windows code is compile- and clippy-checked against
`x86_64-pc-windows-gnu` with git2 stubbed out, as in the previous wave. CI is the real check.

---

## 4. #131: runnable-file refusal gaps

All in `crates/git-core/src/launch.rs`. Unit tests in its existing `mod tests`.

- **Extensions.** Add `wsc`, `psc1`, `ps1xml`, `mht`, `mhtml`, `hlp`, and the disk images `iso`,
  `img`, `vhd`, `vhdx`, `dmg` (macOS mounts `.dmg` on open, like Windows mounts `.iso`/`.vhd`).
- **Trailing dot or space.** `lower_extension` first trims trailing `.` and ` ` from the
  file name (Windows strips them on open), on every OS. So `x.exe.`, `x.bat ` and `x.cmd. .`
  have the extensions `exe`, `bat` and `cmd`.
- **`[Desktop Entry]` sniff.**
  - Trim ASCII whitespace **plus** vertical tab (`\x0b`, which GLib's `g_ascii_isspace` also
    skips).
  - Accept `[Desktop Entry]`, `[KDE Desktop Entry]` and any `[Desktop Action …]` as the first
    group.
  - Read up to 64 KiB for the sniff (`read_head` keeps 1 KiB for the magic numbers; the
    desktop sniff reads on only when the first 1 KiB held nothing but blank lines and
    comments).
  - If the cap is reached and the header hasn't been decided, the file counts as runnable
    (fail closed). EOF without a group header counts as not runnable.
  - A header *after* a non-comment line still doesn't count (shared-mime-info and the
    existing `# Notes\n[Desktop Entry] is a header` test).

---

## Product decisions

**D1: When to refuse case-colliding link/folder entries (#156).**
- A. When `core.ignorecase=true`, **or always on macOS/Windows builds**. This covers a repo
  whose `ignorecase` is wrong on a case-insensitive file system (copied repo, edited config).
  Linux on a case-sensitive file system is never affected. **Recommended.**
- B. Only when `core.ignorecase=true`: follows git's own model exactly, but trusts a setting
  that can be stale.
- C. Always, on every platform: simplest and catches Linux case-insensitive mounts with a wrong
  setting (ext4 casefold, CIFS), but Linux users of a legitimate repo holding `D` and `d/`
  can no longer switch to or check out those commits.

**D2: Clone when the fresh tree has a collision (#156).**
- A. Fail the clone with the `CaseCollision` message and clean up as for any failed clone.
  Simple and nothing half-done is left. **Recommended.**
- B. Keep the clone but leave the work tree unchecked-out (`git clone --no-checkout` state),
  and report the message as a warning. The user can still inspect history, but the repo opens
  with every file shown as deleted, which is confusing.
- C. Don't check clone at all and rely on libgit2's links-last ordering (the probe decides).

**D3: Windows walk for #150 and #149 (maintenance trade-off).**
- A. **Pinned walk**: open each component by full path with `FILE_FLAG_OPEN_REPARSE_POINT` and
  no `FILE_SHARE_DELETE` (std `OpenOptionsExt`), plus **one** more hand-declared function
  (`GetFileInformationByHandleEx`, two `info` classes with plain `u32`/`u64` structs). No new
  dependency, minimal unsafe surface. **Recommended.**
- B. `windows-sys` 0.61 (MIT OR Apache-2.0, already in `Cargo.lock` transitively) as a direct
  `cfg(windows)` dependency of `git-core`, with true `openat` semantics through `NtCreateFile` +
  `RootDirectory` and delete by handle (`SetFileInformationByHandle`, POSIX semantics). It
  has typed, generated bindings, and needs a `LICENSE_COMPLIANCE.md` row and feature flags
  (`Wdk_Storage_FileSystem`, `Win32_Storage_FileSystem`, `Win32_System_IO`,
  `Win32_Foundation`). It's the most robust, at the cost of one more direct dependency to
  track.
- C. The same `NtCreateFile` design with hand-written declarations. No dependency, but about
  eight hand-declared functions and structs whose layouts must match `ntdef.h` exactly. This
  has the highest review cost and is easy to get subtly wrong.

**D4: Unicode folding for the case check (#156).**
- A. Unicode lowercase plus the HFS+ ignorable code points, no normalization. No dependency.
  NFC/NFD collisions are a documented residual. **Recommended** (the check is defense in
  depth on top of libgit2's ordering and the `lstat`-based guard).
- B. Also NFC-normalize via `unicode-normalization` (MIT/Apache): a new direct dependency
  plus a license row.
- C. ASCII-only folding (like `selected`): misses `Ä`/`ä` on APFS and NTFS.

**D5: How the macOS/Windows probe runs in CI (#156).**
- A. A **permanent** self-skipping test binary (`tests/case_insensitive_links.rs`), run by
  the existing `rust` matrix once a **draft PR** is open (CI runs on `pull_request`). One
  temporary extra step prints its table (`cargo test -p git-core --test
  case_insensitive_links -- --nocapture`), removed in the final task. **Recommended**: the
  probe becomes the regression test and nothing else in CI changes.
- B. A temporary workflow `.github/workflows/probe-156.yml` (`on: push` to this branch, or
  `workflow_dispatch`) running an `#[ignore]`d probe, deleted before merge. The probe is
  thrown away afterwards.
- C. The user runs the probe binary by hand on a Mac and a Windows machine.

---

## Testing strategy

- Real temp-dir repos only. Every **new** test binary starts with
  `test_support::hermetic_git!()` (via `tests/common/mod.rs`, which already has it, or
  directly).
- Case-collision unit and integration tests build trees with `TreeBuilder` link entries
  (mode `0o120000`), so no file-system link is needed. They run on Linux with
  `core.ignorecase=true` set in the repo config, and on every CI OS. Clone and submodule tests
  that can't preset the new repo's config live as unit tests in `clone.rs`/`submodule.rs` and
  pass an internal `CasePolicy::Always`.
- Symlink/FIFO tests stay `#[cfg(unix)]`. Junction tests are `#[cfg(windows)]` and create
  junctions with `mklink /J`.
- The "outside" target is always a second `TempDir`.
- Race tests are deterministic through the walker's `step` seam. Stress tests are
  `#[ignore]`.

## Out of scope

- libgit2's own checkout writes racing a local process (no `openat`-style checkout in
  libgit2), and the check-then-libgit2 refusals.
- NFC/NFD collisions (unless D4-B).
- Case-folding the HEAD-tracked-link exemption (`tracked_as_link`). A case-only rename of a
  link into a folder is refused, which is conservative.
- p4merge virtual base (#159).
- Hard links to outside files (unchanged residual).

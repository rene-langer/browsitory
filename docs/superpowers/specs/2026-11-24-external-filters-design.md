# External filter drivers (#1063): design

Status: decided 2026-11-24 (refuse; see "Decision"). Bundle b61.

## Problem

libgit2 runs only its built-in filters (CRLF/eol, `ident`). A `.gitattributes` `filter=<name>`
whose driver is configured (`filter.<name>.clean`, `.smudge`, `.process`, or `.required`) is
ignored, so Browsitory moves raw bytes where git would run the driver:

- **Clean direction** (work tree → object database): Stage, Stage all, staging hunks or lines,
  Mark resolved, Stash. With git-crypt unlocked, the work tree holds plaintext: Browsitory
  commits the plaintext blob, and Push publishes the secret. With Git LFS, the full binary is
  committed instead of the pointer.
- **Smudge direction** (object database → work tree): Switch, Checkout commit, Reset (hard),
  Discard, Restore version, Pull, Merge, Cherry-pick, Revert, Rebase, Stash apply, patch apply,
  Undo restores. With LFS the pointer text overwrites the real file; with git-crypt the
  ciphertext overwrites the plaintext.

Nothing in git-core reads `filter.*` today.

## Options

1. **Refuse** the operation, before anything is written, when it would pass a path with an
   external driver through a filter. Clear error naming the path and the driver; the user does
   that change with the git CLI.
2. **Run the drivers**: implement clean/smudge (`sh -c` with `%f`) and the long-running
   `process` protocol (pkt-line, LFS's `filter-process`). git2 0.21 / libgit2-sys don't expose
   `git_filter_register`, so this needs hand-written FFI for libgit2's filter struct, plus
   timeouts, cancel, Windows `sh.exe`, and status/diff running the clean filter on every stat-dirty
   file (as git does). LFS support is "not planned" (issue text).
3. **Route** the affected operations through the git CLI. Browsitory has no dependency on a
   git binary outside hooks on Windows (`sh.exe`); every operation listed above has
   Browsitory-specific guards (links, case pairs, hard links, held files, rollback) the CLI
   wouldn't share.
4. **Banner only** on open ("this repository uses the X filter"). Warns, doesn't prevent the
   leak.

## Decision

Option 1. It is the only one that never writes plaintext where a filter would have encrypted,
needs no new dependency, and changes nothing for repositories without a configured driver (the
common case: one config scan, no attribute lookups). Running drivers (2) or routing (3) is a
feature for a later issue; refusing now doesn't block it. A banner (4) can follow as UI polish.

## Which paths count

`external_filter::Drivers::load(repo)` reads the configuration snapshot (all levels, so a global
`git lfs install` counts) and collects every driver name `<name>` with a non-empty
`filter.<name>.clean`, `.smudge` or `.process` (last value wins, as in git), or
`filter.<name>.required` true (git fails without a command then; an unreadable boolean counts as
true). A driver name is the config subsection, case-sensitive.

A path uses a driver when its `filter` attribute (`get_attr`, `FILE_THEN_INDEX`: the work tree's
`.gitattributes` first, as `git add` reads them) is a string naming one of those drivers. A
`filter=<name>` without any driver config is ignored, as git ignores it. Symbolic links are never
filtered (git doesn't filter them), so staging a link stays allowed.

## Where the refusal happens

New error type `git_core::ExternalFilter { path, driver }` ("'secret.env' uses the 'crypt'
filter, which Browsitory can't run. Nothing was changed. Use git on the command line for this
change."), carried as a transparent variant by each operation's error enum, like `CaseCollision`.

### Checkouts (smudge direction), one choke point

Every libgit2 checkout Browsitory starts installs its callbacks through `hardlink::install`
(`hardlink::on_checkout` or `partial_checkout::Written::record`). Its notify callback already
sees each file libgit2 plans to write (`UPDATED`), while planning and before anything is written.
It now also looks the path up and, for a filtered one, records the refusal and returns `false`.
libgit2 then stops planning and writes nothing (no file, no index), but returns a positive
code, which git2 reports as `Ok(())` (checked: a checkout aborted this way returns `Ok`, the file
and index unchanged). So `install` returns a `FilterGuard`, and every call site checks it right
after the libgit2 call (`guard.check()?`), before moving HEAD or anything else.

libgit2's compound operations go on after an aborted checkout: `git_stash_apply` runs a second
checkout and writes the index, `git_cherrypick`/`git_revert` leave their state files. These
call sites get an explicit pre-check of the paths they will write before calling libgit2, and
the guard check stays as a backstop that goes through the call site's existing failure path
(rollback, `clear_commit_state`):

- cherry-pick/revert step and rebase step: the paths where libgit2's in-memory result differs
  from HEAD's tree;
- stash apply: the paths the stash changes (its work-tree, index and untracked trees against its
  base).

Patch apply to the work tree (`repo.apply` with `WorkDir`/`Both`: discard hunk/lines, apply
patch) takes no checkout options: the diff's paths are pre-checked.

### Clean direction

- `stage_file` (and so `stage_paths`, Stage all, the untracked-folder route): before
  `index.add_path` of a regular file.
- `stage_hunk`/`stage_lines`: the path, before the apply to the index.
- Mark resolved, Resolve with ours/theirs, Resolve add/delete, resolving hunks in the conflict
  view: the path, before anything is written.
- Stash save: the changed paths (`WorktreeScan::changed_paths`, or the requested paths),
  before either route. This also covers `stash_save2`'s internal reset, which takes no callbacks.

### Left alone

- Status and diffs still compare raw work-tree bytes (a git-crypt file can show as modified);
  display only.
- Side files and the VSCode content provider already document that drivers don't run.
- A fresh clone and a new worktree write into an empty folder: nothing of the user's is
  overwritten (LFS pointers / ciphertext, as git without the driver installed).
- Undo snapshots keep raw work-tree bytes internally, never in a ref that is pushed.
- Unstage and the other index-only operations move existing blobs; no filter is involved.

## Residual risk

- The attribute is read from the current work tree and index. A checkout that itself brings in a
  `.gitattributes` adding a filter writes the files it newly covers unfiltered; they are new
  files (nothing of the user's is overwritten), as with git when the driver isn't installed.
- A driver configured only through `GIT_CONFIG_PARAMETERS` or similar environment variables is
  seen only if libgit2 reads it.

## #802

#802 is about which built-in clean filters apply to a link kept as a file when
`core.symlinks` is off. Refusing external drivers doesn't change it; it stays open.

## Testing

`crates/git-core/tests/external_filter.rs`: real temp repositories with
`.gitattributes` `secret.env filter=crypt` and `filter.crypt.clean`/`.smudge` set in the
repository's config to a command that would fail if run. Refusal of each user-facing operation
(error names path and driver; HEAD, index, work tree and stash list unchanged), and the
unaffected cases (no driver config, other paths, the filtered file unchanged by a switch, a
symbolic link).

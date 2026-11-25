# External filter notice (#1087): design

Status: decided 2026-11-24 (owner: "do it"). Bundle b68. Follows #1063 (PR #1086, design
`2026-11-24-external-filters-design.md`), whose refusals stay as they are.

## Problem

Since #1086 every write of a file under a configured filter driver (git-crypt, Git LFS) is
refused, but two display gaps remain:

- Status compares raw work-tree bytes with the stored (cleaned) blob. A file whose stat data is
  stale (touched, written by another tool) shows as modified although `git status`, which runs
  the clean filter, finds it unchanged. Its row invites Stage, which is refused.
- The user learns that a repository uses a driver only from a refusal.

The owner asked for a notice when the repository uses such a driver, and for files to not show
as changed when only the filter explains the difference. Never hide a real change.

## When does only the filter explain the difference?

git decides by running the clean filter on the work-tree file and comparing the result with
the index blob. Browsitory doesn't run drivers (a status refresh would then run arbitrary
programs on every stat-dirty file, the thing #1063 decided against). Without running the driver:

- **Git LFS: decidable.** The clean output of `git-lfs` is a pointer that is a function of the
  content: `version https://git-lfs.github.com/spec/v1\noid sha256:<hex>\nsize <n>\n`. git runs
  the driver first, then the CRLF and `ident` conversions, which leave a pointer unchanged. So
  when the index blob is exactly that canonical pointer and the work-tree file has `n` bytes
  whose SHA-256 is `<hex>`, the clean filter would produce the index blob: the file is
  unchanged. This is only trusted for a driver whose effective clean command (`process` if set,
  else `clean`) is `git-lfs` (its first word's file name `git-lfs` or `git-lfs.exe`, as
  `git lfs install` writes); any other driver could clean differently. Pointers with extension
  lines or another layout aren't matched (the file stays listed). Cost: only for an unstaged
  modified file under such a driver whose index blob is a pointer of the file's size, one
  streaming SHA-256 of the file, about what libgit2 already spends hashing it with SHA-1.
- **Anything else (git-crypt, custom drivers): not decidable** without the driver (git-crypt's
  blob is keyed ciphertext). The file stays listed, with a marker saying Browsitory can't
  compare it.

So: LFS files that only differ by the filter are left out of the list; every other changed file
under a driver is listed and marked. A file is never hidden on a guess.

## Backend

- `StatusEntry.external_filter: Option<String>`: the driver's name on an unstaged entry other
  than a deletion (new, modified, type change, conflict, not an untracked-folder entry) whose
  work-tree file is not a symbolic link and whose `filter` attribute names a configured driver
  (`Drivers::refusal`, the same lookup the refusals use). Staged entries compare two stored
  blobs and a deletion reads no file: no marker. Set, and the LFS rule applied, by
  `external_filter::annotate` in the app's status reads: `status_refreshing_index`,
  `change_stamp::status_with_stamp`, `untracked_folder_files`. `status::status` (the write
  commands' internal read) stays raw. Any error on the way (config, attributes, index, reading
  the file) leaves the entry listed and, where the lookup failed, unmarked.
- `HeadInfo.external_filters: Vec<String>`: the configured drivers that the repository's
  attribute files name in a `filter=<name>` (sorted, once each). Read on every refresh with the
  rest of `HeadInfo`; without a configured driver nothing beyond the config read happens.
  Sources: every `.gitattributes` the index tracks (the work-tree file, else the index blob, as
  `FILE_THEN_INDEX`), the work tree's top-level `.gitattributes` even if untracked,
  `info/attributes`, and `core.attributesFile` when set. A global `git lfs install` alone
  therefore shows no notice in a repository that tracks nothing with LFS. A line's first word is
  its pattern (or `[attr]name`); the words after it are scanned for `filter=<name>`.
- Wire: `StatusEntry.externalFilter?: string` and `HeadInfo.externalFilters?: string[]`, each
  left out when empty (as `untrackedFolder` and `unbornBranch`), so existing snapshots don't
  change; new wire fixtures pin both.

## Frontend

- Changes pane: when `headInfo.externalFilters` is non-empty, a polite one-row notice at the
  top (the `GoToNotice` pattern: message and ✕, not an error banner, since nothing failed):
  "This repository uses the 'lfs' filter, which Browsitory can't run. It won't stage, discard
  or check out files under it: use git on the command line for those." ✕ hides it until the
  filters change or the pane is opened again.
- File row with `externalFilter`: a muted note "'crypt' filter" (the untracked-folder note's
  style) with a title explaining it, and the Stage toggle disabled with the reason as its title.
- Stage all / Stage modified (button and palette, `lib/stagePaths`) leave such files out, as
  they leave out conflicts. The backend refusal is unchanged for a file staged any other way.
- Viewer: above a marked file's diff, a note: Browsitory can't run the filter, so the diff
  compares the file on disk with the version git stored, and the difference may be only the
  filter's.

## Left alone

- #1086's refusals, Discard (refused per file), the diff itself, `status::status`.
- System-wide attribute files (`$(prefix)/etc/gitattributes`) and the default global file used
  without `core.attributesFile` aren't scanned for the notice; their files are still marked and
  refused.

## Testing

- `crates/git-core/tests/external_filter_status.rs`: real temp repositories; drivers configured
  with commands that fail if run (`false`; for LFS `git-lfs` commands, which the tests never
  run). Marker on modified, new (also inside an untracked folder), conflicted; none on staged,
  deleted, unfiltered, links, without driver config. LFS: a stale file whose content matches the
  pointer is left out of both app reads; a different content, a size mismatch, a non-canonical
  pointer, a non-LFS driver with a matching pointer each stay listed and marked. `head_info`:
  drivers named in `.gitattributes`, in a nested one, in `info/attributes`; none when only
  configured, none when named but not configured.
- Wire fixtures for both fields (Tauri and sidecar snapshots).
- Frontend (Vitest, `RepoClient` mocked): notice shown, dismissed; row marker and disabled
  Stage; Stage all leaves marked files out; the viewer note.

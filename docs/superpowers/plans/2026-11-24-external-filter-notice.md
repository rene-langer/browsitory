# External filter notice (#1087): plan

Design: `docs/superpowers/specs/2026-11-24-external-filter-notice-design.md`. Branch
`b68-external-filter-notice`.

## Commit 1: scaffold (no behaviour)

So the tests compile against the new fields: `StatusEntry.external_filter` (always `None`),
`HeadInfo.external_filters` (always empty), the wire fields (`externalFilter?`,
`externalFilters?`, skipped when empty), the regenerated `wire-dto.ts`, the hand-written
`RepoClient.ts` types. Existing tests and snapshots stay green.

## Commit 2: Rust tests (red)

`crates/git-core/tests/external_filter_status.rs`, fixture as `external_filter.rs`
(`secret.env filter=crypt`, `filter.crypt.clean/smudge = false`), plus an LFS fixture
(`*.bin filter=lfs`, `filter.lfs.process = git-lfs filter-process`, `filter.lfs.clean =
git-lfs clean -- %f`, `filter.lfs.required = true`; never run), `big.bin` committed as the
canonical pointer of `hello lfs\n` (sha256 `4859402c…7bba`, size 10), then the real content
written to the work tree.

Expected (each read through both `status_refreshing_index` and `status_with_stamp` where it
concerns the list):

1. Edited `secret.env`: one unstaged Modified entry, `external_filter == Some("crypt")`.
2. Untracked `new/a.env` (`new/** filter=crypt`): New entry marked `crypt`;
   `untracked_folder_files("new/")` too when listed as a folder.
3. Staged edit of `secret.env` (staged before the driver is configured): the staged entry
   unmarked.
4. Deleted `secret.env`: unmarked.
5. Edited `plain.txt`: unmarked.
6. No driver config: edited `secret.env` unmarked.
7. Unix: a symbolic link `link` named by the filter: unmarked.
8. Conflict on `secret.env` (made before the driver is configured): marked.
9. LFS stale file with the pointer's content: not listed.
10. LFS file with other content of the same size: listed, Modified, marked `lfs`.
11. LFS file with other size: listed, marked.
12. Pointer with an extension line before `oid`: listed, marked.
13. Matching pointer but the driver's command isn't git-lfs (`filter.lfs.clean = other`):
    listed, marked.
14. `head_info(repo).external_filters`: `["crypt"]` for the fixture with driver config; `[]`
    without driver config; `[]` when only a driver is configured but no attribute names it;
    `["crypt"]` from `.git/info/attributes` alone; `["lfs"]` from a nested `sub/.gitattributes`
    alone; macro line `[attr]secret filter=crypt` counts.

Wire: `crates/test-support/wire_fixtures.rs` gets `StatusEntry/external-filter` and a
`HeadInfo` case with `externalFilters`; both snapshot files updated by hand (they pass after the
scaffold, so they go in commit 1's snapshot update, not as red tests).

Commit message quotes `cargo test -p git-core --test external_filter_status` (red).

## Commit 3: Rust implementation

`external_filter.rs`: `Drivers` keeps which drivers run `git-lfs`; `annotate(repo, entries)`;
`named_drivers(repo)`; `lfs_unchanged` (canonical pointer parse, size check, streaming SHA-256
with `sha2`). `status.rs`/`change_stamp.rs`/`repo.rs` call them. `sha2` added to git-core with a
`docs/LICENSE_COMPLIANCE.md` row (MIT OR Apache-2.0, already in the shipped tree through
`secret-service`). `scripts/check-privacy-hosts.py`: `git-lfs.github.com` on
`NON_NETWORK_HOSTS` (the pointer's version line, never contacted).

## Commit 4: frontend tests (red)

- `lib/stagePaths.test.ts`: `stageAllPaths`/`stageModifiedPaths` leave out entries with
  `externalFilter`.
- `UncommittedDiffPane.test.tsx`: with `headInfo.externalFilters: ["lfs"]` the notice
  ("uses the 'lfs' filter") shows, ✕ hides it; without, none. A row with
  `externalFilter: "crypt"` shows "'crypt' filter" and its Stage button is disabled; the
  viewer shows "can't run the 'crypt' filter" above its diff.

## Commit 5: frontend implementation

`UncommittedFileRow`, `UncommittedDiffPane`, `stagePaths`, CSS.

## Commit 6: docs

`docs/guide/troubleshooting.md` (filter section), `docs/ARCHITECTURE.md` ("External filter
drivers"), `changelog.d/1087-external-filter-notice.md`.

# External filter drivers (#1063): plan

Design: `docs/superpowers/specs/2026-11-24-external-filters-design.md`. Branch
`b61-external-filters`.

## Commit 1: tests (red)

`crates/git-core/tests/external_filter.rs`. Fixture: `init_repo`, `.gitattributes`
`secret.env filter=crypt`, `secret.env` and `plain.txt` committed on `main`; then
`filter.crypt.clean`/`filter.crypt.smudge` set in the repository config to `false` (a command
that fails, never run by these tests). Every refusal asserts: the call fails, its message names
`'secret.env'` and `'crypt'`, and nothing changed (HEAD, the index file's bytes, `secret.env`'s
work-tree bytes, the stash list, `repo.state()` as applicable).

Refused (all succeed today, so all fail red):

1. `stage_file(secret.env)` after editing it.
2. `stage_paths([plain.txt, secret.env])`: refused, `plain.txt` not staged either.
3. `stage_file("new/")` of an untracked folder holding a filtered file
   (`.gitattributes` `new/** filter=crypt`).
4. `stage_hunk` on the edited `secret.env`.
5. `stage_lines` on the edited `secret.env`.
6. `save_stash` (everything) with `secret.env` edited.
7. `save_stash` with `paths: [secret.env]`.
8. `switch_branch(other)` where `other` changes `secret.env`.
9. `checkout_commit` of a commit that changes `secret.env`.
10. `reset_to(older, Hard)` where `secret.env` differs.
11. `discard_file(secret.env)` after editing it.
12. `discard_hunk` on the edited `secret.env`.
13. `restore_file_version(older, secret.env)`.
14. `cherry_pick` of a commit that changes `secret.env`; also no `CHERRY_PICK_HEAD`.
15. `revert` of a commit that changes `secret.env`.
16. `start_merge(other)` fast-forward and a real merge, each changing `secret.env`.
17. `start_rebase` replaying a commit that changes `secret.env`.
18. `apply_stash` of a stash (made before the driver was configured) holding `secret.env`.
19. `apply_patch` of a patch changing `secret.env`.
20. `mark_resolved` and `resolve_whole_file` of a conflict on `secret.env` (conflict made before
    the driver is configured).
21. `filter.crypt.required=true` alone (no command) is refused as well.

Unaffected (pass today and after):

22. With the driver configured: staging, switching and discarding `plain.txt`; a switch to a
    branch where `secret.env` is the same.
23. `filter=crypt` with no `filter.crypt.*` config: staging `secret.env` works.
24. `filter.crypt.clean` set to the empty string (git's no-op) and nothing else: staging works.
25. Unix: a symbolic link named by the filter attribute stages (git doesn't filter links).

Commit message quotes `cargo test -p git-core --test external_filter` (red).

## Commit 2: implementation

- `crates/git-core/src/external_filter.rs`: `Drivers::load`, `Drivers::driver_for`,
  `refuse_paths`, `ExternalFilter` (pub, re-exported from `lib.rs`), `FilterGuard`.
- `hardlink::install` returns `FilterGuard` and refuses in its notify callback; `on_checkout` and
  `Written::record` pass it on. Each call site checks it after the libgit2 call.
- Pre-checks: `stage.rs` (add_path, hunk/line apply, work-tree apply), `merge.rs` (resolve
  paths), `stash.rs` (save, apply), `pick.rs`/`rebase.rs` (merged index vs HEAD),
  `patch.rs` (work-tree apply).
- `ExternalFilter` variant in each error enum touched.

## Commit 3: docs

`docs/ARCHITECTURE.md` (new bullet next to the side-file filter note), `docs/guide/`
(`staging-and-committing.md` / `troubleshooting.md`), `changelog.d/1063-external-filters.md`.

## Checks

`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace`, docs nav/zensical if a guide page changes structure.

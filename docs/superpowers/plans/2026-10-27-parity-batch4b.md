# Feature Parity Batch 4b (Refs and Remotes) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tag badges on graph rows with a tag menu, deleting remote branches, pushing any local branch to any remote branch (with "Set as upstream"), Fetch all remotes with per-remote failures and a persisted Prune toggle, and per-branch ahead/behind counts, in both the Tauri app and the VSCode extension. Also fixes #97.

**Architecture:** `git-core` gains tags in `RefDecorations`, `BranchInfo.upstream/ahead/behind`, and in `remote.rs`: `push_branch` (with `push_current_branch` as a wrapper that now targets the upstream's branch name), `delete_remote_branch`, `fetch_remote(prune)` and `fetch_all`. All of it goes through the existing `push_refs`/fetch callback plumbing. `repo-service` adds three transfer commands and a `TransferEvent::RemoteFailed` event. Tauri and the sidecar expose them, and `RepoClient` gains `fetchAllRemotes`, `pushBranch` and `deleteRemoteBranch`, plus a `prune` argument on `fetchRemote`. The frontend adds `RefBadges`, `PushBranchDialog`, `DeleteRemoteBranchDialog`, a split Fetch button, and branch-tree counts.

**Tech Stack:** Rust (`git2` 0.21 / libgit2 1.9, `thiserror`, `serde` in tauri-app/sidecar), React + TypeScript, Vitest + Testing Library, WebdriverIO (tauri-driver).

**Spec:** `docs/superpowers/specs/2026-10-27-parity-batch4b-design.md`

## Global Constraints

- No new Rust or npm dependency. `docs/LICENSE_COMPLIANCE.md` stays untouched. Icons come from `lucide-react` (already a dependency): `Tag`, `Check`, `ChevronDown`.
- Not undoable: no new arm in `undo::plan_for`. Its `_ => return None` covers the new commands.
- `prune: Option<bool>` / `boolean | null`. `None`/`null` means `git2::FetchPrune::Unspecified`, so libgit2 reads `remote.<name>.prune`, then `fetch.prune`. Pull always passes `None`.
- Operation ids: `fetch-N` (existing), `fetch-all-N`, `push-N` (existing, reused by `push_branch`), `delete-remote-N`.
- Wire methods (Tauri command = sidecar method, camelCase params):
  - `fetch_remote { repoPath, remoteName, prune }` returns the operation id
  - `fetch_all_remotes { repoPath, prune }` returns the operation id
  - `push_branch { repoPath, remoteName, localBranch, remoteBranch, setUpstream }` returns the operation id
  - `delete_remote_branch { repoPath, remoteName, branchName }` returns the operation id
- Wire values: `TransferOperation` gains `FetchAll` and `DeleteRemoteBranch`. The progress phase gains `RemoteFailed`, sent on the `transfer-progress` event / `transferProgress` notification with `errorKind` set and an extra `remoteName` field. `remoteName` is omitted from every other event.
- New response fields on the wire: `GraphCommit.tags: string[]`, `BranchInfo.upstream: string|null`, `BranchInfo.ahead: number|null`, `BranchInfo.behind: number|null`, `RemoteInfo.prune: boolean`. In TypeScript they are declared **optional** (`?`), following `UpstreamInfo.ahead?`.
- localStorage key `fetch-prune:<repoPath>`, values `"on"` / `"off"`. Every access goes through `try`/`catch`.
- UI copy, exactly:
  - Tag badge `title`: `Tag <name>`.
  - Tag menu items:
    - `Push <tag> to <remote>`
    - `Push tag` (disabled, title `No remote configured.`)
    - `Checkout (detached)`
    - `Delete tag…`
  - Tag delete confirm: aria-label `Delete local tag <tag>`, message `Delete local tag <tag>?`, button `Delete tag`.
  - Local branch menu: `Push to…`. Remote branch menu: `Delete <remote>/<branch>…`.
  - Push dialog:
    - aria-label `Push <local>`
    - fields `Remote`, `Remote branch`, checkbox `Set as upstream`
    - buttons `Push`, `Cancel`
  - Delete dialog:
    - aria-label `Delete remote branch`
    - fields (picker mode) `Remote`, `Branch`
    - text `Delete <remote>/<branch> on the remote? This can't be undone.`
    - buttons `Delete remote branch`, `Cancel`
    - load error `Could not list branches for <remote>.`
  - Sync bar:
    - chevron aria-label `More fetch actions`
    - menu items `Fetch all remotes` and `Prune` (checkbox, title `Remove remote-tracking branches that were deleted on the remote`)
    - Push title without upstream: `Push this branch to a remote and set its upstream…`
  - Branch tree counts: `↑<n>` / `↓<m>` (non-zero parts only), aria-label/title `<n> ahead, <m> behind <upstream>`.
  - Palette: `Fetch all remotes`, `Push branch to…`, `Delete remote branch…`.
  - Errors:
    - `Could not fetch from <a>, <b>.` and, when some remote succeeded, the suffix ` The other remotes were fetched.`
    - `Fetch cancelled.`
    - `Delete cancelled.`
    - `The remote refused to delete the branch.`
    - `Deleting the remote branch failed`
- Tests: `git-core`/`repo-service` use real temp-dir repos with bare remotes. The frontend mocks `RepoClient` only.
- Every push touching `crates/`, `frontend/src/` or `e2e/` needs a `CHANGELOG.md` update (pre-push hook). The changelog lands in Task 14, so push only after Task 14.
- E2E caveat: a separate branch is fixing E2E flakiness (#98–#100). The new spec owns all of its state and must not depend on, or disturb, other specs.

## Review Focus

1. **A rejected push of a non-current branch must not offer "Force push (with lease)".** That button force-pushes the *current* branch. Pinned in Task 8 (`offers no force push after a rejected push of another branch`).
2. **A current branch tracking a differently named upstream** (`fx` → `origin/feature-x`). Push and force push must target `feature-x` and must not create `origin/fx`. Pinned in Task 3 (`push_current_branch_targets_the_upstream_branch_name_on_that_remote`, `force_push_with_a_differently_named_upstream_overwrites_that_branch`).
3. **A remote branch is deleted (or published) while its remote folder is open in the tree.** The row must disappear (or appear) without collapsing and re-expanding the folder. Pinned in Task 11 (`reloads open remote folders when refreshGeneration changes`).
4. **The graph selection names a branch that was just deleted, or renamed outside the app** (#97). The refresh must still load, and isolating a branch and then deleting it must not leave an empty graph. Pinned in Task 1 (`graph_log_skips_a_selected_branch_that_no_longer_exists`) and Task 8 (`deleting the only selected branch shows all branches again`).
5. **Fetch all with one dead remote** (a removed bare repo, an expired token). The healthy remotes still update, the refresh still runs, and the banner names only the dead one. Pinned in Task 4 (`fetch_all_records_an_unreachable_remote_and_fetches_the_rest`), Task 5 (`fetch_all_remotes_reports_each_unreachable_remote_and_completes`) and Task 8 (`names the failed remotes after a partial fetch all`).

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/git-core/src/graph.rs` | `GraphCommit.tags`, tags in `RefDecorations`, `graph_log` skips missing selected refs |
| `crates/git-core/src/branch.rs` | `BranchInfo.upstream/ahead/behind` |
| `crates/git-core/src/remote.rs` | `TransferOperation::{FetchAll, DeleteRemoteBranch}`, `RemoteInfo.prune`, `FetchAllOutcome`, `fetch_remote(prune)`, `fetch_all`, `push_branch`, `delete_remote_branch`, `set_branch_upstream`, `configured_upstream`, push-target fix |
| `crates/git-core/tests/{graph,branch,remote}.rs` | real-repo tests |
| `crates/repo-service/src/worker/mod.rs` | `TransferEvent::RemoteFailed`, `Command::{FetchAllRemotes, PushBranch, DeleteRemoteBranch}`, `FetchRemote.prune`, dispatch, worker tests |
| `crates/repo-service/src/worker/remote.rs` | handlers + `WorkerHandle` methods |
| `crates/tauri-app/src/commands/{mod,remote}.rs`, `main.rs` | DTO fields, `RemoteFailed` payload, commands, registration, DTO tests |
| `crates/vscode-sidecar/src/dispatch.rs`, `tests/protocol_roundtrip.rs` | same for the sidecar |
| `frontend/src/ipc/RepoClient.ts`, `tauriRepoClient.ts`, `vscodeRepoClient.ts` (+ tests, fixtures) | types and methods |
| `frontend/src/lib/fetchPrune.ts`, `lib/graphSelection.ts` (new, + tests) | prune preference, selection pruning |
| `frontend/src/state/useRemoteTransferActions.ts`, `useBranchActions.ts`, `useAppState.ts` (+ `useAppState.test.ts`) | new actions and state |
| `frontend/src/components/primitives/ContextMenu.tsx` (+ css, test) | `checked` items |
| `frontend/src/components/RefBadges.tsx` (new, + test), `CommitGraph.tsx`, `CommitGraph.module.css`, `FileHistoryList.tsx`, `CommitSearchResults.tsx` | badges, tag menu |
| `frontend/src/components/PushBranchDialog.tsx`, `DeleteRemoteBranchDialog.tsx` (new, + css, tests) | dialogs |
| `frontend/src/components/BranchTree.tsx`, `SyncBar.tsx` (+ css, tests) | menu items, counts, cache reload, split Fetch, publish |
| `frontend/src/App.tsx`, `lib/commands.ts` (+ tests) | wiring, palette |
| `e2e/specs/refs-and-remotes.spec.ts` (new) | E2E |
| `CHANGELOG.md`, `docs/USER_GUIDE.md`, `docs/ARCHITECTURE.md` | docs |

---

### Task 1: Tags on graph rows, and `graph_log` skips missing selected branches (#97)

**Files:**
- Modify: `crates/git-core/src/graph.rs`
- Modify: `crates/tauri-app/src/commands/mod.rs` (`GraphCommitDto` + `From`, every `GraphCommit { … }` literal in its tests)
- Modify: `crates/vscode-sidecar/src/dispatch.rs` (`GraphCommitDto` + `From`)
- Test: `crates/git-core/tests/graph.rs`, `crates/tauri-app/src/commands/mod.rs` (`graph_commit_dto_serializes_camel_case_fields`)

**Interfaces:**
- Produces: `GraphCommit.tags: Vec<String>` (sorted tag names without `refs/tags/`), and wire field `tags` on every `GraphCommitDto` (graph, file history, search pages).

- [ ] **Step 1: Write the failing tests**

Append to `crates/git-core/tests/graph.rs`:

```rust
#[test]
fn graph_log_reports_lightweight_and_annotated_tags_sorted_on_their_commit() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "first");
    write_file(dir.path(), "file.txt", "v2");
    commit_all(&repo, "second");
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    let signature = repo.signature().unwrap();
    repo.tag_lightweight("zeta", head.as_object(), false).unwrap();
    repo.tag("alpha", head.as_object(), &signature, "annotated", false).unwrap();

    let commits = git_core::graph::graph_log(&repo, 10, None).unwrap();

    assert_eq!(commits[0].summary, "second");
    assert_eq!(commits[0].tags, vec!["alpha".to_string(), "zeta".to_string()]);
    assert!(commits[1].tags.is_empty());
}

#[test]
fn graph_log_ignores_a_tag_that_does_not_point_at_a_commit() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "first");
    let tree = repo.head().unwrap().peel_to_tree().unwrap();
    repo.tag_lightweight("tree-tag", tree.as_object(), false).unwrap();

    let commits = git_core::graph::graph_log(&repo, 10, None).unwrap();

    assert_eq!(commits.len(), 1);
    assert!(commits[0].tags.is_empty());
}

#[test]
fn graph_log_skips_a_selected_branch_that_no_longer_exists() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let main = repo.head().unwrap().shorthand().unwrap().to_string();
    git_core::branch::create_branch(&repo, "gone", "HEAD").unwrap();
    git_core::branch::switch_branch(&repo, &main).unwrap();
    git_core::branch::delete_branch(&repo, "gone", true).unwrap();

    let commits =
        git_core::graph::graph_log(&repo, 10, Some(&["gone".to_string(), main.clone()])).unwrap();

    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].summary, "initial commit");
    assert!(git_core::graph::graph_log(&repo, 10, Some(&["gone".to_string()]))
        .unwrap()
        .is_empty());
}
```

In `crates/tauri-app/src/commands/mod.rs`'s `graph_commit_dto_serializes_camel_case_fields`, add `tags: vec!["v1.0".into()],` to the `GraphCommit` literal and assert `assert_eq!(json["tags"], serde_json::json!(["v1.0"]));`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --test graph`
Expected: FAIL to compile (`no field tags on GraphCommit`).

- [ ] **Step 3: Implement**

`graph.rs`:
- Add the field after `remote_branch_refs`: `/// Tag names (without "refs/tags/") whose target peels to this commit, sorted.` `pub tags: Vec<String>,`
- `RefDecorations` gains `tags: HashMap<Oid, Vec<String>>`. In `load`, after the remote loop:

```rust
        let mut tags: HashMap<Oid, Vec<String>> = HashMap::new();
        for reference in repo.references_glob("refs/tags/*")? {
            let reference = reference?;
            let Ok(full_name) = reference.name() else {
                continue;
            };
            let Some(name) = full_name.strip_prefix("refs/tags/") else {
                continue;
            };
            // Lightweight and (nested) annotated tags alike; a tag on a tree or blob has no row.
            let Ok(commit) = reference.peel_to_commit() else {
                continue;
            };
            tags.entry(commit.id()).or_default().push(name.to_string());
        }
        for names in tags.values_mut() {
            names.sort();
        }
        Ok(Self { local, remote, tags })
```

- `graph_commit` sets `tags: self.tags.get(&oid).cloned().unwrap_or_default(),`.
- Update the doc comment on `RefDecorations` ("Local/remote branch names and tag names keyed by …").
- `graph_log`'s `Some(names)` arm:

```rust
        Some(names) => {
            // A persisted selection can outlive its branches (deleted or renamed, possibly
            // outside the app). Skip the missing ones, like `search_commits`, instead of
            // failing the whole graph (#97).
            for name in names {
                let refname = format!("refs/heads/{name}");
                match repo.find_reference(&refname) {
                    Ok(_) => revwalk.push_ref(&refname)?,
                    Err(error) if error.code() == git2::ErrorCode::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
```

- `crates/tauri-app/src/commands/mod.rs`: `GraphCommitDto` gains `pub tags: Vec<String>,` and `From` maps `tags: c.tags`. Add `tags: vec![],` to the other `GraphCommit` literals in the test module (`file_history_entry_dto_…`, `search_page_dto_…`). Let `cargo test -p tauri-app --no-run` list any that remain.
- `crates/vscode-sidecar/src/dispatch.rs`: the same field on its `GraphCommitDto` and `From`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p git-core --test graph && cargo test -p tauri-app graph_commit_dto && cargo test -p vscode-sidecar`
Expected: PASS.

- [ ] **Step 5: Lint and commit**

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`

```bash
git add crates/git-core crates/tauri-app crates/vscode-sidecar
git commit -m "feat(git-core): tag badges on graph rows; skip missing selected branches (#97)"
```

---

### Task 2: `BranchInfo` upstream, ahead and behind

**Files:**
- Modify: `crates/git-core/src/remote.rs` (`configured_upstream`, make `ahead_behind` `pub(crate)`, `current_upstream` uses the helper)
- Modify: `crates/git-core/src/branch.rs`
- Modify: `crates/tauri-app/src/commands/mod.rs` (`BranchInfoDto` + `From` + a new test after `graph_commit_dto_serializes_camel_case_fields`), `crates/vscode-sidecar/src/dispatch.rs` (`BranchInfoDto` + `From`)
- Test: `crates/git-core/tests/branch.rs`

**Interfaces:**
- Produces: `BranchInfo { name, is_current, upstream: Option<String>, ahead: Option<usize>, behind: Option<usize> }`.
- Produces (crate-internal, used by Task 3): `pub(crate) fn configured_upstream(config: &git2::Config, local_branch: &str) -> Result<Option<(String, String)>, git2::Error>`, which returns `(remote_name, remote_branch)`. Also `pub(crate) fn ahead_behind(repo, local_branch, remote_name, remote_branch) -> (Option<usize>, Option<usize>)` (existing).
- Wire: `BranchInfoDto { name, isCurrent, upstream, ahead, behind }`. The nulls are serialized.

- [ ] **Step 1: Write the failing tests**

Append to `crates/git-core/tests/branch.rs`:

```rust
#[test]
fn list_branches_reports_no_upstream_or_counts_without_upstream_config() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "content");
    commit_all(&repo, "initial commit");

    let branch = git_core::branch::list_branches(&repo).unwrap().remove(0);

    assert_eq!(branch.upstream, None);
    assert_eq!((branch.ahead, branch.behind), (None, None));
}

#[test]
fn list_branches_reports_the_upstream_but_no_counts_before_the_first_fetch() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "content");
    commit_all(&repo, "initial commit");
    let name = repo.head().unwrap().shorthand().unwrap().to_string();
    git_core::remote::add_remote(&repo, "origin", "file:///tmp/never-fetched.git", None).unwrap();
    git_core::remote::set_current_upstream(&repo, "origin", "trunk").unwrap();

    let branch = git_core::branch::list_branches(&repo)
        .unwrap()
        .into_iter()
        .find(|branch| branch.name == name)
        .unwrap();

    assert_eq!(branch.upstream.as_deref(), Some("origin/trunk"));
    assert_eq!((branch.ahead, branch.behind), (None, None));
}

#[test]
fn list_branches_counts_commits_ahead_and_behind_the_tracking_ref() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "1");
    commit_all(&repo, "base");
    let base = repo.head().unwrap().target().unwrap();
    write_file(dir.path(), "file.txt", "remote");
    commit_all(&repo, "remote only");
    let remote_tip = repo.head().unwrap().target().unwrap();
    let head_ref = repo.head().unwrap().name().unwrap().to_string();
    let name = repo.head().unwrap().shorthand().unwrap().to_string();
    repo.reference(&head_ref, base, true, "rewind").unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force())).unwrap();
    write_file(dir.path(), "file.txt", "local 1");
    commit_all(&repo, "local 1");
    write_file(dir.path(), "file.txt", "local 2");
    commit_all(&repo, "local 2");
    git_core::remote::add_remote(&repo, "origin", "file:///tmp/origin.git", None).unwrap();
    repo.reference("refs/remotes/origin/main", remote_tip, true, "test").unwrap();
    git_core::remote::set_current_upstream(&repo, "origin", "main").unwrap();

    let branch = git_core::branch::list_branches(&repo)
        .unwrap()
        .into_iter()
        .find(|branch| branch.name == name)
        .unwrap();

    assert_eq!(branch.upstream.as_deref(), Some("origin/main"));
    assert_eq!((branch.ahead, branch.behind), (Some(2), Some(1)));
}
```

In `crates/tauri-app/src/commands/mod.rs`'s test module, add:

```rust
    #[test]
    fn branch_info_dto_serializes_upstream_and_counts_including_nulls() {
        let tracked = serde_json::to_value(BranchInfoDto::from(git_core::branch::BranchInfo {
            name: "main".into(),
            is_current: true,
            upstream: Some("origin/main".into()),
            ahead: Some(2),
            behind: Some(0),
        }))
        .unwrap();
        assert_eq!(tracked["isCurrent"], true);
        assert_eq!(tracked["upstream"], "origin/main");
        assert_eq!(tracked["ahead"], 2);
        assert_eq!(tracked["behind"], 0);

        let untracked = serde_json::to_value(BranchInfoDto::from(git_core::branch::BranchInfo {
            name: "topic".into(),
            is_current: false,
            upstream: None,
            ahead: None,
            behind: None,
        }))
        .unwrap();
        let object = untracked.as_object().unwrap();
        assert_eq!(object["upstream"], serde_json::Value::Null);
        assert!(object.contains_key("ahead") && object.contains_key("behind"));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --test branch`
Expected: FAIL to compile (`no field upstream on BranchInfo`).

- [ ] **Step 3: Implement**

`remote.rs`:

```rust
/// `(remote, remote_branch)` from `branch.<local_branch>.remote` / `.merge`, or `None` when
/// either key is missing. Never checks that the tracking ref exists.
pub(crate) fn configured_upstream(
    config: &git2::Config,
    local_branch: &str,
) -> Result<Option<(String, String)>, git2::Error> {
    let read = |key: String| match config.get_string(&key) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.code() == ErrorCode::NotFound => Ok(None),
        Err(error) => Err(error),
    };
    let Some(remote) = read(format!("branch.{local_branch}.remote"))? else {
        return Ok(None);
    };
    let Some(merge) = read(format!("branch.{local_branch}.merge"))? else {
        return Ok(None);
    };
    let branch = merge.strip_prefix("refs/heads/").unwrap_or(&merge).to_string();
    Ok(Some((remote, branch)))
}
```

- Make `fn ahead_behind` `pub(crate)`.
- In `current_upstream`, replace the `remote_key`/`merge_ref` reads with:

```rust
    let config = repo.config()?;
    let Some((remote_name, remote_branch)) = configured_upstream(&config, &local_branch)? else {
        return Ok(None);
    };
```

  Behavior change: a `.remote` without a `.merge` now gives `None` instead of an error.

`branch.rs`: add the three fields with the spec's doc comments. `list_branches`:

```rust
pub fn list_branches(repo: &Repository) -> Result<Vec<BranchInfo>, BranchError> {
    let config = repo.config()?;
    let mut branches = Vec::new();
    for entry in repo.branches(Some(BranchType::Local))? {
        let (branch, _) = entry?;
        let Ok(Some(name)) = branch.name() else {
            continue;
        };
        let configured = crate::remote::configured_upstream(&config, name)?;
        let (ahead, behind) = match &configured {
            Some((remote, remote_branch)) => {
                crate::remote::ahead_behind(repo, name, remote, remote_branch)
            }
            None => (None, None),
        };
        branches.push(BranchInfo {
            name: name.to_string(),
            is_current: branch.is_head(),
            upstream: configured.map(|(remote, remote_branch)| format!("{remote}/{remote_branch}")),
            ahead,
            behind,
        });
    }
    Ok(branches)
}
```

- Tauri `BranchInfoDto` gains `pub upstream: Option<String>, pub ahead: Option<usize>, pub behind: Option<usize>`, plus the `From` mapping. Make the same change in the sidecar's `BranchInfoDto`. Do not add `skip_serializing_if`: the nulls are part of the contract.

- [ ] **Step 4: Run tests**

Run: `cargo test -p git-core && cargo test -p tauri-app branch_info_dto && cargo test -p vscode-sidecar`
Expected: PASS.

- [ ] **Step 5: Lint and commit**

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`

```bash
git add crates/git-core crates/tauri-app crates/vscode-sidecar
git commit -m "feat(git-core): report upstream and ahead/behind counts per local branch"
```

---

### Task 3: `push_branch`, `delete_remote_branch`, and the upstream push-target fix

**Files:**
- Modify: `crates/git-core/src/remote.rs`
- Test: `crates/git-core/tests/remote.rs` (extend the `use git_core::remote::{…}` list with `delete_remote_branch, push_branch`)

**Interfaces:**
- Consumes: `configured_upstream` (Task 2).
- Produces (used by Task 5):

```rust
pub enum TransferOperation { Fetch, Pull, PushBranch, ForcePushBranch, PushTags, DeleteRemoteBranch, Clone } // FetchAll added in Task 4
#[allow(clippy::too_many_arguments)]
pub fn push_branch(repo: &Repository, remote_name: &str, local_branch: &str, remote_branch: &str, set_upstream: bool,
    credentials: &mut dyn CredentialProvider, reporter: &mut dyn TransferReporter, cancel: &dyn Fn() -> bool) -> Result<(), RemoteError>;
pub fn delete_remote_branch(repo: &Repository, remote_name: &str, branch: &str,
    credentials: &mut dyn CredentialProvider, reporter: &mut dyn TransferReporter, cancel: &dyn Fn() -> bool) -> Result<(), RemoteError>;
pub fn set_branch_upstream(repo: &Repository, local_branch: &str, remote_name: &str, remote_branch: &str) -> Result<(), RemoteError>;
```

- [ ] **Step 1: Write the failing tests**

Append to `crates/git-core/tests/remote.rs`:

```rust
fn quiet() -> (NoCredentials, VecReporter) {
    (NoCredentials, VecReporter::default())
}

#[test]
fn pushing_a_non_current_branch_without_upstream_creates_only_the_remote_branch() {
    let fixture = local_and_bare_remote();
    let head = fixture.local.head().unwrap().peel_to_commit().unwrap();
    fixture.local.branch("topic", &head, false).unwrap();
    let (mut credentials, mut reporter) = quiet();

    push_branch(&fixture.local, "origin", "topic", "topic", false, &mut credentials, &mut reporter, &|| false).unwrap();

    assert_eq!(fixture.bare_tip("topic"), Some(head.id()));
    assert_eq!(fixture.local.head().unwrap().shorthand().unwrap(), "main");
    assert!(fixture.local.config().unwrap().get_string("branch.topic.remote").is_err());
}

#[test]
fn pushing_with_set_upstream_records_the_upstream_under_the_chosen_name() {
    let fixture = local_and_bare_remote();
    let head = fixture.local.head().unwrap().peel_to_commit().unwrap();
    fixture.local.branch("topic", &head, false).unwrap();
    let (mut credentials, mut reporter) = quiet();

    push_branch(&fixture.local, "origin", "topic", "published", true, &mut credentials, &mut reporter, &|| false).unwrap();

    assert_eq!(fixture.bare_tip("published"), Some(head.id()));
    assert_eq!(fixture.bare_tip("topic"), None);
    let config = fixture.local.config().unwrap();
    assert_eq!(config.get_string("branch.topic.remote").unwrap(), "origin");
    assert_eq!(config.get_string("branch.topic.merge").unwrap(), "refs/heads/published");
    let topic = git_core::branch::list_branches(&fixture.local)
        .unwrap()
        .into_iter()
        .find(|branch| branch.name == "topic")
        .unwrap();
    assert_eq!(topic.upstream.as_deref(), Some("origin/published"));
    assert_eq!((topic.ahead, topic.behind), (Some(0), Some(0)));
}

#[test]
fn a_rejected_push_with_set_upstream_leaves_the_upstream_unset() {
    let fixture = local_and_bare_remote();
    let old = fixture.local.head().unwrap().peel_to_commit().unwrap();
    fixture.local.branch("old", &old, false).unwrap();
    fixture.remote_commit("remote moves on");
    // `old` lacks the remote's new commit, so pushing it onto main is not a fast-forward.
    let (mut credentials, mut reporter) = quiet();

    let result = push_branch(&fixture.local, "origin", "old", "main", true, &mut credentials, &mut reporter, &|| false);

    assert!(matches!(result, Err(RemoteError::NonFastForward)));
    assert!(fixture.local.config().unwrap().get_string("branch.old.remote").is_err());
}

#[test]
fn an_invalid_remote_branch_name_is_rejected_before_connecting() {
    let fixture = local_and_bare_remote();
    let (mut credentials, mut reporter) = quiet();

    let result = push_branch(&fixture.local, "origin", "main", "bad name", false, &mut credentials, &mut reporter, &|| false);

    assert!(matches!(result, Err(RemoteError::Git(_))));
    assert!(reporter.events.is_empty());
    assert_eq!(fixture.bare_tip("bad name"), None);
}

#[test]
fn push_current_branch_targets_the_upstream_branch_name_on_that_remote() {
    let fixture = local_and_bare_remote();
    fixture.local_commit("local work");
    let local_tip = fixture.local.head().unwrap().target().unwrap();
    let main_before = fixture.bare_tip("main");
    set_current_upstream(&fixture.local, "origin", "renamed").unwrap();
    let (mut credentials, mut reporter) = quiet();

    push_current_branch(&fixture.local, "origin", &mut credentials, &mut reporter, &|| false).unwrap();

    assert_eq!(fixture.bare_tip("renamed"), Some(local_tip));
    assert_eq!(fixture.bare_tip("main"), main_before);
}

#[test]
fn force_push_with_a_differently_named_upstream_overwrites_that_branch() {
    let fixture = local_and_bare_remote();
    fixture.local_commit("a");
    let (mut credentials, mut reporter) = quiet();
    push_branch(&fixture.local, "origin", "main", "renamed", true, &mut credentials, &mut reporter, &|| false).unwrap();
    let base = fixture.local.head().unwrap().peel_to_commit().unwrap().parent(0).unwrap();
    fixture.local.reset(base.as_object(), git2::ResetType::Hard, None).unwrap();
    fixture.write_local("local.txt", "b");
    common::commit_all(&fixture.local, "b");
    let rewritten = fixture.local.head().unwrap().target().unwrap();
    let main_before = fixture.bare_tip("main");

    force_push_current_branch(&fixture.local, "origin", &mut credentials, &mut reporter, &|| false).unwrap();

    assert_eq!(fixture.bare_tip("renamed"), Some(rewritten));
    assert_eq!(fixture.bare_tip("main"), main_before);
}

#[test]
fn deleting_a_remote_branch_removes_the_remote_ref_and_the_tracking_ref() {
    let fixture = local_and_bare_remote();
    let head = fixture.local.head().unwrap().peel_to_commit().unwrap();
    fixture.local.branch("doomed", &head, false).unwrap();
    let (mut credentials, mut reporter) = quiet();
    push_branch(&fixture.local, "origin", "doomed", "doomed", false, &mut credentials, &mut reporter, &|| false).unwrap();
    assert!(fixture.bare_tip("doomed").is_some());
    assert!(fixture.local.find_reference("refs/remotes/origin/doomed").is_ok());
    let mut events = VecReporter::default();

    delete_remote_branch(&fixture.local, "origin", "doomed", &mut credentials, &mut events, &|| false).unwrap();

    assert_eq!(fixture.bare_tip("doomed"), None);
    assert!(fixture.local.find_reference("refs/remotes/origin/doomed").is_err());
    assert!(fixture.local.find_branch("doomed", git2::BranchType::Local).is_ok());
    assert!(events.events.iter().all(|event| event.operation == TransferOperation::DeleteRemoteBranch));
}

#[test]
fn a_remote_branch_delete_cancelled_at_negotiation_leaves_both_refs() {
    let fixture = local_and_bare_remote();
    let (mut credentials, mut reporter) = quiet();

    let result = delete_remote_branch(&fixture.local, "origin", "main", &mut credentials, &mut reporter, &|| true);

    assert!(matches!(result, Err(RemoteError::Cancelled)));
    assert!(fixture.bare_tip("main").is_some());
    assert!(fixture.local.find_reference("refs/remotes/origin/main").is_ok());
}

#[test]
fn deleting_head_or_an_invalid_name_is_rejected_before_connecting() {
    let fixture = local_and_bare_remote();
    let (mut credentials, mut reporter) = quiet();

    for name in ["HEAD", "", "bad name"] {
        let result = delete_remote_branch(&fixture.local, "origin", name, &mut credentials, &mut reporter, &|| false);
        assert!(matches!(result, Err(RemoteError::Git(_))), "{name:?}");
    }
    assert!(reporter.events.is_empty());
}
```

If `cargo fmt` reflows these long calls, keep its output.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --test remote`
Expected: FAIL to compile (`push_branch` / `delete_remote_branch` not found).

- [ ] **Step 3: Implement**

In `remote.rs`:

- Add `DeleteRemoteBranch` to `TransferOperation` (after `PushTags`).
- Add the helpers and functions:

```rust
fn validate_branch_name(name: &str) -> Result<(), RemoteError> {
    if name.is_empty()
        || name == "HEAD"
        || name.starts_with('-')
        || !git2::Reference::is_valid_name(&format!("refs/heads/{name}"))
    {
        return Err(git2::Error::from_str("invalid branch name").into());
    }
    Ok(())
}

/// Records `<remote_name>/<remote_branch>` as `local_branch`'s upstream. Uses
/// `Branch::set_upstream` when the tracking ref exists, else writes the two config keys itself
/// (a branch can track a remote branch that has not been fetched yet).
pub fn set_branch_upstream(
    repo: &Repository,
    local_branch: &str,
    remote_name: &str,
    remote_branch: &str,
) -> Result<(), RemoteError> {
    repo.find_remote(remote_name)?;
    let mut branch = repo.find_branch(local_branch, BranchType::Local)?;
    let upstream = format!("{remote_name}/{remote_branch}");
    match branch.set_upstream(Some(&upstream)) {
        Ok(()) => {}
        Err(error) if error.code() == ErrorCode::NotFound => {
            let mut config = repo.config()?;
            config.set_str(&format!("branch.{local_branch}.remote"), remote_name)?;
            config.set_str(
                &format!("branch.{local_branch}.merge"),
                &format!("refs/heads/{remote_branch}"),
            )?;
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

/// Pushes `local_branch` to `refs/heads/<remote_branch>` on `remote_name`. With `set_upstream`,
/// a push that lands also makes that remote branch the local branch's upstream; a failed push
/// leaves the config untouched. `cancel` behaves as for `push_current_branch`.
#[allow(clippy::too_many_arguments)]
pub fn push_branch(
    repo: &Repository,
    remote_name: &str,
    local_branch: &str,
    remote_branch: &str,
    set_upstream: bool,
    credentials: &mut dyn CredentialProvider,
    reporter: &mut dyn TransferReporter,
    cancel: &dyn Fn() -> bool,
) -> Result<(), RemoteError> {
    repo.find_branch(local_branch, BranchType::Local)?;
    validate_branch_name(remote_branch)?;
    let refspec = format!("refs/heads/{local_branch}:refs/heads/{remote_branch}");
    push_refs(
        repo,
        remote_name,
        &[refspec],
        TransferOperation::PushBranch,
        credentials,
        reporter,
        cancel,
        None,
    )?;
    if set_upstream {
        set_branch_upstream(repo, local_branch, remote_name, remote_branch)?;
    }
    Ok(())
}

/// Deletes `refs/heads/<branch>` on `remote_name`, then our `refs/remotes/<remote>/<branch>`.
/// Local branches that tracked it keep their config (git's "gone" upstream).
pub fn delete_remote_branch(
    repo: &Repository,
    remote_name: &str,
    branch: &str,
    credentials: &mut dyn CredentialProvider,
    reporter: &mut dyn TransferReporter,
    cancel: &dyn Fn() -> bool,
) -> Result<(), RemoteError> {
    validate_branch_name(branch)?;
    push_refs(
        repo,
        remote_name,
        &[format!(":refs/heads/{branch}")],
        TransferOperation::DeleteRemoteBranch,
        credentials,
        reporter,
        cancel,
        None,
    )?;
    // libgit2's push update_tips normally removed it already; this makes it not depend on that.
    match repo.find_reference(&format!("refs/remotes/{remote_name}/{branch}")) {
        Ok(mut tracking) => tracking.delete()?,
        Err(error) if error.code() == ErrorCode::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

/// The current branch and where pushing it to `remote_name` lands: its upstream branch when the
/// upstream lives on that remote, else the same name.
fn current_push_target(
    repo: &Repository,
    remote_name: &str,
) -> Result<(String, String), RemoteError> {
    let local = current_local_branch_name(repo)?;
    let config = repo.config()?;
    let target = match configured_upstream(&config, &local)? {
        Some((remote, branch)) if remote == remote_name => branch,
        _ => local.clone(),
    };
    Ok((local, target))
}
```

- `push_current_branch` body becomes:

```rust
    let (local, target) = current_push_target(repo, remote_name)?;
    push_branch(repo, remote_name, &local, &target, false, credentials, reporter, cancel)
```

  Update its doc comment: it pushes to the upstream's branch name when the upstream is on `remote_name`.
- `force_push_current_branch`: replace `let branch = current_local_branch_name(repo)?;` with `let (local, target) = current_push_target(repo, remote_name)?;`. The lease reads `refs/remotes/{remote_name}/{target}`, `dst` is `refs/heads/{target}`, and the refspec is `+refs/heads/{local}:{dst}`. Update the doc comment the same way.
- `set_current_upstream` becomes:

```rust
    repo.find_remote(remote_name)?;
    let local_branch = current_local_branch_name(repo)?;
    set_branch_upstream(repo, &local_branch, remote_name, remote_branch)
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p git-core`
Expected: PASS, including every existing push, force-push and upstream test.

- [ ] **Step 5: Lint and commit**

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`

```bash
git add crates/git-core
git commit -m "feat(git-core): push_branch with set_upstream, delete_remote_branch; push to the upstream's branch name"
```

---

### Task 4: `fetch_remote(prune)`, `fetch_all`, `RemoteInfo.prune`

**Files:**
- Modify: `crates/git-core/src/remote.rs`
- Modify: `crates/repo-service/src/worker/remote.rs` (the two existing `fetch_remote` calls pass `None`), `crates/repo-service/src/worker/mod.rs` (the `RemoteInfo { … }` literal in `remote_and_current_upstream_round_trip_through_the_worker` gains `prune: false`)
- Test: `crates/git-core/tests/remote.rs` (existing `fetch_remote(...)` calls gain `None` after the remote name; extend the `use` list with `fetch_all, FetchAllOutcome`)

**Interfaces:**
- Produces (used by Task 5):

```rust
pub enum TransferOperation { Fetch, FetchAll, Pull, PushBranch, ForcePushBranch, PushTags, DeleteRemoteBranch, Clone }
pub struct RemoteInfo { pub name: String, pub fetch_url: String, pub push_url: Option<String>, pub prune: bool }
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FetchAllOutcome { pub fetched: Vec<String>, pub failed: Vec<(String, TransferErrorKind)> }
pub fn fetch_remote(repo: &Repository, remote_name: &str, prune: Option<bool>, operation_id: String,
    credentials: &mut dyn CredentialProvider, reporter: &mut dyn TransferReporter, cancel: &dyn Fn() -> bool) -> Result<(), RemoteError>;
pub fn fetch_all<'c>(repo: &Repository, prune: Option<bool>, operation_id: String,
    credentials_for: &mut dyn FnMut(&str) -> Result<Box<dyn CredentialProvider + 'c>, RemoteError>,
    reporter: &mut dyn TransferReporter, cancel: &dyn Fn() -> bool) -> Result<FetchAllOutcome, RemoteError>;
```

- [ ] **Step 1: Write the failing tests**

Append to `crates/git-core/tests/remote.rs`:

```rust
fn no_credentials_for(_remote: &str) -> Result<Box<dyn CredentialProvider>, RemoteError> {
    Ok(Box::new(NoCredentials))
}

/// Publishes `branch` from the source repo, fetches it into `local`, then deletes it on the
/// remote, leaving `refs/remotes/origin/<branch>` stale in `local`.
fn stale_tracking_ref(fixture: &RemoteFixture, branch: &str) {
    let head = fixture.source.head().unwrap().peel_to_commit().unwrap();
    fixture.source.branch(branch, &head, false).unwrap();
    let mut origin = fixture.source.find_remote("origin").unwrap();
    origin.push(&[format!("refs/heads/{branch}:refs/heads/{branch}")], None).unwrap();
    fixture.local.find_remote("origin").unwrap().fetch(&[] as &[&str], None, None).unwrap();
    assert!(fixture.local.find_reference(&format!("refs/remotes/origin/{branch}")).is_ok());
    origin.push(&[format!(":refs/heads/{branch}")], None).unwrap();
}

#[test]
fn fetch_with_prune_removes_a_remote_deleted_branch_and_without_keeps_it() {
    let fixture = local_and_bare_remote();
    stale_tracking_ref(&fixture, "gone");
    let mut reporter = VecReporter::default();

    fetch_remote(&fixture.local, "origin", Some(false), "f-1".into(), &mut NoCredentials, &mut reporter, &|| false).unwrap();
    assert!(fixture.local.find_reference("refs/remotes/origin/gone").is_ok());

    fetch_remote(&fixture.local, "origin", Some(true), "f-2".into(), &mut NoCredentials, &mut reporter, &|| false).unwrap();
    assert!(fixture.local.find_reference("refs/remotes/origin/gone").is_err());
    assert!(fixture.local.find_reference("refs/remotes/origin/main").is_ok());
}

#[test]
fn fetch_with_unspecified_prune_follows_fetch_prune_config() {
    let fixture = local_and_bare_remote();
    stale_tracking_ref(&fixture, "gone");
    fixture.local.config().unwrap().set_bool("fetch.prune", true).unwrap();

    fetch_remote(&fixture.local, "origin", None, "f-1".into(), &mut NoCredentials, &mut VecReporter::default(), &|| false).unwrap();

    assert!(fixture.local.find_reference("refs/remotes/origin/gone").is_err());
}

#[test]
fn remote_info_prune_prefers_the_remote_setting_over_fetch_prune() {
    let (_dir, repo) = common::init_repo();
    add_remote(&repo, "a", "file:///tmp/a.git", None).unwrap();
    add_remote(&repo, "b", "file:///tmp/b.git", None).unwrap();
    assert!(list_remotes(&repo).unwrap().iter().all(|remote| !remote.prune));
    let mut config = repo.config().unwrap();
    config.set_bool("fetch.prune", true).unwrap();
    config.set_bool("remote.b.prune", false).unwrap();

    let remotes = list_remotes(&repo).unwrap();

    assert!(remotes.iter().find(|remote| remote.name == "a").unwrap().prune);
    assert!(!remotes.iter().find(|remote| remote.name == "b").unwrap().prune);
}

#[test]
fn fetch_all_records_an_unreachable_remote_and_fetches_the_rest() {
    let fixture = local_and_bare_remote();
    fixture.remote_commit_without_fetch("only on the remote");
    let missing = tempfile::TempDir::new().unwrap();
    fixture.local.remote("broken", missing.path().join("nope").to_str().unwrap()).unwrap();
    let mut reporter = VecReporter::default();

    let outcome = fetch_all(&fixture.local, None, "all-1".into(), &mut no_credentials_for, &mut reporter, &|| false).unwrap();

    assert_eq!(outcome.fetched, vec!["origin".to_string()]);
    assert_eq!(outcome.failed, vec![("broken".to_string(), TransferErrorKind::TransferFailed)]);
    assert_eq!(
        fixture.local.find_reference("refs/remotes/origin/main").unwrap().target(),
        Some(fixture.remote_tip())
    );
    assert!(reporter.events.iter().all(|event| {
        event.operation_id == "all-1" && event.operation == TransferOperation::FetchAll
    }));
}

#[test]
fn fetch_all_without_remotes_is_an_empty_success() {
    let (_dir, repo) = common::init_repo();

    let outcome = fetch_all(&repo, None, "all-1".into(), &mut no_credentials_for, &mut VecReporter::default(), &|| false).unwrap();

    assert_eq!(outcome, FetchAllOutcome::default());
}

#[test]
fn fetch_all_cancelled_before_the_first_remote_fetches_nothing() {
    let fixture = local_and_bare_remote();
    fixture.remote_commit_without_fetch("only on the remote");
    let before = fixture.local.find_reference("refs/remotes/origin/main").unwrap().target();

    let result = fetch_all(&fixture.local, None, "all-1".into(), &mut no_credentials_for, &mut VecReporter::default(), &|| true);

    assert!(matches!(result, Err(RemoteError::Cancelled)));
    assert_eq!(fixture.local.find_reference("refs/remotes/origin/main").unwrap().target(), before);
}
```

Then add `None` as the third argument of every existing `fetch_remote(` call in this file (`fetch_updates_tracking_ref_and_reports_owned_progress`, `a_fetch_aborts_when_the_cancel_flag_is_set_before_the_first_progress_tick`, `a_cancelled_fetch_is_not_reported_as_a_generic_transfer_failure`, `fetch_from_an_unreachable_remote_fails_without_creating_a_tracking_ref`).

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --test remote`
Expected: FAIL to compile (`fetch_all` not found, wrong number of arguments to `fetch_remote`).

- [ ] **Step 3: Implement**

In `remote.rs`:

- Add `FetchAll` to `TransferOperation` (after `Fetch`). Import `FetchPrune` from `git2`.
- `RemoteInfo` gains `/// Effective prune default: remote.<name>.prune, else fetch.prune, else false.` `pub prune: bool`. In `list_remotes`, read `let config = repo.config()?;` once before the loop, then:

```rust
            prune: match config_bool(&config, &format!("remote.{name}.prune"))? {
                Some(prune) => prune,
                None => config_bool(&config, "fetch.prune")?.unwrap_or(false),
            },
```

  with

```rust
fn config_bool(config: &git2::Config, key: &str) -> Result<Option<bool>, git2::Error> {
    match config.get_bool(key) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.code() == ErrorCode::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}
```

- Add the outcome type and the prune mapping:

```rust
/// Result of `fetch_all`: remotes fetched, and the ones that failed with why.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FetchAllOutcome {
    pub fetched: Vec<String>,
    pub failed: Vec<(String, TransferErrorKind)>,
}

/// `None` leaves pruning to git config (`remote.<name>.prune`, then `fetch.prune`).
fn fetch_prune(prune: Option<bool>) -> FetchPrune {
    match prune {
        None => FetchPrune::Unspecified,
        Some(true) => FetchPrune::On,
        Some(false) => FetchPrune::Off,
    }
}
```

- Rename the body of `fetch_remote` into a private `fetch_with`, which also takes `prune: Option<bool>`, `operation: TransferOperation` and `operation_id: &str`:
  - each `TransferOperation::Fetch` in the three callbacks becomes `operation`
  - each `operation_id.clone()` becomes `operation_id.to_string()`
  - after `options.remote_callbacks(callbacks);` add `options.prune(fetch_prune(prune));`
  - give it `#[allow(clippy::too_many_arguments)]`

  `fetch_remote` keeps its doc comment, gains `prune: Option<bool>` after `remote_name`, and calls `fetch_with(repo, remote_name, prune, TransferOperation::Fetch, &operation_id, credentials, reporter, cancel)`.
- `fetch_all`:

```rust
/// Fetches every remote in `repo.remotes()` order under one `operation_id`, reporting progress
/// as `TransferOperation::FetchAll`. A remote that fails (transport, credentials) is recorded in
/// `failed` and the next one is tried; a cancel stops the loop with `Cancelled` (remotes fetched
/// so far stay fetched). `credentials_for` builds each remote's credential provider.
pub fn fetch_all<'c>(
    repo: &Repository,
    prune: Option<bool>,
    operation_id: String,
    credentials_for: &mut dyn FnMut(&str) -> Result<Box<dyn CredentialProvider + 'c>, RemoteError>,
    reporter: &mut dyn TransferReporter,
    cancel: &dyn Fn() -> bool,
) -> Result<FetchAllOutcome, RemoteError> {
    let names: Vec<String> = repo
        .remotes()?
        .iter()
        .flatten()
        .flatten()
        .map(str::to_string)
        .collect();
    let mut outcome = FetchAllOutcome::default();
    for name in names {
        if cancel() {
            return Err(RemoteError::Cancelled);
        }
        let result = credentials_for(&name).and_then(|mut provider| {
            fetch_with(
                repo,
                &name,
                prune,
                TransferOperation::FetchAll,
                &operation_id,
                provider.as_mut(),
                reporter,
                cancel,
            )
        });
        match result {
            Ok(()) => outcome.fetched.push(name),
            Err(RemoteError::Cancelled) => return Err(RemoteError::Cancelled),
            Err(error) => outcome.failed.push((name, error.transfer_error_kind())),
        }
    }
    Ok(outcome)
}
```

- `crates/repo-service/src/worker/remote.rs`: in `fetch` and `pull`, the `git_core::remote::fetch_remote(repo, &remote_name, …)` calls gain `None` as the third argument. Task 5 threads the real value through `fetch`.
- `crates/repo-service/src/worker/mod.rs`: the expected `RemoteInfo` literal gains `prune: false`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p git-core && cargo test -p repo-service`
Expected: PASS.

- [ ] **Step 5: Lint and commit**

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`

```bash
git add crates/git-core crates/repo-service
git commit -m "feat(git-core): fetch prune option, fetch_all with per-remote failures, RemoteInfo.prune"
```

---

### Task 5: Worker commands and the `RemoteFailed` event

**Files:**
- Modify: `crates/repo-service/src/worker/mod.rs`:
  - `TransferEvent::RemoteFailed`
  - `Command::FetchRemote.prune`
  - the new `Command` variants after `PushTags`
  - dispatch arms next to `PushTags`
  - tests after `tags_and_branch_push_round_trip_through_the_worker`
- Modify: `crates/repo-service/src/worker/remote.rs`: rename the handler `push_branch` to `push_current`, then add the handlers `fetch_all`, `push_branch` and `delete_remote_branch`, and the `WorkerHandle` methods.
- Modify (to keep the workspace compiling):
  - `crates/tauri-app/src/commands/mod.rs`: `TransferProgressDto.remote_name`, the `RemoteFailed` arm in `transfer_event_payload`, and a test
  - `crates/tauri-app/src/commands/remote.rs`: `fetch_remote` passes `None`
  - `crates/vscode-sidecar/src/dispatch.rs`: the same DTO field and arm, and `fetch_remote` passes `None`
  - every `handle.fetch_remote(…)` call in worker tests gains `None`

**Interfaces:**
- Consumes: `fetch_all`, `push_branch`, `delete_remote_branch`, `fetch_remote(prune)` (Tasks 3–4).
- Produces (used by Task 6):

```rust
pub enum TransferEvent { Started {..}, Progress(..), RemoteFailed { operation_id: String, remote_name: String, error: TransferErrorKind }, Completed {..} }
impl WorkerHandle {
    pub fn fetch_remote(&self, remote_name: String, prune: Option<bool>, events: Sender<TransferEvent>) -> Result<String, String>;
    pub fn fetch_all_remotes(&self, prune: Option<bool>, events: Sender<TransferEvent>) -> Result<String, String>;
    pub fn push_branch(&self, remote_name: String, local_branch: String, remote_branch: String, set_upstream: bool, events: Sender<TransferEvent>) -> Result<String, String>;
    pub fn delete_remote_branch(&self, remote_name: String, branch_name: String, events: Sender<TransferEvent>) -> Result<String, String>;
}
```

- Tauri `TransferProgressDto` gains `#[serde(skip_serializing_if = "Option::is_none")] pub remote_name: Option<String>`.

- [ ] **Step 1: Write the failing worker tests**

In `mod.rs`'s test module:

```rust
    fn completed(events: &[TransferEvent]) -> (TransferOperation, Option<TransferErrorKind>) {
        match events.last() {
            Some(TransferEvent::Completed { operation, error, .. }) => (*operation, *error),
            other => panic!("expected a Completed event last, got {other:?}"),
        }
    }

    #[test]
    fn fetch_all_remotes_reports_each_unreachable_remote_and_completes() {
        let (_source_dir, _remote_dir, local_dir) = local_and_bare_remote();
        let missing = TempDir::new().expect("missing dir");
        Repository::open(local_dir.path())
            .expect("open local")
            .remote("broken", missing.path().join("nope").to_str().expect("path"))
            .expect("add broken remote");
        let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
        let (event_tx, event_rx) = mpsc::channel();

        let operation_id = worker.handle().fetch_all_remotes(None, event_tx).expect("start fetch all");
        let events: Vec<_> = event_rx.iter().collect();

        assert!(operation_id.starts_with("fetch-all-"));
        assert!(matches!(
            events.first(),
            Some(TransferEvent::Started { operation: TransferOperation::FetchAll, .. })
        ));
        let failed: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                TransferEvent::RemoteFailed { operation_id: id, remote_name, error } => {
                    assert_eq!(id, &operation_id);
                    Some((remote_name.clone(), *error))
                }
                _ => None,
            })
            .collect();
        assert_eq!(failed, vec![("broken".to_string(), TransferErrorKind::TransferFailed)]);
        assert_eq!(completed(&events), (TransferOperation::FetchAll, None));
        let local = Repository::open(local_dir.path()).expect("reopen local");
        assert!(local.branches(Some(git2::BranchType::Remote)).expect("branches").count() > 0);
    }

    #[test]
    fn fetch_all_remotes_fails_when_every_remote_fails() {
        let (local_dir, repo) = init_repo();
        let missing = TempDir::new().expect("missing dir");
        repo.remote("broken", missing.path().join("nope").to_str().expect("path"))
            .expect("add broken remote");
        drop(repo);
        let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
        let (event_tx, event_rx) = mpsc::channel();

        worker.handle().fetch_all_remotes(Some(true), event_tx).expect("start fetch all");
        let events: Vec<_> = event_rx.iter().collect();

        assert_eq!(
            completed(&events),
            (TransferOperation::FetchAll, Some(TransferErrorKind::TransferFailed))
        );
    }

    #[test]
    fn push_branch_publishes_with_upstream_and_delete_remote_branch_removes_it() {
        let (_source_dir, remote_dir, local_dir) = local_and_bare_remote();
        let local = Repository::open(local_dir.path()).expect("open local");
        write_file(local_dir.path(), "local.txt", "local\n");
        commit_all(&local, "local root");
        let head = local.head().expect("head").peel_to_commit().expect("commit");
        local.branch("topic", &head, false).expect("create topic");
        drop(head);
        drop(local);
        let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
        let handle = worker.handle();
        let bare_has = |name: &str| {
            Repository::open_bare(remote_dir.path())
                .expect("open bare")
                .find_reference(&format!("refs/heads/{name}"))
                .is_ok()
        };

        let (event_tx, event_rx) = mpsc::channel();
        let push_id = handle
            .push_branch("origin".into(), "topic".into(), "topic".into(), true, event_tx)
            .expect("start push");
        let events: Vec<_> = event_rx.iter().collect();
        assert!(push_id.starts_with("push-"));
        assert_eq!(completed(&events), (TransferOperation::PushBranch, None));
        assert!(bare_has("topic"));
        let topic = handle
            .list_branches()
            .expect("list branches")
            .into_iter()
            .find(|branch| branch.name == "topic")
            .expect("topic listed");
        assert_eq!(topic.upstream.as_deref(), Some("origin/topic"));

        let (event_tx, event_rx) = mpsc::channel();
        let delete_id = handle
            .delete_remote_branch("origin".into(), "topic".into(), event_tx)
            .expect("start delete");
        let events: Vec<_> = event_rx.iter().collect();
        assert!(delete_id.starts_with("delete-remote-"));
        assert_eq!(completed(&events), (TransferOperation::DeleteRemoteBranch, None));
        assert!(!bare_has("topic"));
        assert!(handle.get_undo_state().expect("undo state").undo.is_none());
    }
```

Also add `None` as the second argument of every existing `handle.fetch_remote("origin".into(), …)` / `.fetch_remote(…)` call in this test module.

In `crates/tauri-app/src/commands/mod.rs`'s test module:

```rust
    #[test]
    fn remote_failed_event_carries_the_remote_name_and_other_events_omit_it() {
        let (event, failed) = transfer_event_payload(TransferEvent::RemoteFailed {
            operation_id: "fetch-all-7".into(),
            remote_name: "backup".into(),
            error: TransferErrorKind::MissingCredential,
        });
        assert_eq!(event, "transfer-progress");
        let json = serde_json::to_value(failed).unwrap();
        assert_eq!(json["phase"], "RemoteFailed");
        assert_eq!(json["operation"], "FetchAll");
        assert_eq!(json["remoteName"], "backup");
        assert_eq!(json["errorKind"], "MissingCredential");

        let (_, completed) = transfer_event_payload(TransferEvent::Completed {
            operation_id: "fetch-all-7".into(),
            operation: TransferOperation::FetchAll,
            error: None,
        });
        let json = serde_json::to_value(completed).unwrap();
        assert!(!json.as_object().unwrap().contains_key("remoteName"));
        assert_eq!(json["operation"], "FetchAll");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p repo-service`
Expected: FAIL to compile (`fetch_all_remotes`, `RemoteFailed` missing).

- [ ] **Step 3: Implement the worker**

`mod.rs`:
- `TransferEvent` gains, between `Progress` and `Completed`:

```rust
    /// One remote of a Fetch all failed; sent before the operation's `Completed`.
    RemoteFailed {
        operation_id: String,
        remote_name: String,
        error: TransferErrorKind,
    },
```

- `Command::FetchRemote` gains `prune: Option<bool>`. Add after `PushTags`:

```rust
    FetchAllRemotes {
        prune: Option<bool>,
        operation_id: String,
        events: Sender<TransferEvent>,
        reply: Sender<Result<String, String>>,
    },
    PushBranch {
        remote_name: String,
        local_branch: String,
        remote_branch: String,
        set_upstream: bool,
        operation_id: String,
        events: Sender<TransferEvent>,
        reply: Sender<Result<String, String>>,
    },
    DeleteRemoteBranch {
        remote_name: String,
        branch_name: String,
        operation_id: String,
        events: Sender<TransferEvent>,
        reply: Sender<Result<String, String>>,
    },
```

- Dispatch: `Command::FetchRemote { remote_name, prune, … } => remote::fetch(&repo, &credential_service, remote_name, prune, remote::TransferOp { … }, events, reply)`. `Command::PushCurrentBranch` now calls `remote::push_current(…)`. Add arms that build `remote::TransferOp { id: operation_id, cancelled: &cancelled }` exactly like `PushTags` and call `remote::fetch_all(&repo, &credential_service, prune, op, events, reply)`, `remote::push_branch(&repo, &credential_service, remote_name, local_branch, remote_branch, set_upstream, op, events, reply)` and `remote::delete_remote_branch(&repo, &credential_service, remote_name, branch_name, op, events, reply)`.

`worker/remote.rs`:
- Rename `pub(super) fn push_branch` (the current-branch handler) to `push_current`.
- `fetch` gains `prune: Option<bool>` after `remote_name` and passes it to `fetch_remote`.
- New handlers. Every one sends `Started`, replies with `Ok(op.id.clone())`, builds a `ChannelReporter`, and runs with `op.cancel_check()`, the same way `push_tags` does. `push_branch` and `delete_remote_branch` end with `complete(&events, op, <operation>, result)`:

```rust
#[allow(clippy::too_many_arguments)]
pub(super) fn push_branch<S: CredentialStore>(
    repo: &git2::Repository,
    credentials: &CredentialService<S>,
    remote_name: String,
    local_branch: String,
    remote_branch: String,
    set_upstream: bool,
    op: TransferOp<'_>,
    events: Sender<TransferEvent>,
    reply: Sender<Result<String, String>>,
) {
    let _ = events.send(TransferEvent::Started {
        operation_id: op.id.clone(),
        operation: TransferOperation::PushBranch,
    });
    let _ = reply.send(Ok(op.id.clone()));
    let mut reporter = ChannelReporter { events: events.clone(), operation_id: op.id.clone() };
    let cancel = op.cancel_check();
    let result = git_core::remote::remote_auth_profile(repo, &remote_name).and_then(|profile| {
        let mut provider = crate::credentials::RemoteCredentialProvider::new(credentials, profile);
        git_core::remote::push_branch(
            repo, &remote_name, &local_branch, &remote_branch, set_upstream,
            &mut provider, &mut reporter, &cancel,
        )
    });
    complete(&events, op, TransferOperation::PushBranch, result);
}
```

  `delete_remote_branch` has the same shape with `TransferOperation::DeleteRemoteBranch` and `git_core::remote::delete_remote_branch(repo, &remote_name, &branch_name, …)`. `fetch_all`:

```rust
pub(super) fn fetch_all<S: CredentialStore>(
    repo: &git2::Repository,
    credentials: &CredentialService<S>,
    prune: Option<bool>,
    op: TransferOp<'_>,
    events: Sender<TransferEvent>,
    reply: Sender<Result<String, String>>,
) {
    let _ = events.send(TransferEvent::Started {
        operation_id: op.id.clone(),
        operation: TransferOperation::FetchAll,
    });
    let _ = reply.send(Ok(op.id.clone()));
    let mut reporter = ChannelReporter { events: events.clone(), operation_id: op.id.clone() };
    let cancel = op.cancel_check();
    let result = git_core::remote::fetch_all(
        repo,
        prune,
        op.id.clone(),
        &mut |remote_name| {
            let profile = git_core::remote::remote_auth_profile(repo, remote_name)?;
            Ok(Box::new(crate::credentials::RemoteCredentialProvider::new(credentials, profile))
                as Box<dyn git_core::remote::CredentialProvider + '_>)
        },
        &mut reporter,
        &cancel,
    );
    let error = match &result {
        Ok(outcome) => {
            for (remote_name, kind) in &outcome.failed {
                let _ = events.send(TransferEvent::RemoteFailed {
                    operation_id: op.id.clone(),
                    remote_name: remote_name.clone(),
                    error: *kind,
                });
            }
            // Only a Fetch all where nothing succeeded fails as a whole.
            if outcome.fetched.is_empty() {
                outcome.failed.first().map(|(_, kind)| *kind)
            } else {
                None
            }
        }
        Err(error) => Some(error.transfer_error_kind()),
    };
    op.clear();
    let _ = events.send(TransferEvent::Completed {
        operation_id: op.id,
        operation: TransferOperation::FetchAll,
        error,
    });
}
```

  If the compiler rejects `'_` in the cast, name the lifetime with a small `fn credential_provider<'a, S: CredentialStore>(repo: &git2::Repository, credentials: &'a CredentialService<S>, remote_name: &str) -> Result<Box<dyn git_core::remote::CredentialProvider + 'a>, git_core::remote::RemoteError>` and call it from the closure.
- `WorkerHandle`:
  - `fetch_remote` gains `prune: Option<bool>` (second parameter) and forwards it.
  - `fetch_all_remotes(prune, events)` sends `Command::FetchAllRemotes` with id `format!("fetch-all-{}", NEXT_TRANSFER_ID.fetch_add(1, Ordering::Relaxed))`.
  - `push_branch(remote_name, local_branch, remote_branch, set_upstream, events)` uses id `push-N`.
  - `delete_remote_branch(remote_name, branch_name, events)` uses id `delete-remote-N`.

  All of them use the same send/recv error strings as `push_tags`.

- [ ] **Step 4: Keep Tauri and the sidecar compiling**

- `crates/tauri-app/src/commands/mod.rs`:
  - `TransferProgressDto` gains `#[serde(skip_serializing_if = "Option::is_none")] pub remote_name: Option<String>,`. Every existing construction (`From`, `Started`, `Completed`) sets `remote_name: None`.
  - New arm:

```rust
        TransferEvent::RemoteFailed {
            operation_id,
            remote_name,
            error,
        } => (
            "transfer-progress",
            TransferProgressDto {
                operation_id,
                // Only Fetch all reports per-remote failures.
                operation: format!("{:?}", git_core::remote::TransferOperation::FetchAll),
                phase: "RemoteFailed".to_string(),
                error_kind: Some(format!("{error:?}")),
                current: 0,
                total: 0,
                received_bytes: 0,
                message: None,
                remote_name: Some(remote_name),
            },
        ),
```

- `crates/tauri-app/src/commands/remote.rs`: `fetch_remote` calls `.fetch_remote(remote_name, None, event_tx)` for now. Task 6 adds the parameter.
- `crates/vscode-sidecar/src/dispatch.rs`: add the same DTO field (and `skip_serializing_if`) and the same arm in `transfer_progress_dto`. `fetch_remote` passes `None`. The test helpers at the bottom of `dispatch.rs` that construct `TransferEvent` are unaffected.

- [ ] **Step 5: Run tests**

Run: `cargo test -p repo-service && cargo test -p tauri-app && cargo test -p vscode-sidecar`
Expected: PASS.

- [ ] **Step 6: Lint and commit**

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`

```bash
git add crates/repo-service crates/tauri-app crates/vscode-sidecar
git commit -m "feat(repo-service): fetch all, push branch and delete remote branch worker commands"
```

---

### Task 6: Tauri commands and sidecar methods

**Files:**
- Modify: `crates/tauri-app/src/commands/remote.rs` (`fetch_remote` gains `prune`, plus three new commands), `crates/tauri-app/src/main.rs` (the `use` list and `generate_handler!`, next to `fetch_remote`), `crates/tauri-app/src/commands/mod.rs` (`RemoteInfoDto.prune` + `From` + a test)
- Modify: `crates/vscode-sidecar/src/dispatch.rs` (method table next to `"fetch_remote"`, param structs, functions, `RemoteInfoDto.prune`)
- Test: `crates/vscode-sidecar/tests/protocol_roundtrip.rs` (a new test after `push_current_branch_and_push_tags_stream_notifications_through_the_sidecar`)

**Interfaces:**
- Consumes: the `WorkerHandle` methods (Task 5).
- Produces: the wire methods in Global Constraints.

- [ ] **Step 1: Write the failing tests**

Tauri, in the test module:

```rust
    #[test]
    fn remote_info_dto_serializes_the_prune_default() {
        let json = serde_json::to_value(RemoteInfoDto::from((
            git_core::remote::RemoteInfo {
                name: "origin".into(),
                fetch_url: "https://example.com/repo.git".into(),
                push_url: None,
                prune: true,
            },
            None,
        )))
        .unwrap();
        assert_eq!(json["prune"], true);
        assert_eq!(json["fetchUrl"], "https://example.com/repo.git");
    }
```

Sidecar. Read the existing `push_current_branch_and_push_tags_stream_notifications_through_the_sidecar` first and reuse its fixture (`local_and_bare_remote()`, `RemoteFixture::local_path`) and its `call_and_collect_notifications` / `is_terminal_notification` helpers exactly as it does:

```rust
#[test]
fn fetch_all_push_branch_and_delete_remote_branch_stream_notifications_through_the_sidecar() {
    let fixture = local_and_bare_remote();
    let repo_path = fixture.local_path();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({ "path": repo_path }));

    let (fetch_all, notes) = sidecar.call_and_collect_notifications(
        2,
        "fetch_all_remotes",
        serde_json::json!({ "repoPath": repo_path, "prune": null }),
    );
    let fetch_id = fetch_all["result"].as_str().unwrap().to_string();
    assert!(fetch_id.starts_with("fetch-all-"));
    let last = notes.last().unwrap();
    assert_eq!(last["params"]["operation"], "FetchAll");
    assert_eq!(last["params"]["phase"], "Completed");

    let (push, notes) = sidecar.call_and_collect_notifications(
        3,
        "push_branch",
        serde_json::json!({
            "repoPath": repo_path, "remoteName": "origin", "localBranch": "main",
            "remoteBranch": "sidecar-published", "setUpstream": false
        }),
    );
    assert!(push["result"].as_str().unwrap().starts_with("push-"));
    assert_eq!(notes.last().unwrap()["params"]["phase"], "Completed");

    let (delete, notes) = sidecar.call_and_collect_notifications(
        4,
        "delete_remote_branch",
        serde_json::json!({ "repoPath": repo_path, "remoteName": "origin", "branchName": "sidecar-published" }),
    );
    assert!(delete["result"].as_str().unwrap().starts_with("delete-remote-"));
    let last = notes.last().unwrap();
    assert_eq!(last["params"]["operation"], "DeleteRemoteBranch");
    assert_eq!(last["params"]["phase"], "Completed");
}
```

Adjust the spawn call, the fixture's local branch name, and the destructuring to match how the existing push test does it. That test is the template, and its helper signatures win over this sketch.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p tauri-app remote_info_dto && cargo test -p vscode-sidecar fetch_all_push_branch`
Expected: FAIL (`no field prune on RemoteInfoDto`; sidecar `Method not found`).

- [ ] **Step 3: Implement**

- Tauri `RemoteInfoDto` gains `pub prune: bool`, mapped from `remote.prune`. The same goes for the sidecar's `RemoteInfoDto`.
- `commands/remote.rs`:

```rust
#[tauri::command]
pub async fn fetch_remote(
    repo_path: String,
    remote_name: String,
    prune: Option<bool>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let (event_tx, event_rx) = mpsc::channel();
    let operation_id =
        worker_handle(&state, &repo_path)?.fetch_remote(remote_name, prune, event_tx)?;
    emit_transfer_events(app, event_rx);
    Ok(operation_id)
}

#[tauri::command]
pub async fn fetch_all_remotes(
    repo_path: String,
    prune: Option<bool>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let (event_tx, event_rx) = mpsc::channel();
    let operation_id = worker_handle(&state, &repo_path)?.fetch_all_remotes(prune, event_tx)?;
    emit_transfer_events(app, event_rx);
    Ok(operation_id)
}
```

  `push_branch(repo_path, remote_name, local_branch, remote_branch, set_upstream: bool, app, state)` and `delete_remote_branch(repo_path, remote_name, branch_name, app, state)` follow the same pattern. All of them are `async fn` (ARCHITECTURE "Threading model"). `pub use remote::*;` already re-exports them. Register all three in `main.rs`.
- Sidecar:
  - Method table entries `"fetch_all_remotes" => fetch_all_remotes(params, repos, stdout)`, `"push_branch" => push_branch(params, repos, stdout)` and `"delete_remote_branch" => delete_remote_branch(params, repos, stdout)`.
  - `FetchRemoteParams` gains `prune: Option<bool>`, and `fetch_remote` forwards it.
  - New `#[derive(Deserialize)] #[serde(rename_all = "camelCase")]` param structs: `FetchAllRemotesParams { repo_path, prune: Option<bool> }`, `PushBranchParams { repo_path, remote_name, local_branch, remote_branch, set_upstream: bool }` and `DeleteRemoteBranchParams { repo_path, remote_name, branch_name }`.
  - Each function body mirrors `push_current_branch`: open a channel, `spawn_progress_relay(event_rx, Arc::clone(stdout))`, call the handle, and return `Value::String(operation_id)`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p tauri-app && cargo test -p vscode-sidecar`
Expected: PASS.

- [ ] **Step 5: Lint and commit**

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`

```bash
git add crates/tauri-app crates/vscode-sidecar
git commit -m "feat: expose fetch_all_remotes, push_branch, delete_remote_branch and fetch prune over Tauri and the sidecar"
```

---

### Task 7: `RepoClient` types and methods in both clients

**Files:**
- Modify: `frontend/src/ipc/RepoClient.ts`, `tauriRepoClient.ts`, `vscodeRepoClient.ts`
- Test: `frontend/src/ipc/tauriRepoClient.test.ts`, `vscodeRepoClient.test.ts`
- Fixtures: every full `RepoClient` literal that `tsc` reports

**Interfaces:**
- Produces (used by Tasks 8–12):

```ts
export type TransferOperation =
  | "Fetch" | "FetchAll" | "Pull" | "PushBranch" | "PushTags" | "ForcePushBranch" | "DeleteRemoteBranch" | "Clone";
// TransferProgress
phase: "Starting" | "Receiving" | "Updating" | "RemoteFailed" | "Completed" | "Failed";
/** Only on `RemoteFailed`: the remote that failed during a Fetch all. */
remoteName?: string;
// BranchInfo
upstream?: string | null; ahead?: number | null; behind?: number | null;
// RemoteInfo
prune?: boolean;
// GraphCommit
tags?: string[];

fetchRemote(repoPath: string, remoteName: string, prune: boolean | null): Promise<string>;
fetchAllRemotes(repoPath: string, prune: boolean | null): Promise<string>;
pushBranch(repoPath: string, remoteName: string, localBranch: string, remoteBranch: string, setUpstream: boolean): Promise<string>;
deleteRemoteBranch(repoPath: string, remoteName: string, branchName: string): Promise<string>;
```

- [ ] **Step 1: Write the failing client tests**

`tauriRepoClient.test.ts`: in "waits for the transfer listeners before fetching" (the test at about line 190), call `fetchRemote(TEST_REPO_PATH, "origin", null)` and expect `{ repoPath: TEST_REPO_PATH, remoteName: "origin", prune: null }`. Add:

```ts
  it("invokes the new transfer commands with camelCase arguments", async () => {
    vi.mocked(invoke).mockResolvedValue("op-1");
    await tauriRepoClient.fetchAllRemotes(TEST_REPO_PATH, true);
    expect(invoke).toHaveBeenLastCalledWith("fetch_all_remotes", { repoPath: TEST_REPO_PATH, prune: true });
    await tauriRepoClient.pushBranch(TEST_REPO_PATH, "origin", "topic", "published", true);
    expect(invoke).toHaveBeenLastCalledWith("push_branch", {
      repoPath: TEST_REPO_PATH,
      remoteName: "origin",
      localBranch: "topic",
      remoteBranch: "published",
      setUpstream: true,
    });
    await tauriRepoClient.deleteRemoteBranch(TEST_REPO_PATH, "origin", "topic");
    expect(invoke).toHaveBeenLastCalledWith("delete_remote_branch", {
      repoPath: TEST_REPO_PATH,
      remoteName: "origin",
      branchName: "topic",
    });
  });
```

The transfer listeners must be ready for these calls to reach `invoke`. Copy the listener-resolution setup from the test above it, or from whichever existing test resolves `transferListenersReady` first, into this test.

`vscodeRepoClient.test.ts`: in "wires fetchRemote, pushCurrentBranch, and pushTags", call `fetchRemote("/repo", "origin", false)` and expect `params: { repoPath: "/repo", remoteName: "origin", prune: false }`. Do the same for the other `fetchRemote("/repo", "origin")` call at about line 788, passing `null`. Add:

```ts
  it("wires fetchAllRemotes, pushBranch and deleteRemoteBranch", async () => {
    const all = vscodeRepoClient.fetchAllRemotes("/repo", null);
    expect(postMessage).toHaveBeenLastCalledWith({
      jsonrpc: "2.0", id: 1, method: "fetch_all_remotes", params: { repoPath: "/repo", prune: null },
    });
    respond(1, "fetch-all-1");
    await expect(all).resolves.toBe("fetch-all-1");

    const push = vscodeRepoClient.pushBranch("/repo", "origin", "topic", "topic", false);
    expect(postMessage).toHaveBeenLastCalledWith({
      jsonrpc: "2.0", id: 2, method: "push_branch",
      params: { repoPath: "/repo", remoteName: "origin", localBranch: "topic", remoteBranch: "topic", setUpstream: false },
    });
    respond(2, "push-2");
    await expect(push).resolves.toBe("push-2");

    const del = vscodeRepoClient.deleteRemoteBranch("/repo", "origin", "topic");
    expect(postMessage).toHaveBeenLastCalledWith({
      jsonrpc: "2.0", id: 3, method: "delete_remote_branch",
      params: { repoPath: "/repo", remoteName: "origin", branchName: "topic" },
    });
    respond(3, "delete-remote-3");
    await expect(del).resolves.toBe("delete-remote-3");
  });
```

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/ipc`
Expected: FAIL (`fetchAllRemotes is not a function`, and the `fetch_remote` params mismatch).

- [ ] **Step 3: Implement**

- `RepoClient.ts`: make the type changes from Interfaces, each optional field with a one-line comment saying the backend always sends it. Add the methods after `pushTags` with doc comments:
  - `fetchRemote`: `prune` `null` = git config decides (`remote.<name>.prune`, then `fetch.prune`).
  - `fetchAllRemotes`: every remote in turn under one operation id; a failed remote arrives as a `RemoteFailed` progress event, and the operation completes unless every remote failed.
  - `pushBranch`: pushes `localBranch` to `remoteBranch`, and when `setUpstream` is set and the push lands, records it as the branch's upstream.
  - `deleteRemoteBranch`: deletes the branch on the remote and its remote-tracking ref.
- `tauriRepoClient.ts`: each new method does `await transferListenersReady;` and then `loggedInvoke<string>("…", { … })`, like `pushCurrentBranch`. `fetchRemote` passes `prune`.
- `vscodeRepoClient.ts`: `call<string>("fetch_all_remotes", { repoPath, prune })`, and so on. `fetchRemote` passes `prune`.

- [ ] **Step 4: Fix call sites and `RepoClient` fixtures**

Run: `cd frontend && pnpm exec tsc --noEmit -p .`
- `useRemoteTransferActions.ts` `fetchRemote`: pass `null` for now (Task 8 wires the preference).
- Every full `RepoClient` literal that errors gets `fetchAllRemotes: async () => "fetch-all-1", pushBranch: async () => "push-1", deleteRemoteBranch: async () => "delete-remote-1",`. In `App.test.tsx`'s `fakeClient`, use `unused` for all three, like other transfer methods there.

Repeat until `tsc` is clean.

- [ ] **Step 5: Run tests and lint**

Run: `cd frontend && pnpm test -- --run && pnpm lint`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): RepoClient fetchAllRemotes, pushBranch, deleteRemoteBranch and fetch prune"
```

---

### Task 8: State: prune preference, new transfer actions, dialogs, selection pruning (#97)

**Files:**
- Create: `frontend/src/lib/fetchPrune.ts` + `fetchPrune.test.ts`, `frontend/src/lib/graphSelection.ts` + `graphSelection.test.ts`
- Modify: `frontend/src/state/useRemoteTransferActions.ts`, `useBranchActions.ts`, `useAppState.ts`
- Test: `frontend/src/state/useAppState.test.ts`

**Interfaces:**
- Consumes: Task 7's client methods.
- Produces (used by Tasks 10–12):

```ts
// lib/fetchPrune.ts
export function readFetchPrune(repoPath: string): boolean | null;
export function writeFetchPrune(repoPath: string, prune: boolean): void;
export function effectiveFetchPrune(override: boolean | null, remotes: RemoteInfo[], upstream: UpstreamInfo | null): boolean;
// lib/graphSelection.ts
export function withoutBranch(selection: string[] | null, name: string, remaining: string[]): string[] | null;
export function withRenamedBranch(selection: string[] | null, oldName: string, newName: string): string[] | null;
// AppState
fetchPrune: boolean | null;
pushDialog: { localBranch: string } | null;
deleteRemoteBranchDialog: { remoteName: string | null; branchName: string | null } | null;
// UseAppStateResult / RemoteTransferActions
fetchAllRemotes(): Promise<void>;
setFetchPrune(prune: boolean): void;
pushBranch(remoteName: string, localBranch: string, remoteBranch: string, setUpstream: boolean): Promise<void>;
openPushDialog(localBranch: string): void;
closePushDialog(): void;
deleteRemoteBranch(remoteName: string, branchName: string): Promise<void>;
openDeleteRemoteBranchDialog(remoteName: string | null, branchName: string | null): void;
closeDeleteRemoteBranchDialog(): void;
```

- [ ] **Step 1: Write the failing lib tests**

`lib/graphSelection.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { withRenamedBranch, withoutBranch } from "./graphSelection";

describe("withoutBranch", () => {
  it("needs no write when nothing is selected or the branch is not in the selection", () => {
    expect(withoutBranch(null, "a", ["b"])).toBeNull();
    expect(withoutBranch(["b"], "a", ["b"])).toBeNull();
  });
  it("drops the deleted branch", () => {
    expect(withoutBranch(["a", "b"], "a", ["b", "c"])).toEqual(["b"]);
  });
  it("deleting the only selected branch shows all branches again", () => {
    expect(withoutBranch(["a"], "a", ["b", "c"])).toEqual(["b", "c"]);
  });
});

describe("withRenamedBranch", () => {
  it("renames in place and skips unrelated selections", () => {
    expect(withRenamedBranch(["a", "b"], "a", "z")).toEqual(["z", "b"]);
    expect(withRenamedBranch(["b"], "a", "z")).toBeNull();
    expect(withRenamedBranch(null, "a", "z")).toBeNull();
  });
});
```

`lib/fetchPrune.test.ts`:

```ts
import { afterEach, describe, expect, it, vi } from "vitest";
import type { RemoteInfo } from "../ipc/RepoClient";
import { effectiveFetchPrune, readFetchPrune, writeFetchPrune } from "./fetchPrune";

const remote = (name: string, prune?: boolean): RemoteInfo => ({
  name, fetchUrl: "x", pushUrl: null, authMode: null, authUsername: null, prune,
});

describe("fetch prune preference", () => {
  afterEach(() => {
    localStorage.clear();
    vi.restoreAllMocks();
  });

  it("is unset until written, per repo", () => {
    expect(readFetchPrune("/a")).toBeNull();
    writeFetchPrune("/a", true);
    writeFetchPrune("/b", false);
    expect(readFetchPrune("/a")).toBe(true);
    expect(readFetchPrune("/b")).toBe(false);
  });

  it("survives a storage that throws", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => { throw new Error("blocked"); });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => { throw new Error("blocked"); });
    expect(readFetchPrune("/a")).toBeNull();
    expect(() => writeFetchPrune("/a", true)).not.toThrow();
  });

  it("falls back to the fetch target remote's git config default", () => {
    const remotes = [remote("origin", false), remote("backup", true)];
    expect(effectiveFetchPrune(null, remotes, null)).toBe(false);
    expect(effectiveFetchPrune(null, remotes, { localBranch: "main", remoteName: "backup", remoteBranch: "main" })).toBe(true);
    expect(effectiveFetchPrune(true, remotes, null)).toBe(true);
    expect(effectiveFetchPrune(null, [remote("origin")], null)).toBe(false);
  });
});
```

- [ ] **Step 2: Write the failing state tests**

In `useAppState.test.ts`, reuse `transferClient(overrides)` and the `listener` capture pattern from "guides a non-fast-forward push…". Add a local helper:

```ts
function progress(operationId: string, operation: TransferProgress["operation"], phase: TransferProgress["phase"], extra: Partial<TransferProgress> = {}): TransferProgress {
  return { operationId, operation, phase, errorKind: null, current: 0, total: 0, receivedBytes: 0, message: null, ...extra };
}
```

Tests:

```ts
  it("names the failed remotes after a partial fetch all, once the refresh lands", async () => {
    let listener: ((p: TransferProgress) => void) | null = null;
    const client = transferClient({
      subscribeTransferProgress: (next) => { listener = next; return () => {}; },
      fetchAllRemotes: async () => {
        listener?.(progress("fetch-all-1", "FetchAll", "Starting"));
        listener?.(progress("fetch-all-1", "FetchAll", "RemoteFailed", { errorKind: "TransferFailed", remoteName: "backup" }));
        listener?.(progress("fetch-all-1", "FetchAll", "Completed"));
        return "fetch-all-1";
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.fetchAllRemotes());
    await act(async () => {});

    expect(result.current.state.error).toBe("Could not fetch from backup. The other remotes were fetched.");
    expect(result.current.state.transfer).toBeNull();
  });

  it("reports a fetch all where every remote failed", async () => {
    let listener: ((p: TransferProgress) => void) | null = null;
    const client = transferClient({
      subscribeTransferProgress: (next) => { listener = next; return () => {}; },
      fetchAllRemotes: async () => {
        listener?.(progress("fetch-all-2", "FetchAll", "Starting"));
        listener?.(progress("fetch-all-2", "FetchAll", "RemoteFailed", { errorKind: "TransferFailed", remoteName: "a" }));
        listener?.(progress("fetch-all-2", "FetchAll", "RemoteFailed", { errorKind: "MissingCredential", remoteName: "b" }));
        listener?.(progress("fetch-all-2", "FetchAll", "Failed", { errorKind: "TransferFailed" }));
        return "fetch-all-2";
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.fetchAllRemotes());

    expect(result.current.state.error).toBe("Could not fetch from a, b.");
  });

  it("passes the prune preference to every fetch once it is set, and null before", async () => {
    localStorage.clear();
    const fetchRemote = vi.fn(async () => "fetch-1");
    const fetchAllRemotes = vi.fn(async () => "fetch-all-1");
    const client = transferClient({ fetchRemote, fetchAllRemotes });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.fetchRemote("origin"));
    expect(fetchRemote).toHaveBeenLastCalledWith(TEST_REPO_PATH, "origin", null);

    act(() => result.current.setFetchPrune(true));
    await act(() => result.current.fetchRemote("origin"));
    await act(() => result.current.fetchAllRemotes());
    expect(fetchRemote).toHaveBeenLastCalledWith(TEST_REPO_PATH, "origin", true);
    expect(fetchAllRemotes).toHaveBeenLastCalledWith(TEST_REPO_PATH, true);
    expect(result.current.state.fetchPrune).toBe(true);
    expect(localStorage.getItem(`fetch-prune:${TEST_REPO_PATH}`)).toBe("on");
    localStorage.clear();
  });

  it("offers no force push after a rejected push of another branch", async () => {
    let listener: ((p: TransferProgress) => void) | null = null;
    const client = transferClient({
      listBranches: async () => [{ name: "main", isCurrent: true }, { name: "topic", isCurrent: false }],
      subscribeTransferProgress: (next) => { listener = next; return () => {}; },
      pushBranch: async () => {
        listener?.(progress("push-9", "PushBranch", "Starting"));
        listener?.(progress("push-9", "PushBranch", "Failed", { errorKind: "NonFastForward" }));
        return "push-9";
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.pushBranch("origin", "topic", "topic", false));

    expect(result.current.state.forcePushOffer).toBeNull();
    expect(result.current.state.error).toMatch(/Push was rejected/);
  });

  it("offers force push after a rejected same-name push of the current branch", async () => {
    let listener: ((p: TransferProgress) => void) | null = null;
    const client = transferClient({
      listBranches: async () => [{ name: "main", isCurrent: true }],
      getCurrentUpstream: async () => null,
      subscribeTransferProgress: (next) => { listener = next; return () => {}; },
      pushBranch: async () => {
        listener?.(progress("push-10", "PushBranch", "Starting"));
        listener?.(progress("push-10", "PushBranch", "Failed", { errorKind: "NonFastForward" }));
        return "push-10";
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.pushBranch("origin", "main", "main", true));

    expect(result.current.state.forcePushOffer).toEqual({ remoteName: "origin" });
  });

  it("closes the push and delete dialogs when their transfer starts", async () => {
    const client = transferClient({});
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    act(() => result.current.openPushDialog("topic"));
    expect(result.current.state.pushDialog).toEqual({ localBranch: "topic" });
    await act(() => result.current.pushBranch("origin", "topic", "topic", true));
    expect(result.current.state.pushDialog).toBeNull();

    act(() => result.current.openDeleteRemoteBranchDialog("origin", "topic"));
    expect(result.current.state.deleteRemoteBranchDialog).toEqual({ remoteName: "origin", branchName: "topic" });
    await act(() => result.current.deleteRemoteBranch("origin", "topic"));
    expect(result.current.state.deleteRemoteBranchDialog).toBeNull();
  });

  it("removes a deleted branch from the saved graph selection", async () => {
    const setGraphBranchSelection = vi.fn(async () => {});
    const client = transferClient({
      listBranches: async () => [{ name: "main", isCurrent: true }, { name: "topic", isCurrent: false }],
      getGraphBranchSelection: async () => ["main", "topic"],
      deleteBranch: async () => {},
      setGraphBranchSelection,
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.deleteBranch("topic", false));

    expect(setGraphBranchSelection).toHaveBeenCalledWith(TEST_REPO_PATH, ["main"]);
  });

  it("renames a branch inside the saved graph selection", async () => {
    const setGraphBranchSelection = vi.fn(async () => {});
    const client = transferClient({
      listBranches: async () => [{ name: "main", isCurrent: true }, { name: "topic", isCurrent: false }],
      getGraphBranchSelection: async () => ["topic"],
      renameBranch: async () => {},
      setGraphBranchSelection,
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.renameBranch("topic", "feature"));

    expect(setGraphBranchSelection).toHaveBeenCalledWith(TEST_REPO_PATH, ["feature"]);
  });
```

If `transferClient`'s base (`remoteManagementClient`) lacks a method these tests reach (`deleteBranch`, `renameBranch`, `getGraphBranchSelection`, `setGraphBranchSelection`), the override supplies it, as above. Check the base object and keep only the overrides that are needed.

- [ ] **Step 3: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/lib/fetchPrune src/lib/graphSelection src/state/useAppState`
Expected: FAIL (modules and actions missing).

- [ ] **Step 4: Implement the libs**

`lib/graphSelection.ts`:

```ts
/**
 * The persisted graph branch selection after deleting `name`, or `null` when it needs no write.
 * An emptied selection becomes `remaining` (every local branch left, i.e. "show all") so
 * isolating a branch and then deleting it never leaves an empty graph (#97).
 */
export function withoutBranch(selection: string[] | null, name: string, remaining: string[]): string[] | null {
  if (selection === null || !selection.includes(name)) return null;
  const next = selection.filter((candidate) => candidate !== name);
  return next.length > 0 ? next : remaining;
}

/** The selection after renaming `oldName`, or `null` when it needs no write. */
export function withRenamedBranch(selection: string[] | null, oldName: string, newName: string): string[] | null {
  if (selection === null || !selection.includes(oldName)) return null;
  return selection.map((candidate) => (candidate === oldName ? newName : candidate));
}
```

`lib/fetchPrune.ts`:

```ts
import type { RemoteInfo, UpstreamInfo } from "../ipc/RepoClient";

// Per repo, per machine: a UI preference, not repository state (see the 4b design).
const storageKey = (repoPath: string) => `fetch-prune:${repoPath}`;

/** The user's explicit Prune choice for this repo, or `null` when git config should decide. */
export function readFetchPrune(repoPath: string): boolean | null {
  try {
    const stored = localStorage.getItem(storageKey(repoPath));
    return stored === "on" ? true : stored === "off" ? false : null;
  } catch {
    return null;
  }
}

export function writeFetchPrune(repoPath: string, prune: boolean): void {
  try {
    localStorage.setItem(storageKey(repoPath), prune ? "on" : "off");
  } catch {
    // Blocked storage: the in-memory choice still applies for this session.
  }
}

/** What the Prune checkbox shows: the explicit choice, else the sync bar fetch target's git config. */
export function effectiveFetchPrune(override: boolean | null, remotes: RemoteInfo[], upstream: UpstreamInfo | null): boolean {
  if (override !== null) return override;
  const target = upstream?.remoteName ?? remotes[0]?.name;
  return remotes.find((remote) => remote.name === target)?.prune ?? false;
}
```

- [ ] **Step 5: Implement the state**

`useAppState.ts`:
- `AppState` gains the three fields from Interfaces, with comments.
- In the initial state, set `fetchPrune: readFetchPrune(repoPath), pushDialog: null, deleteRemoteBranchDialog: null`.
- Add the new methods to `UseAppStateResult` (next to `forcePushCurrentBranch`) and to both the destructuring of `useRemoteTransferActions(…)` and the returned object.
- Call `useRemoteTransferActions` with two extra trailing arguments: `state.fetchPrune` and `state.branches.find((branch) => branch.isCurrent)?.name ?? null`.
- Call `useBranchActions` with two extra trailing arguments: `state.graphBranchSelection` and `state.branches.map((branch) => branch.name)`.

`useBranchActions.ts`:
- New params `graphBranchSelection: string[] | null, branchNames: string[]`. Mirror both into refs (`useRef` + `useEffect` assignment, like `upstreamRef` in `useRemoteTransferActions`).
- `deleteBranch`'s mutate becomes:

```ts
        async () => {
          await client.deleteBranch(repoPath, name, force);
          const next = withoutBranch(
            selectionRef.current,
            name,
            branchNamesRef.current.filter((candidate) => candidate !== name),
          );
          if (next !== null) await client.setGraphBranchSelection(repoPath, next);
        },
```

- `renameBranch`'s mutate becomes:

```ts
        async () => {
          await client.renameBranch(repoPath, oldName, newName);
          const next = withRenamedBranch(selectionRef.current, oldName, newName);
          if (next !== null) await client.setGraphBranchSelection(repoPath, next);
        },
```

`useRemoteTransferActions.ts`:
- New params `fetchPrune: boolean | null, currentBranch: string | null`, mirrored into `fetchPruneRef` / `currentBranchRef`.
- `const failedRemotes = useRef<string[]>([]);`. Reset it to `[]` in `startTransfer` (next to `transferRequestPending.current = true`) and when a `Starting` event is accepted.
- A module-level helper:

```ts
function fetchAllFailureMessage(remotes: string[], othersFetched: boolean): string {
  const base = `Could not fetch from ${remotes.join(", ")}.`;
  return othersFetched ? `${base} The other remotes were fetched.` : base;
}
```

- In the subscription, after the `operationId` check:

```ts
      if (progress.phase === "RemoteFailed") {
        if (progress.remoteName !== undefined) failedRemotes.current = [...failedRemotes.current, progress.remoteName];
        return;
      }
```

- `Completed`: capture `const failed = failedRemotes.current; failedRemotes.current = [];` before `refresh()`. In the `.finally` `setState`, add `error: failed.length > 0 ? fetchAllFailureMessage(failed, true) : prev.error`, so it lands after `refresh()` has reset `error`, in the same update that clears `transfer`.
- `Failed`: compute the message as

```ts
        const failed = failedRemotes.current;
        failedRemotes.current = [];
        const message =
          progress.operation === "FetchAll" && progress.errorKind !== "Cancelled" && failed.length > 0
            ? fetchAllFailureMessage(failed, false)
            : transferFailureMessage(progress, upstreamLabel);
```

- `transferFailureMessage`:
  - Cancelled: `Fetch`/`FetchAll` → "Fetch cancelled.", `DeleteRemoteBranch` → "Delete cancelled.", else "Push cancelled.".
  - After the `StaleLease` branch, add:

```ts
  if (progress.operation === "DeleteRemoteBranch") {
    return progress.errorKind === "RejectedRemoteRef"
      ? "The remote refused to delete the branch."
      : "Deleting the remote branch failed";
  }
```

  - The final fetch line becomes `if (progress.operation === "Fetch" || progress.operation === "FetchAll") return "Fetch failed";`.
- Actions:

```ts
  const fetchRemote = useCallback(
    (remoteName: string) => startTransfer("Fetch", () => client.fetchRemote(repoPath, remoteName, fetchPruneRef.current)),
    [client, startTransfer, repoPath],
  );
  const fetchAllRemotes = useCallback(
    () => startTransfer("FetchAll", () => client.fetchAllRemotes(repoPath, fetchPruneRef.current)),
    [client, startTransfer, repoPath],
  );
  const setFetchPrune = useCallback(
    (prune: boolean) => {
      writeFetchPrune(repoPath, prune);
      fetchPruneRef.current = prune;
      setState((prev) => ({ ...prev, fetchPrune: prune }));
    },
    [repoPath, setState],
  );
  const pushBranch = useCallback(
    (remoteName: string, localBranch: string, remoteBranch: string, setUpstream: boolean) => {
      // The force-push offer force-pushes the *current* branch to where force_push_current_branch
      // resolves it: only offer it when that is exactly what this push targeted.
      const upstream = upstreamRef.current;
      const forceTarget = upstream !== null && upstream.remoteName === remoteName ? upstream.remoteBranch : localBranch;
      lastPushRemote.current =
        localBranch === currentBranchRef.current && remoteBranch === forceTarget ? remoteName : null;
      setState((prev) => ({ ...prev, pushDialog: null }));
      return startTransfer("PushBranch", () =>
        client.pushBranch(repoPath, remoteName, localBranch, remoteBranch, setUpstream),
      );
    },
    [client, startTransfer, repoPath, setState],
  );
  const deleteRemoteBranch = useCallback(
    (remoteName: string, branchName: string) => {
      setState((prev) => ({ ...prev, deleteRemoteBranchDialog: null }));
      return startTransfer("DeleteRemoteBranch", () => client.deleteRemoteBranch(repoPath, remoteName, branchName));
    },
    [client, startTransfer, repoPath, setState],
  );
```

  Add the four open/close setters (`setState((prev) => ({ ...prev, pushDialog: { localBranch } }))`, …). Add every new member to `RemoteTransferActions` and to the returned object.
- Fix the label for the force-push lease: in `forcePushCurrentBranch`, set `forcePushTarget.current.branch` to `upstreamRef.current.remoteBranch` when `upstreamRef.current?.remoteName === remoteName`, else to the current branch name, so a stale-lease message names the branch that was actually leased.

- [ ] **Step 6: Run tests, types and lint**

Run: `cd frontend && pnpm exec tsc --noEmit -p . && pnpm test -- --run && pnpm lint`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): fetch all, prune preference, push and delete-remote-branch state; prune graph selection on delete/rename (#97)"
```

---

### Task 9: `ContextMenu` checkbox items, `RefBadges`, and the tag badge menu

**Files:**
- Modify: `frontend/src/components/primitives/ContextMenu.tsx`, `ContextMenu.module.css`, `ContextMenu.test.tsx`
- Create: `frontend/src/components/RefBadges.tsx`, `RefBadges.test.tsx`
- Modify: `frontend/src/components/CommitGraph.tsx`, `CommitGraph.module.css`, `CommitGraph.test.tsx`, `FileHistoryList.tsx`, `CommitSearchResults.tsx`

**Interfaces:**
- Produces: `ContextMenuItem.checked?: boolean` (used by Task 11).

```tsx
export function RefBadges(props: {
  commit: GraphCommit;
  shownRemoteBranches?: ReadonlySet<string> | null; // null/undefined = show every remote badge
  onTagContextMenu?: (tag: string, event: MouseEvent<HTMLSpanElement>) => void;
}): JSX.Element;
```

- New optional `CommitGraph` props (used by Task 12): `remotes?: RemoteInfo[]`, `onPushTag?: (remoteName: string, tag: string) => void`, `onDeleteTag?: (tag: string) => Promise<void>`. Without `onPushTag`, tag badges have no menu.

- [ ] **Step 1: Write the failing tests**

`ContextMenu.test.tsx`:

```tsx
  it("renders checked items as menuitemcheckbox with aria-checked", () => {
    const onSelect = vi.fn();
    render(
      <ContextMenu x={0} y={0} onClose={() => {}} items={[
        { label: "Plain", onSelect: () => {} },
        { label: "Prune", checked: true, onSelect },
        { label: "Other", checked: false, onSelect: () => {} },
      ]} />,
    );
    expect(screen.getByRole("menuitem", { name: "Plain" })).toBeInTheDocument();
    expect(screen.getByRole("menuitemcheckbox", { name: "Prune" })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("menuitemcheckbox", { name: "Other" })).toHaveAttribute("aria-checked", "false");
    fireEvent.click(screen.getByRole("menuitemcheckbox", { name: "Prune" }));
    expect(onSelect).toHaveBeenCalled();
  });
```

`RefBadges.test.tsx`:

```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { GraphCommit } from "../ipc/RepoClient";
import { RefBadges } from "./RefBadges";

const commit: GraphCommit = {
  id: "a".repeat(40), shortId: "aaaaaaa", summary: "s", authorName: "A", authorEmail: "a@x",
  timestamp: 0, parentIds: [], branchRefs: ["main"], remoteBranchRefs: ["origin/main", "origin/old"],
  tags: ["v1.0", "v1.1"],
};

describe("RefBadges", () => {
  it("renders branch, filtered remote and tag badges", () => {
    render(<RefBadges commit={commit} shownRemoteBranches={new Set(["origin/main"])} />);
    expect(screen.getByText("main")).toBeInTheDocument();
    expect(screen.getByText("origin/main")).toBeInTheDocument();
    expect(screen.queryByText("origin/old")).toBeNull();
    expect(screen.getByTitle("Tag v1.0")).toHaveTextContent("v1.0");
    expect(screen.getByTitle("Tag v1.1")).toBeInTheDocument();
  });

  it("treats a commit without tags as having none", () => {
    render(<RefBadges commit={{ ...commit, tags: undefined }} />);
    expect(screen.queryByTitle(/^Tag /)).toBeNull();
  });

  it("reports a right-click on a tag and stops it reaching the row", () => {
    const onTag = vi.fn();
    const onRow = vi.fn();
    render(<div onContextMenu={onRow}><RefBadges commit={commit} onTagContextMenu={onTag} /></div>);
    fireEvent.contextMenu(screen.getByTitle("Tag v1.1"));
    expect(onTag).toHaveBeenCalledWith("v1.1", expect.anything());
    expect(onRow).not.toHaveBeenCalled();
  });
});
```

`CommitGraph.test.tsx`: add a `describe("tag badges", …)` that uses the existing `setup(overrides)` helper of the context-menu describe (about line 1017). The graph commit fixture there gets `tags: ["v2.0"]` through an override of `commits`.

```tsx
  it("opens a tag menu with push per remote, checkout, and a confirmed delete", async () => {
    const onPushTag = vi.fn();
    const onDeleteTag = vi.fn(async () => {});
    const onCheckoutCommit = vi.fn();
    const tagged = { ...commits[0], tags: ["v2.0"] };
    setup({
      commits: [tagged, ...commits.slice(1)],
      remotes: [
        { name: "origin", fetchUrl: "x", pushUrl: null, authMode: null, authUsername: null },
        { name: "backup", fetchUrl: "y", pushUrl: null, authMode: null, authUsername: null },
      ],
      onPushTag,
      onDeleteTag,
      onCheckoutCommit,
    });

    fireEvent.contextMenu(screen.getByTitle("Tag v2.0"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Push v2.0 to backup" }));
    expect(onPushTag).toHaveBeenCalledWith("backup", "v2.0");

    fireEvent.contextMenu(screen.getByTitle("Tag v2.0"));
    expect(screen.queryByRole("menuitem", { name: "Cherry-pick" })).toBeNull();
    fireEvent.click(screen.getByRole("menuitem", { name: "Checkout (detached)" }));
    expect(onCheckoutCommit).toHaveBeenCalledWith(tagged.id);

    fireEvent.contextMenu(screen.getByTitle("Tag v2.0"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Delete tag…" }));
    const dialog = screen.getByRole("dialog", { name: "Delete local tag v2.0" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete tag" }));
    await waitFor(() => expect(onDeleteTag).toHaveBeenCalledWith("v2.0"));
  });

  it("disables tag push without remotes", () => {
    setup({ commits: [{ ...commits[0], tags: ["v2.0"] }, ...commits.slice(1)], remotes: [], onPushTag: vi.fn(), onDeleteTag: vi.fn(async () => {}) });
    fireEvent.contextMenu(screen.getByTitle("Tag v2.0"));
    const push = screen.getByRole("menuitem", { name: "Push tag" });
    expect(push).toBeDisabled();
    expect(push).toHaveAttribute("title", "No remote configured.");
  });
```

Use whatever name the existing helper gives its commit fixture in place of `commits`, and import `within`/`waitFor` if the file does not already.

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/components/primitives/ContextMenu src/components/RefBadges src/components/CommitGraph`
Expected: FAIL.

- [ ] **Step 3: Implement**

- `ContextMenu.tsx`:
  - `ContextMenuItem` gains `/** When set, the item is a checkbox (menuitemcheckbox + aria-checked) with a check mark. */ checked?: boolean;`.
  - In the item button, use `role={item.checked === undefined ? "menuitem" : "menuitemcheckbox"}` and `aria-checked={item.checked}`, and render `{item.checked !== undefined && <Check size={12} aria-hidden="true" className={item.checked ? styles.check : styles.checkHidden} />}` before the label (`import { Check } from "lucide-react"`).
  - CSS: `.check { flex-shrink: 0; }` `.checkHidden { flex-shrink: 0; visibility: hidden; }`.
- `RefBadges.tsx`:

```tsx
import type { MouseEvent } from "react";
import { Tag } from "lucide-react";
import type { GraphCommit } from "../ipc/RepoClient";
import graphStyles from "./CommitGraph.module.css";

/** Branch, remote-branch and tag badges for one commit row (graph, file history, search). */
export function RefBadges({
  commit,
  shownRemoteBranches = null,
  onTagContextMenu,
}: {
  commit: GraphCommit;
  shownRemoteBranches?: ReadonlySet<string> | null;
  onTagContextMenu?: (tag: string, event: MouseEvent<HTMLSpanElement>) => void;
}) {
  const remoteRefs =
    shownRemoteBranches === null
      ? commit.remoteBranchRefs
      : commit.remoteBranchRefs.filter((ref) => shownRemoteBranches.has(ref));
  return (
    <>
      {commit.branchRefs.map((ref) => (
        <span key={`branch:${ref}`} className={graphStyles.branchBadge}>{ref}</span>
      ))}
      {remoteRefs.map((ref) => (
        <span key={`remote:${ref}`} className={graphStyles.remoteBranchBadge}>{ref}</span>
      ))}
      {(commit.tags ?? []).map((tag) => (
        <span
          key={`tag:${tag}`}
          className={graphStyles.tagBadge}
          title={`Tag ${tag}`}
          onContextMenu={
            onTagContextMenu === undefined
              ? undefined
              : (event) => {
                  // The row's own context menu (commit actions) must not open as well.
                  event.preventDefault();
                  event.stopPropagation();
                  onTagContextMenu(tag, event);
                }
          }
        >
          <Tag size={10} aria-hidden="true" />
          {tag}
        </span>
      ))}
    </>
  );
}
```

- `CommitGraph.module.css`:

```css
/* Outlined with a tag icon, so tags never read as branches. */
.tagBadge {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  font-size: var(--text-xs);
  line-height: 1.5;
  padding: 0 5px;
  border: 1px solid currentColor;
  border-radius: var(--radius-sm);
  color: var(--color-text-muted);
  flex-shrink: 0;
}
```

- `CommitGraph.tsx`:
  - New optional props with doc comments (tag push/delete/checkout, `TagPanel` stays the keyboard path).
  - State `tagMenu: { tag: string; commitId: string; x: number; y: number } | null` and `tagToDelete: string | null`.
  - Replace the two badge `.map`s with `<RefBadges commit={commit} shownRemoteBranches={shownRemoteBranches} onTagContextMenu={onPushTag === undefined ? undefined : (tag, event) => setTagMenu({ tag, commitId: commit.id, x: event.clientX, y: event.clientY })} />`.
  - Add:

```tsx
  function tagItems(tag: string, commitId: string): ContextMenuItem[] {
    const push: ContextMenuItem[] =
      remotes.length === 0
        ? [{ label: "Push tag", onSelect: () => {}, disabled: true, title: "No remote configured." }]
        : remotes.map((remote) => ({
            label: `Push ${tag} to ${remote.name}`,
            onSelect: () => onPushTag?.(remote.name, tag),
            disabled: pending,
          }));
    return [
      ...push,
      { label: "Checkout (detached)", onSelect: () => onCheckoutCommit(commitId), disabled: pending },
      { label: "Delete tag…", destructive: true, onSelect: () => setTagToDelete(tag), disabled: pending },
    ];
  }
```

  - Render a second `ContextMenu` for `tagMenu`, with `onClose` like the commit menu (refocus the list), and the confirm dialog next to `ResetDialog`:

```tsx
    {tagToDelete !== null && (
      <ConfirmDialog
        ariaLabel={`Delete local tag ${tagToDelete}`}
        message={`Delete local tag ${tagToDelete}?`}
        confirmLabel="Delete tag"
        confirmDisabled={pending}
        onConfirm={() => void onDeleteTag?.(tagToDelete).then(() => setTagToDelete(null))}
        onCancel={() => setTagToDelete(null)}
      />
    )}
```

  Default `remotes = []` in the destructuring.
- `FileHistoryList.tsx` and `CommitSearchResults.tsx`: replace their two badge `.map`s with `<RefBadges commit={entry.commit} />` / `<RefBadges commit={commit} />`. Tags then show there too, without a menu.

- [ ] **Step 4: Run tests and lint**

Run: `cd frontend && pnpm test -- --run && pnpm lint`
Expected: PASS (existing FileHistoryList/CommitSearchResults/CommitGraph badge tests still pass).

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): tag badges on graph rows with push, checkout and delete menu"
```

---

### Task 10: `PushBranchDialog` and `DeleteRemoteBranchDialog`

**Files:**
- Create: `frontend/src/components/PushBranchDialog.tsx`, `PushBranchDialog.module.css`, `PushBranchDialog.test.tsx`
- Create: `frontend/src/components/DeleteRemoteBranchDialog.tsx`, `DeleteRemoteBranchDialog.test.tsx` (reuse `PushBranchDialog.module.css` for layout, `primitives/ConfirmDialog.module.css` for the danger button)

**Interfaces:**

```tsx
export function PushBranchDialog(props: {
  localBranch: string;
  remotes: RemoteInfo[];
  /** `BranchInfo.upstream` of `localBranch` ("origin/main"), or null/undefined without one. */
  branchUpstream: string | null | undefined;
  onPush: (remoteName: string, remoteBranch: string, setUpstream: boolean) => void;
  onCancel: () => void;
}): JSX.Element;

export function DeleteRemoteBranchDialog(props: {
  remotes: RemoteInfo[];
  initialRemote: string | null;
  initialBranch: string | null; // both set = confirm-only mode
  onListRemoteBranches: (remoteName: string) => Promise<string[]>;
  onDelete: (remoteName: string, branchName: string) => void;
  onCancel: () => void;
}): JSX.Element;
```

- [ ] **Step 1: Write the failing tests**

`PushBranchDialog.test.tsx`:

```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RemoteInfo } from "../ipc/RepoClient";
import { PushBranchDialog } from "./PushBranchDialog";

const remote = (name: string): RemoteInfo => ({ name, fetchUrl: name, pushUrl: null, authMode: null, authUsername: null });

describe("PushBranchDialog", () => {
  it("defaults to origin, the local name, and Set as upstream when the branch has none", () => {
    const onPush = vi.fn();
    render(<PushBranchDialog localBranch="topic" remotes={[remote("backup"), remote("origin")]} branchUpstream={null} onPush={onPush} onCancel={() => {}} />);

    expect(screen.getByRole("dialog", { name: "Push topic" })).toBeInTheDocument();
    expect(screen.getByLabelText("Remote")).toHaveValue("origin");
    expect(screen.getByLabelText("Remote branch")).toHaveValue("topic");
    expect(screen.getByLabelText("Set as upstream")).toBeChecked();
    fireEvent.click(screen.getByRole("button", { name: "Push" }));
    expect(onPush).toHaveBeenCalledWith("origin", "topic", true);
  });

  it("starts on the branch's upstream remote, unchecked, and pushes the edited name", () => {
    const onPush = vi.fn();
    render(<PushBranchDialog localBranch="topic" remotes={[remote("origin"), remote("backup")]} branchUpstream="backup/topic" onPush={onPush} onCancel={() => {}} />);

    expect(screen.getByLabelText("Remote")).toHaveValue("backup");
    expect(screen.getByLabelText("Set as upstream")).not.toBeChecked();
    fireEvent.change(screen.getByLabelText("Remote branch"), { target: { value: "  renamed " } });
    fireEvent.click(screen.getByRole("button", { name: "Push" }));
    expect(onPush).toHaveBeenCalledWith("backup", "renamed", false);
  });

  it("blocks an empty or invalid remote branch name", () => {
    const onPush = vi.fn();
    render(<PushBranchDialog localBranch="topic" remotes={[remote("origin")]} branchUpstream={null} onPush={onPush} onCancel={() => {}} />);

    fireEvent.change(screen.getByLabelText("Remote branch"), { target: { value: "bad name" } });
    expect(screen.getByText("Branch names can't contain spaces.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Push" })).toBeDisabled();
    fireEvent.change(screen.getByLabelText("Remote branch"), { target: { value: " " } });
    expect(screen.getByRole("button", { name: "Push" })).toBeDisabled();
    fireEvent.submit(screen.getByRole("button", { name: "Push" }).closest("form")!);
    expect(onPush).not.toHaveBeenCalled();
  });
});
```

`DeleteRemoteBranchDialog.test.tsx`:

```tsx
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RemoteInfo } from "../ipc/RepoClient";
import { DeleteRemoteBranchDialog } from "./DeleteRemoteBranchDialog";

const remote = (name: string): RemoteInfo => ({ name, fetchUrl: name, pushUrl: null, authMode: null, authUsername: null });

describe("DeleteRemoteBranchDialog", () => {
  it("confirms a preset branch without listing anything", () => {
    const onDelete = vi.fn();
    const onList = vi.fn(async () => []);
    render(<DeleteRemoteBranchDialog remotes={[remote("origin")]} initialRemote="origin" initialBranch="topic" onListRemoteBranches={onList} onDelete={onDelete} onCancel={() => {}} />);

    expect(screen.getByRole("dialog", { name: "Delete remote branch" })).toHaveTextContent(
      "Delete origin/topic on the remote? This can't be undone.",
    );
    expect(screen.queryByLabelText("Branch")).toBeNull();
    expect(onList).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus();
    fireEvent.click(screen.getByRole("button", { name: "Delete remote branch" }));
    expect(onDelete).toHaveBeenCalledWith("origin", "topic");
  });

  it("lets the palette pick a remote and one of its branches", async () => {
    const onDelete = vi.fn();
    const onList = vi.fn(async (name: string) => (name === "origin" ? ["main", "topic"] : ["mirror-only"]));
    render(<DeleteRemoteBranchDialog remotes={[remote("origin"), remote("backup")]} initialRemote={null} initialBranch={null} onListRemoteBranches={onList} onDelete={onDelete} onCancel={() => {}} />);

    await waitFor(() => expect(screen.getByLabelText("Branch")).toHaveValue("main"));
    fireEvent.change(screen.getByLabelText("Branch"), { target: { value: "topic" } });
    fireEvent.click(screen.getByRole("button", { name: "Delete remote branch" }));
    expect(onDelete).toHaveBeenCalledWith("origin", "topic");

    fireEvent.change(screen.getByLabelText("Remote"), { target: { value: "backup" } });
    await waitFor(() => expect(screen.getByLabelText("Branch")).toHaveValue("mirror-only"));
  });

  it("shows a load failure and keeps Delete disabled", async () => {
    render(<DeleteRemoteBranchDialog remotes={[remote("origin")]} initialRemote={null} initialBranch={null} onListRemoteBranches={async () => { throw new Error("x"); }} onDelete={() => {}} onCancel={() => {}} />);

    expect(await screen.findByText("Could not list branches for origin.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Delete remote branch" })).toBeDisabled();
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/components/PushBranchDialog src/components/DeleteRemoteBranchDialog`
Expected: FAIL (modules missing).

- [ ] **Step 3: Implement**

`PushBranchDialog.tsx` (on `FormDialog` + `Field` primitives):

```tsx
import { useState } from "react";
import type { RemoteInfo } from "../ipc/RepoClient";
import { branchNameProblem } from "../lib/refName";
import { Field } from "./primitives/Field";
import { FormDialog } from "./primitives/FormDialog";
import styles from "./PushBranchDialog.module.css";

function defaultRemote(remotes: RemoteInfo[], branchUpstream: string | null | undefined): string {
  const upstreamRemote =
    branchUpstream == null ? undefined : remotes.find((remote) => branchUpstream.startsWith(`${remote.name}/`));
  return upstreamRemote?.name ?? (remotes.some((remote) => remote.name === "origin") ? "origin" : (remotes[0]?.name ?? ""));
}

/** "Push to…": any local branch to any remote branch, optionally becoming its upstream. */
export function PushBranchDialog({
  localBranch,
  remotes,
  branchUpstream,
  onPush,
  onCancel,
}: {
  localBranch: string;
  remotes: RemoteInfo[];
  /** `BranchInfo.upstream` of `localBranch` ("origin/main"), or null/undefined without one. */
  branchUpstream: string | null | undefined;
  onPush: (remoteName: string, remoteBranch: string, setUpstream: boolean) => void;
  onCancel: () => void;
}) {
  const [remoteName, setRemoteName] = useState(() => defaultRemote(remotes, branchUpstream));
  const [remoteBranch, setRemoteBranch] = useState(localBranch);
  const [setUpstream, setSetUpstream] = useState(branchUpstream == null);
  const trimmed = remoteBranch.trim();
  const problem = branchNameProblem(trimmed);
  const blocked = trimmed === "" || problem !== null || remoteName === "";

  return (
    <FormDialog ariaLabel={`Push ${localBranch}`} onCancel={onCancel}>
      <form
        className={styles.form}
        onSubmit={(event) => {
          event.preventDefault();
          if (!blocked) onPush(remoteName, trimmed, setUpstream);
        }}
      >
        <Field label="Remote">
          {(props) => (
            <select {...props} value={remoteName} onChange={(event) => setRemoteName(event.target.value)}>
              {remotes.map((remote) => <option key={remote.name} value={remote.name}>{remote.name}</option>)}
            </select>
          )}
        </Field>
        <Field label="Remote branch" error={problem}>
          {(props) => <input {...props} value={remoteBranch} onChange={(event) => setRemoteBranch(event.target.value)} />}
        </Field>
        <label className={styles.checkbox}>
          <input type="checkbox" checked={setUpstream} onChange={(event) => setSetUpstream(event.target.checked)} />
          Set as upstream
        </label>
        <div className={styles.actions}>
          <button type="submit" disabled={blocked}>Push</button>
          <button type="button" onClick={onCancel}>Cancel</button>
        </div>
      </form>
    </FormDialog>
  );
}
```

`Field` renders the `error` text itself. Check `Field.tsx`: if it renders the error, the "Branch names can't contain spaces." assertion passes as written. Otherwise render `{problem !== null && <p className={styles.problem}>{problem}</p>}` under the input.

`PushBranchDialog.module.css`: `.form { display: flex; flex-direction: column; gap: var(--space-3); }`, `.checkbox { display: inline-flex; align-items: center; gap: var(--space-2); }`, `.actions { display: flex; gap: var(--space-2); }`, `.problem { margin: 0; color: var(--color-danger-text); }`.

`DeleteRemoteBranchDialog.tsx`:
- `const preset = initialRemote !== null && initialBranch !== null;`
- State: `remoteName` (initial `initialRemote ?? remotes[0]?.name ?? ""`), `branchName` (initial `initialBranch ?? ""`), `options: string[] | null` and `loadError: string | null`.
- Load the list in an effect keyed on `[preset, remoteName, onListRemoteBranches]`. Set state **only** inside the promise callbacks, behind a `cancelled` flag, to satisfy `react-hooks/set-state-in-effect`. On success: `setOptions(names); setBranchName(names[0] ?? ""); setLoadError(null)`. On failure: `setLoadError(\`Could not list branches for ${remoteName}.\`)`.
- The Remote select's `onChange` resets `options` to `null`, `branchName` to `""` and `loadError` to `null` before calling `setRemoteName`.
- Render `FormDialog ariaLabel="Delete remote branch"`.
  - Picker mode: a `Remote` `Field` select with `data-autofocus`, and a `Branch` `Field` select (options from `options`, disabled while `options === null`). Show `loadError` in an `InlineError`-free `<p className={styles.problem}>`.
  - Then `<p>Delete {remoteName}/{branchName} on the remote? This can't be undone.</p>`, but only once `branchName !== ""`.
  - Buttons: `<button type="button" className={confirmStyles.confirmButton} disabled={branchName === ""} onClick={() => onDelete(remoteName, branchName)}>Delete remote branch</button>` and `<button type="button" data-autofocus={preset ? true : undefined} onClick={onCancel}>Cancel</button>`, with `import confirmStyles from "./primitives/ConfirmDialog.module.css"`.

- [ ] **Step 4: Run tests and lint**

Run: `cd frontend && pnpm test -- --run && pnpm lint`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components
git commit -m "feat(frontend): Push to… and Delete remote branch dialogs"
```

---

### Task 11: Branch tree items, counts and cache reload; split Fetch and publish in the sync bar

**Files:**
- Modify: `frontend/src/components/BranchTree.tsx`, `BranchTree.module.css`, `BranchTree.test.tsx`
- Modify: `frontend/src/components/SyncBar.tsx`, `SyncBar.module.css`, `SyncBar.test.tsx`

**Interfaces:**
- New required `BranchTree` props (wired in Task 12): `onOpenPushDialog: (localBranch: string) => void`, `onOpenDeleteRemoteBranch: (remoteName: string, branchName: string) => void`, `refreshGeneration: number`.
- New required `SyncBar` props: `prune: boolean`, `onFetchAll: () => void | Promise<void>`, `onTogglePrune: (prune: boolean) => void`, `onPublish: () => void`.

- [ ] **Step 1: Write the failing tests**

`BranchTree.test.tsx`: add the three props to `renderTree`'s defaults (`onOpenPushDialog: vi.fn()`, `onOpenDeleteRemoteBranch: vi.fn()`, `refreshGeneration: 0`). `renderTree` already returns `{ ...render(…), props }`, so `rerender` is available. Then:

```tsx
const originOnly = [{ name: "origin", fetchUrl: "x", pushUrl: null, authMode: null, authUsername: null }];

describe("BranchTree — refs and remotes (4b)", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("shows only the non-zero ahead/behind parts with an accessible label", () => {
    renderTree({
      branches: [
        { name: "main", isCurrent: true, upstream: "origin/main", ahead: 2, behind: 0 },
        { name: "synced", isCurrent: false, upstream: "origin/synced", ahead: 0, behind: 0 },
        { name: "never-fetched", isCurrent: false, upstream: "origin/x", ahead: null, behind: null },
      ],
    });
    const counts = screen.getByLabelText("2 ahead, 0 behind origin/main");
    expect(counts).toHaveTextContent("↑2");
    expect(counts).not.toHaveTextContent("↓");
    expect(screen.queryByLabelText(/behind origin\/synced/)).toBeNull();
    expect(screen.queryByLabelText(/behind origin\/x/)).toBeNull();
  });

  it("offers Push to… on a non-current local branch", () => {
    const { props } = renderTree({ remotes: originOnly });
    fireEvent.click(screen.getByRole("button", { name: "Actions for feat/foo" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Push to…" }));
    expect(props.onOpenPushDialog).toHaveBeenCalledWith("feat/foo");
  });

  it("disables Push to… without a remote", () => {
    renderTree({ remotes: [] });
    fireEvent.click(screen.getByRole("button", { name: "Actions for feat/foo" }));
    expect(screen.getByRole("menuitem", { name: "Push to…" })).toBeDisabled();
  });

  it("offers a destructive Delete <remote>/<branch>… on remote branches", async () => {
    const { props } = renderTree({
      remotes: originOnly,
      onListRemoteBranches: vi.fn().mockResolvedValue(["topic"]),
    });
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    const row = (await screen.findByRole("button", { name: "Show origin/topic in graph" })).closest("li")!;
    fireEvent.contextMenu(row);
    fireEvent.click(screen.getByRole("menuitem", { name: "Delete origin/topic…" }));
    expect(props.onOpenDeleteRemoteBranch).toHaveBeenCalledWith("origin", "topic");
  });

  it("reloads open remote folders when refreshGeneration changes", async () => {
    const onListRemoteBranches = vi
      .fn()
      .mockResolvedValueOnce(["main", "topic"])
      .mockResolvedValueOnce(["main"]);
    const { props, rerender } = renderTree({ remotes: originOnly, onListRemoteBranches });
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    expect(await screen.findByRole("button", { name: "Show origin/topic in graph" })).toBeInTheDocument();

    rerender(<BranchTree {...props} refreshGeneration={1} />);

    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "Show origin/topic in graph" })).toBeNull(),
    );
    expect(onListRemoteBranches).toHaveBeenCalledTimes(2);
  });

  it("forgets a closed folder's list on refresh so the next expand reloads it", async () => {
    const onListRemoteBranches = vi.fn().mockResolvedValue(["main"]);
    const { props, rerender } = renderTree({ remotes: originOnly, onListRemoteBranches });
    fireEvent.click(screen.getByRole("button", { name: "origin" })); // expand
    await screen.findByRole("button", { name: "Show origin/main in graph" });
    fireEvent.click(screen.getByRole("button", { name: "origin" })); // collapse

    rerender(<BranchTree {...props} refreshGeneration={1} />);
    expect(onListRemoteBranches).toHaveBeenCalledOnce(); // closed folders are not reloaded eagerly
    fireEvent.click(screen.getByRole("button", { name: "origin" })); // expand again

    await waitFor(() => expect(onListRemoteBranches).toHaveBeenCalledTimes(2));
  });
});
```

The existing "does not re-fetch remote branches on a second expand" test stays valid: without a refresh in between, the cache still serves a re-expand.

`SyncBar.test.tsx`: add `prune: false, onFetchAll: vi.fn(), onTogglePrune: vi.fn(), onPublish: vi.fn()` to `renderBar`'s defaults, then:

```tsx
  it("opens fetch actions: Fetch all remotes and the Prune checkbox", () => {
    const props = renderBar({ prune: true });
    fireEvent.click(screen.getByRole("button", { name: "More fetch actions" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Fetch all remotes" }));
    expect(props.onFetchAll).toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "More fetch actions" }));
    const prune = screen.getByRole("menuitemcheckbox", { name: "Prune" });
    expect(prune).toHaveAttribute("aria-checked", "true");
    fireEvent.click(prune);
    expect(props.onTogglePrune).toHaveBeenCalledWith(false);
  });

  it("disables fetch actions without a remote", () => {
    renderBar({ remotes: [], upstream: null });
    expect(screen.getByRole("button", { name: "More fetch actions" })).toBeDisabled();
  });

  it("publishes through the push dialog when the branch has no upstream", () => {
    const props = renderBar({ upstream: null });
    const push = screen.getByRole("button", { name: "Push" });
    expect(push).toBeEnabled();
    expect(push).toHaveAttribute("title", "Push this branch to a remote and set its upstream…");
    fireEvent.click(push);
    expect(props.onPublish).toHaveBeenCalled();
    expect(props.onPush).not.toHaveBeenCalled();
  });

  it("keeps Push disabled without upstream when there is no remote or HEAD is detached", () => {
    renderBar({ upstream: null, remotes: [] });
    expect(screen.getByRole("button", { name: "Push" })).toBeDisabled();
  });
```

The existing tests in this file that assert Push is disabled without an upstream contradict the new behavior. Update each one to the `remotes: []` case above, and keep the detached case as it is.

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/components/BranchTree src/components/SyncBar`
Expected: FAIL.

- [ ] **Step 3: Implement `BranchTree`**

- Destructure the three new props and add their types with comments.
- Counts, rendered after the name `<span>` when not renaming:

```tsx
function aheadBehind(branch: BranchInfo): { ahead: number; behind: number; label: string } | null {
  const ahead = branch.ahead ?? null;
  const behind = branch.behind ?? null;
  if (branch.upstream == null || ahead === null || behind === null || (ahead === 0 && behind === 0)) return null;
  return { ahead, behind, label: `${ahead} ahead, ${behind} behind ${branch.upstream}` };
}
```

```tsx
            {!isRenaming && counts !== null && (
              <span className={styles.aheadBehind} aria-label={counts.label} title={counts.label}>
                {counts.ahead > 0 && <span aria-hidden="true">↑{counts.ahead}</span>}
                {counts.behind > 0 && <span aria-hidden="true">↓{counts.behind}</span>}
              </span>
            )}
```

  Here `const counts = aheadBehind(branch);` sits next to `shownInGraph`. CSS: `.aheadBehind { display: inline-flex; gap: var(--space-1); font-family: var(--font-mono); font-size: var(--text-xs); color: var(--color-text-muted); flex-shrink: 0; }`. If `--space-1` does not exist in the theme tokens, use `2px`.

  `toHaveTextContent` reads `aria-hidden` children too, so the test's `↑2` check holds.
- `branchContextItems`: before "Isolate branch", push:

```ts
    items.push({
      label: "Push to…",
      disabled: operationDisabled || remotes.length === 0,
      title: operationDisabled ? (operationDisabledReason ?? undefined) : remotes.length === 0 ? "No remote configured." : undefined,
      onSelect: () => onOpenPushDialog(branch.name),
    });
```

- `remoteBranchItems`: append

```ts
      {
        label: `Delete ${remoteName}/${branchName}…`,
        destructive: true,
        disabled: operationDisabled,
        title: operationDisabled ? (operationDisabledReason ?? undefined) : undefined,
        onSelect: () => onOpenDeleteRemoteBranch(remoteName, branchName),
      },
```

- Cache freshness. When `refreshGeneration` changes, keep the lists of open folders on screen (they reload in place) and forget the closed ones, so their next expand reloads. Detect the change during render, the way `TagPanel` tracks `previousTags`, so no `setState` runs synchronously inside an effect:

```tsx
  // The remote-branch lists are a cache of `listRemoteBranches`. Any refresh (a fetch from the
  // sync bar or palette, a push that published a branch, a remote branch delete) may have changed
  // them.
  const [cacheGeneration, setCacheGeneration] = useState(refreshGeneration);
  if (cacheGeneration !== refreshGeneration) {
    setCacheGeneration(refreshGeneration);
    setRemoteBranches((prev) => Object.fromEntries(Object.entries(prev).filter(([name]) => isRemoteOpen(name))));
  }
  const mountGeneration = useRef(refreshGeneration);
  useEffect(() => {
    if (refreshGeneration === mountGeneration.current) return;
    // Open folders reload in place, without a "Loading…" flash.
    for (const remote of remotes) {
      if (isRemoteOpen(remote.name)) fetchRemoteBranchList(remote.name);
    }
    // Only a new refresh triggers this; opening a folder has its own load path.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refreshGeneration]);
```

  Place this after `isRemoteOpen` and `fetchRemoteBranchList` are defined. Move those two helpers above it if needed.
- The comment on the folder-menu Fetch item ("never otherwise invalidated") is now wrong. Shorten it to say the explicit reload just makes the update immediate.

- [ ] **Step 4: Implement `SyncBar`**

- Destructure the new props. Add `const [fetchMenu, setFetchMenu] = useState<{ x: number; y: number } | null>(null);`.
- After the Fetch button:

```tsx
        <button
          type="button"
          className={styles.split}
          aria-label="More fetch actions"
          aria-haspopup="menu"
          aria-expanded={fetchMenu !== null}
          disabled={operationDisabled || fetchRemote === null}
          title={busyTitle}
          onClick={(event) => {
            const rect = event.currentTarget.getBoundingClientRect();
            setFetchMenu({ x: rect.left, y: rect.bottom });
          }}
        >
          <ChevronDown size={12} aria-hidden="true" />
        </button>
```

  …and after the `Toolbar`:

```tsx
      {fetchMenu !== null && (
        <ContextMenu
          x={fetchMenu.x}
          y={fetchMenu.y}
          onClose={() => setFetchMenu(null)}
          items={[
            { label: "Fetch all remotes", onSelect: () => void onFetchAll() },
            {
              label: "Prune",
              checked: prune,
              title: "Remove remote-tracking branches that were deleted on the remote",
              onSelect: () => onTogglePrune(!prune),
            },
          ]}
        />
      )}
```

- Push:

```tsx
  const canPublish = !detached && upstream === null && remotes.length > 0;
  const pushTitle = detached
    ? detachedTitle
    : upstream !== null
      ? upstreamTitle("Push to")
      : (busyTitle ?? (canPublish ? "Push this branch to a remote and set its upstream…" : "No remote configured."));
```

  Then `disabled={operationDisabled || detached || (upstream === null && !canPublish)}`, `title={pushTitle}` and `onClick={() => (upstream !== null ? void onPush(upstream.remoteName) : onPublish())}`.
- Update the component doc comment: Push without upstream opens the Push dialog.
- CSS: `.split { padding: 2px 4px; }`.

- [ ] **Step 5: Run tests, types and lint**

Run: `cd frontend && pnpm exec tsc --noEmit -p . && pnpm test -- --run && pnpm lint`
Expected: the component tests PASS. `tsc` reports the missing props at `App.tsx`'s `<BranchTree>` / `<SyncBar>`. Pass placeholders there now (`onOpenPushDialog={appState.openPushDialog}`, `onOpenDeleteRemoteBranch={appState.openDeleteRemoteBranchDialog}`, `refreshGeneration={appState.state.refreshGeneration}`, `prune={false}`, `onFetchAll={appState.fetchAllRemotes}`, `onTogglePrune={appState.setFetchPrune}`, `onPublish={() => {}}`). Task 12 finishes the wiring.

- [ ] **Step 6: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): branch tree Push to…, remote branch delete, ahead/behind; split Fetch and publish in the sync bar"
```

---

### Task 12: App wiring and palette entries

**Files:**
- Modify: `frontend/src/App.tsx`, `frontend/src/App.test.tsx`, `frontend/src/lib/commands.ts`, `frontend/src/lib/commands.test.ts`

**Interfaces:**
- Consumes: everything from Tasks 8–11.

- [ ] **Step 1: Write the failing tests**

`commands.test.ts` (uses `makeAppState(overrides)`):

```ts
  it("offers Fetch all remotes, Push branch to… and Delete remote branch… when a remote exists", () => {
    const fetchAllRemotes = vi.fn(async () => {});
    const openPushDialog = vi.fn();
    const openDeleteRemoteBranchDialog = vi.fn();
    const appState = makeAppState({
      state: {
        remotes: [{ name: "origin", fetchUrl: "x", pushUrl: null, authMode: null, authUsername: null }],
        branches: [{ name: "main", isCurrent: true }],
      },
      fetchAllRemotes,
      openPushDialog,
      openDeleteRemoteBranchDialog,
    });
    const commands = buildCommands(appState);
    commands.find((command) => command.label === "Fetch all remotes")!.run();
    commands.find((command) => command.label === "Push branch to…")!.run();
    commands.find((command) => command.label === "Delete remote branch…")!.run();
    expect(fetchAllRemotes).toHaveBeenCalled();
    expect(openPushDialog).toHaveBeenCalledWith("main");
    expect(openDeleteRemoteBranchDialog).toHaveBeenCalledWith(null, null);
  });

  it("hides the new remote commands without a remote, and Push branch to… on a detached HEAD", () => {
    const labels = (state: Parameters<typeof makeAppState>[0]) =>
      buildCommands(makeAppState(state)).map((command) => command.label);
    expect(labels({ state: { remotes: [] } })).not.toContain("Fetch all remotes");
    expect(labels({ state: { remotes: [] } })).not.toContain("Delete remote branch…");
    expect(
      labels({
        state: {
          remotes: [{ name: "origin", fetchUrl: "x", pushUrl: null, authMode: null, authUsername: null }],
          branches: [],
          headInfo: { headId: "abc", detachedAt: "abc1234", headPushed: false, operation: null },
        },
      }),
    ).not.toContain("Push branch to…");
  });
```

Adapt the override shape to `makeAppState`'s real signature (it may merge `state` partially). Read it first.

`App.test.tsx`, modelled on "opens a working-tree file's history in place of the graph and closes it again" (same repo bootstrap overrides):

```tsx
  it("publishes the current branch from the sync bar through the Push dialog", async () => {
    const pushBranch = vi.fn(async () => "push-1");
    const client = fakeClient({
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [{ name: "main", isCurrent: true, upstream: null, ahead: null, behind: null }],
      listStashes: async () => [],
      listRemotes: async () => [{ name: "origin", fetchUrl: "x", pushUrl: null, authMode: null, authUsername: null, prune: false }],
      getCurrentUpstream: async () => null,
      pushBranch,
    });
    render(<App client={client} />);

    const push = await screen.findByRole("button", { name: "Push" });
    await waitFor(() => expect(push).toBeEnabled());
    fireEvent.click(push);
    const dialog = await screen.findByRole("dialog", { name: "Push main" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Push" }));

    await waitFor(() => expect(pushBranch).toHaveBeenCalledWith("/repos/a", "origin", "main", "main", true));
    expect(screen.queryByRole("dialog", { name: "Push main" })).toBeNull();
  });
```

Copy any further overrides the model test needs (for example `listWorktrees`, `getGraphBranchSelection`) from that test, and render `App` the same way it does.

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/lib/commands src/App`
Expected: FAIL.

- [ ] **Step 3: Implement**

`commands.ts`: inside the `if (!repositoryOperationDisabled)` block, before the `for (const remote of state.remotes)` loop:

```ts
    if (state.remotes.length > 0) {
      commands.push({
        id: "fetch-all-remotes",
        label: "Fetch all remotes",
        keywords: ["fetch", "all", "remotes", "prune"],
        run: () => void appState.fetchAllRemotes(),
      });
      const current = state.branches.find((branch) => branch.isCurrent);
      if (current !== undefined && state.headInfo.detachedAt === null) {
        commands.push({
          id: "push-branch-to",
          label: "Push branch to…",
          keywords: ["push", "publish", "upstream", "remote", "branch"],
          run: () => appState.openPushDialog(current.name),
        });
      }
      commands.push({
        id: "delete-remote-branch",
        label: "Delete remote branch…",
        keywords: ["delete", "remove", "remote", "branch"],
        run: () => appState.openDeleteRemoteBranchDialog(null, null),
      });
    }
```

`App.tsx`:
- `SyncBar`:
  - `prune={effectiveFetchPrune(appState.state.fetchPrune, appState.state.remotes, appState.state.upstream)}`
  - `onFetchAll={announce("Fetch complete", appState.fetchAllRemotes)}`
  - `onTogglePrune={appState.setFetchPrune}`
  - `onPublish={() => { const current = appState.state.branches.find((branch) => branch.isCurrent); if (current !== undefined) appState.openPushDialog(current.name); }}`
- `BranchTree`: the three props from Task 11 (keep them).
- `CommitGraph`:
  - `remotes={appState.state.remotes}`
  - `onPushTag={(remoteName, tag) => void announce("Push complete", appState.pushTags)(remoteName, [tag])}`
  - `onDeleteTag={appState.deleteTag}`
- Next to the force-push confirm (inside the same `active &&` guard):

```tsx
      {active && appState.state.pushDialog !== null && (() => {
        const localBranch = appState.state.pushDialog.localBranch;
        return (
          <PushBranchDialog
            localBranch={localBranch}
            remotes={appState.state.remotes}
            branchUpstream={appState.state.branches.find((branch) => branch.name === localBranch)?.upstream}
            onPush={(remoteName, remoteBranch, setUpstream) =>
              void announce("Push complete", appState.pushBranch)(remoteName, localBranch, remoteBranch, setUpstream)
            }
            onCancel={appState.closePushDialog}
          />
        );
      })()}
      {active && appState.state.deleteRemoteBranchDialog !== null && (
        <DeleteRemoteBranchDialog
          remotes={appState.state.remotes}
          initialRemote={appState.state.deleteRemoteBranchDialog.remoteName}
          initialBranch={appState.state.deleteRemoteBranchDialog.branchName}
          onListRemoteBranches={appState.listRemoteBranches}
          onDelete={(remoteName, branchName) =>
            void announce("Remote branch deleted", appState.deleteRemoteBranch)(remoteName, branchName)
          }
          onCancel={appState.closeDeleteRemoteBranchDialog}
        />
      )}
```

  If the IIFE trips lint, extract a tiny `PushDialogHost` component in the same file.
- The force-push confirm text (`Overwrites {remoteName}/{currentBranchName} …`) names the branch the lease actually targets. Use `appState.state.upstream?.remoteName === remoteName ? appState.state.upstream.remoteBranch : currentBranchName`.

- [ ] **Step 4: Run tests and lint**

Run: `cd frontend && pnpm exec tsc --noEmit -p . && pnpm test -- --run && pnpm lint && pnpm build`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): wire Push to…, Delete remote branch, Fetch all and tag menu into the app and palette"
```

---

### Task 13: Tauri E2E flow: publish a branch, delete it remotely

**Files:**
- Create: `e2e/specs/refs-and-remotes.spec.ts`

- [ ] **Step 1: Write the spec**

The spec owns the remote `publish-origin`, the local branch `e2e-publish` and the bare directory. It creates them in `before()` and removes them in `after()`. It never uses Fetch all, because other specs may leave unreachable remotes in the shared repo, and it selects its remote explicitly, because the shared repo already has `origin`. `wdio.conf.ts`'s `beforeTest` reloads the app after `before()`.

```ts
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { expandSidebarSection } from "../support/sidebar";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const BARE_PATH = path.join(os.tmpdir(), "browsitory-e2e-publish-remote.git");
const REMOTE = "publish-origin";
const BRANCH = "e2e-publish";

function git(cwd: string, ...args: string[]): string {
  return execFileSync("git", args, { cwd, encoding: "utf8" }).trim();
}

function bareHas(branch: string): boolean {
  try {
    git(BARE_PATH, "rev-parse", "--verify", `refs/heads/${branch}`);
    return true;
  } catch {
    return false;
  }
}

function localHas(ref: string): boolean {
  try {
    git(E2E_REPO_PATH, "rev-parse", "--verify", ref);
    return true;
  } catch {
    return false;
  }
}

// Right-click via a synthetic DOM event (see rebase.spec.ts on why not `.click({ button: "right" })`).
async function contextMenu(element: WebdriverIO.Element) {
  await browser.execute((el) => {
    el.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }));
  }, element);
}

describe("Browsitory refs and remotes", () => {
  before(() => {
    // Order-independent: some earlier spec may or may not have committed yet.
    git(E2E_REPO_PATH, "commit", "--allow-empty", "-m", "e2e: seed for publish");
    fs.rmSync(BARE_PATH, { recursive: true, force: true });
    execFileSync("git", ["init", "--bare", BARE_PATH], { stdio: "inherit" });
    try {
      git(E2E_REPO_PATH, "remote", "remove", REMOTE);
    } catch {
      // not there from an aborted earlier run
    }
    git(E2E_REPO_PATH, "remote", "add", REMOTE, BARE_PATH);
    git(E2E_REPO_PATH, "branch", "-f", BRANCH, "HEAD");
  });

  after(() => {
    try {
      git(E2E_REPO_PATH, "remote", "remove", REMOTE); // also drops branch.<x>.remote/merge pointing at it
    } catch {
      // already gone
    }
    try {
      git(E2E_REPO_PATH, "branch", "-D", BRANCH);
    } catch {
      // already gone
    }
    fs.rmSync(BARE_PATH, { recursive: true, force: true });
  });

  it("publishes a new branch with Push to… and deletes it on the remote", async () => {
    await expandSidebarSection("Branches");

    // Local row for e2e-publish, found by its unique swatch label (remote rows say "<remote>/<name>").
    const localRow = await $(`//button[@aria-label="Show ${BRANCH} in graph"]/ancestor::li[1]`);
    await localRow.waitForExist({ timeout: 10000 });
    await contextMenu(localRow);
    await (await $("button=Push to…")).click();

    const dialog = await $(`dialog[aria-label="Push ${BRANCH}"]`);
    await dialog.waitForExist({ timeout: 10000 });
    const remoteSelect = await dialog.$("select");
    await browser.execute((el, value) => {
      const setter = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value")?.set;
      setter?.call(el, value);
      el.dispatchEvent(new Event("change", { bubbles: true }));
    }, remoteSelect, REMOTE);
    await expect(remoteSelect).toHaveValue(REMOTE);
    await expect(await dialog.$("input[type='checkbox']")).toBeSelected();
    await (await dialog.$("button=Push")).click();

    await browser.waitUntil(() => bareHas(BRANCH), {
      timeout: 15000,
      timeoutMsg: "expected the bare remote to receive the published branch",
    });
    await browser.waitUntil(() => {
      try {
        return git(E2E_REPO_PATH, "config", `branch.${BRANCH}.remote`) === REMOTE;
      } catch {
        return false;
      }
    }, { timeout: 10000, timeoutMsg: "expected Set as upstream to record the upstream" });

    // Expand the remote folder and delete the branch from its row's menu.
    await (await $(`button=${REMOTE}`)).click();
    const remoteRow = await $(`//button[@aria-label="Show ${REMOTE}/${BRANCH} in graph"]/ancestor::li[1]`);
    await remoteRow.waitForExist({ timeout: 10000 });
    await contextMenu(remoteRow);
    await (await $(`button=Delete ${REMOTE}/${BRANCH}…`)).click();

    const confirm = await $(`dialog[aria-label="Delete remote branch"]`);
    await confirm.waitForExist({ timeout: 10000 });
    await (await confirm.$("button=Delete remote branch")).click();

    await browser.waitUntil(() => !bareHas(BRANCH) && !localHas(`refs/remotes/${REMOTE}/${BRANCH}`), {
      timeout: 15000,
      timeoutMsg: "expected the remote branch and its tracking ref to be deleted",
    });
    await remoteRow.waitForExist({ reverse: true, timeout: 10000 });
  });
});
```

- [ ] **Step 2: Run the Tauri E2E**

Use the exact sequence in CLAUDE.md's "E2E: Tauri" section (frontend build with `VITE_E2E_REPO_PATH`, then `cargo build --workspace --features tauri-app/custom-protocol,tauri-app/forge-fixture-override`, then `cd e2e && pnpm install && pnpm test`), under `xvfb-run` on a headless machine. To iterate, run just this spec: `pnpm test -- --spec ./specs/refs-and-remotes.spec.ts`.
Expected: PASS. In a full run, the specs after it (`remote-management`, `remote-transfer`, …) still pass, because after `after()` the shared repo has neither `publish-origin` nor `e2e-publish`, and only gains one empty commit.

- [ ] **Step 3: Commit**

```bash
git add e2e/specs/refs-and-remotes.spec.ts
git commit -m "test(e2e): publish a branch with Push to… and delete it remotely"
```

---

### Task 14: Docs

**Files:**
- Modify: `CHANGELOG.md` (under `## [Unreleased]`, after the "Commit search" subsection), `docs/USER_GUIDE.md` ("Branches and remotes", "Tags"), `docs/ARCHITECTURE.md` (a new "## Fetch all" after "## Commit search")

- [ ] **Step 1: CHANGELOG**

```markdown
### Refs and remotes

- Tags show as outlined badges on commit graph rows (and in file history and search results).
  Right-click one to push it to a remote, check it out (detached), or delete it.
- **Push to…** on any local branch pushes it to any remote under any name, optionally setting
  that remote branch as its upstream. **Push** in the sync bar opens the same dialog when the
  current branch has no upstream yet.
- Remote branches have **Delete <remote>/<branch>…**, which deletes the branch on the remote and
  its remote-tracking branch.
- **Fetch** in the sync bar has a menu with **Fetch all remotes** and a **Prune** toggle
  (remembered per repository; until you set it, your git `fetch.prune` /
  `remote.<name>.prune` config decides). Fetch all keeps going past a remote that fails and names
  it afterwards.
- Local branches with an upstream show `↑n ↓m` in the branch tree.
- Palette: "Fetch all remotes", "Push branch to…", "Delete remote branch…".
- Fixed: a saved graph branch selection naming a deleted or renamed branch no longer breaks the
  whole refresh (#97); deleting or renaming a branch now updates the selection.
- Fixed: Push and force push of a branch whose upstream has a different name now update that
  upstream branch instead of creating a new remote branch with the local name.
```

- [ ] **Step 2: USER_GUIDE**

In "Branches and remotes":
- Add to the list of actions in the paragraph: **Push to…** for local branches, and **Delete <remote>/<branch>…** for remote branches.
- Add a paragraph on publishing. A branch without an upstream can be published with **Push to…**, or with the sync bar's **Push**, which opens the same dialog. The dialog has a remote, a remote branch name (the local name by default), and **Set as upstream**, which is on by default for branches without one.
- Add a paragraph on ahead/behind. `↑n` is commits to push and `↓m` is commits to pull, against the last fetch. Nothing is shown when the branch is in sync or has never been fetched.
- Add a paragraph on the Fetch menu (**Fetch all remotes**, **Prune**). Explain what pruning removes, and that until the toggle is set, git config decides per remote. A remote that fails during Fetch all is named in the error, and the others are still fetched.
- Deleting a remote branch cannot be undone with **Undo**, and neither can any other network operation. Local branches that tracked it keep their upstream setting.

In "Tags": tags also appear as outlined badges on commit rows. Right-click a badge to push the tag to a remote, check out its commit, or delete the local tag. The Tags panel remains the keyboard path.

- [ ] **Step 3: ARCHITECTURE**

```markdown
## Fetch all

- `git-core::remote::fetch_all` fetches every remote in `repo.remotes()` order under one
  operation id (`TransferOperation::FetchAll`), building each remote's credential provider
  through a `credentials_for` callback. A failing remote is recorded, not fatal; a cancel stops
  the loop.
- The worker turns each failure into `TransferEvent::RemoteFailed` (wire: phase `RemoteFailed`
  plus `remoteName`) before `Completed`, which fails only when no remote succeeded.
- `prune: Option<bool>`: `None` is `FetchPrune::Unspecified`, so libgit2 applies
  `remote.<name>.prune` then `fetch.prune` itself. The UI's Prune toggle is a per-repo
  `localStorage` preference; unset, every fetch sends `null`.
```

- [ ] **Step 4: Full verification and commit**

Run:
```bash
cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd frontend && pnpm lint && pnpm test -- --run && pnpm build && cd ..
cd extension && pnpm test -- --run && pnpm run compile && cd ..
```
Expected: all PASS.

```bash
git add CHANGELOG.md docs/USER_GUIDE.md docs/ARCHITECTURE.md
git commit -m "docs: refs and remotes in user guide, changelog and architecture"
```

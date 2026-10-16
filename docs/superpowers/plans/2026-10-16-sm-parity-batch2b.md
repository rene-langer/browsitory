# SM Parity Batch 2b: File History Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show one file's commit history (following renames) in place of the commit graph, with that file's diff for the selected commit, in both the Tauri app and the VSCode extension.

**Architecture:** New `git-core::history::file_history` walks from HEAD with git2, tracking the file's path per lineage (child → parent) and including commits by git's TREESAME rule. A new `diff::commit_rename_diff` shows a rename commit's real edits. Both go through `repo-service` → Tauri commands / VSCode sidecar → `RepoClient`. The frontend adds a `useFileHistory` hook, a `FileHistoryList` that replaces `CommitGraph` while history is open, a "History" button in both diff-pane file headers, a focus mode in the commit diff pane, and a palette command.

**Tech Stack:** Rust (git2, thiserror), Tauri 2, React + TypeScript, Vitest + Testing Library, WebdriverIO (Tauri E2E), @vscode/test-electron (VSCode E2E).

**Spec:** `docs/superpowers/specs/2026-10-16-sm-parity-batch2b-design.md`

## Global Constraints

- git2 only — no `git` subprocess in `git-core`/`repo-service`.
- History starts at HEAD; graph branch selection is ignored.
- Inclusion: non-merge commit shown when blob OID or mode at the tracked path differs from its parent; merge shown only when it differs from **every** parent.
- Rename detection: libgit2 `find_similar` with renames only (default 50% threshold), no copies, only when the tracked path is missing in a parent.
- Page size: 300 (same as `GRAPH_PAGE_SIZE`), "Load more" re-walks with limit + 300.
- Wire format: camelCase DTOs, `oldPath` serialized as `null` (never omitted).
- No new dependencies (license policy).
- `git-core`/`repo-service` tests use real temp-dir repos; frontend tests mock `RepoClient`, never `@tauri-apps/api` or `postMessage`.
- `HistoryError::InvalidPath` message: `Path must be relative to the repository root.`
- Empty-history copy: `No committed history for <path>.`
- Every push touching `crates/`, `frontend/src/` or `e2e/` needs a `CHANGELOG.md` update (pre-push hook) — Task 11 adds it; until then push with `SKIP_CHANGELOG_CHECK=1` only if you must push mid-plan.

## Plan-level refinements to the spec

The spec is updated in Task 11's docs commit to match these:

- Working-tree entry point is a **History button next to Blame** (hidden for untracked files), not a file-menu item: the file menu only exists for unstaged, non-conflicted rows, while history is useful for staged files too.
- File-history state lives in its own hook `frontend/src/state/useFileHistory.ts` (used by `RepoWorkspace` in `App.tsx`), not inside `useAppState` (already 21 KB and split into action hooks).
- Palette command is "File history of current file" and uses the existing `DiffCommands.currentFile` (the commit view has no single selected file).
- Opening history selects its newest entry once loaded.
- In history mode the commit diff pane shows only the history file, with a "Show all N files" button.

## Review Focus

1. Paths with glob characters or spaces (`a[1]*.txt`, `my file.txt`): history and rename diff must match the literal path only — test in Task 1 and Task 2.
2. A directory name passed as path (`src`): expect empty history, not commits touching files inside — test in Task 1.
3. Mode-only change (`chmod +x`): expect that commit to be listed — test in Task 1.
4. Switching files quickly (open history of `a`, then `b` before `a` returns): only `b`'s entries may appear — test in Task 7.
5. Committing to the file while history is open: the refresh must show the new commit at the top — test in Task 7.

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/git-core/src/graph.rs` (modify) | Extract `RefDecorations` (tips-by-OID + `GraphCommit` building) shared with history |
| `crates/git-core/src/history.rs` (create) | `FileHistoryEntry`, `HistoryError`, `file_history` |
| `crates/git-core/src/diff.rs` (modify) | `commit_rename_diff` |
| `crates/git-core/src/lib.rs` (modify) | `pub mod history;` |
| `crates/git-core/tests/history.rs` (create) | Real-repo tests for history + rename diff |
| `crates/repo-service/src/worker/{mod,status}.rs` (modify) | `GetFileHistory`, `GetCommitRenameDiff` commands + handle methods |
| `crates/tauri-app/src/commands/{mod,status}.rs`, `src/main.rs` (modify) | `FileHistoryEntryDto`, two commands, registration |
| `crates/vscode-sidecar/src/dispatch.rs`, `tests/protocol_roundtrip.rs` (modify) | Two dispatch methods + round-trip test |
| `frontend/src/ipc/{RepoClient,tauriRepoClient,vscodeRepoClient}.ts` (modify) | Types + two client methods |
| `frontend/src/state/useFileHistory.ts` (create) | History open/close/load-more/retry/refresh, stale-response guard, selection restore |
| `frontend/src/components/FileHistoryList.tsx` + `.module.css` (create) | Banner + flat commit list + Load more |
| `frontend/src/components/DiffPane.tsx` (modify) | History buttons, focus mode, rename diff |
| `frontend/src/lib/commands.ts` (modify) | Palette command |
| `frontend/src/App.tsx` (modify) | Wire hook, swap graph/list, pass DiffPane props |
| `e2e/specs/file-history.spec.ts`, `extension/e2e/src/specs/file-history.spec.ts` (create) | E2E flows |
| `docs/USER_GUIDE.md`, `CHANGELOG.md`, spec (modify) | Docs |

---

### Task 1: `git-core` file history walk

**Files:**
- Modify: `crates/git-core/src/graph.rs` (tips collection at lines 30–58, commit building at 76–107)
- Create: `crates/git-core/src/history.rs`
- Modify: `crates/git-core/src/lib.rs`
- Test: `crates/git-core/tests/history.rs`

**Interfaces:**
- Consumes: `git_core::graph::GraphCommit` (existing).
- Produces:
  - `pub(crate) struct graph::RefDecorations` with `pub(crate) fn load(repo: &Repository) -> Result<Self, git2::Error>` and `pub(crate) fn graph_commit(&self, commit: &git2::Commit) -> GraphCommit`.
  - `pub struct history::FileHistoryEntry { pub commit: GraphCommit, pub path: String, pub old_path: Option<String> }` (`Debug, Clone, PartialEq, Eq`).
  - `pub enum history::HistoryError { InvalidPath, Git(git2::Error) }`.
  - `pub fn history::file_history(repo: &Repository, path: &str, limit: usize) -> Result<Vec<FileHistoryEntry>, HistoryError>`.

- [ ] **Step 1: Write the failing tests**

Create `crates/git-core/tests/history.rs`. The helper builds commits straight from in-memory trees so renames, deletes and merges are exact (the shared `commit_all` uses `add_all`, which never removes deleted files).

```rust
mod common;

use common::init_repo;
use git2::{Oid, Repository};
use git_core::history::{file_history, FileHistoryEntry, HistoryError};

const REGULAR: u32 = 0o100644;
const EXECUTABLE: u32 = 0o100755;

/// Commits exactly `files` (path, contents, mode) as the whole tree, with `parents`, without
/// touching HEAD or the worktree.
fn commit_tree(repo: &Repository, files: &[(&str, &str, u32)], parents: &[Oid], message: &str) -> Oid {
    let mut index = git2::Index::new().unwrap();
    for (path, contents, mode) in files {
        let id = repo.blob(contents.as_bytes()).unwrap();
        index
            .add(&git2::IndexEntry {
                ctime: git2::IndexTime::new(0, 0),
                mtime: git2::IndexTime::new(0, 0),
                dev: 0,
                ino: 0,
                mode: *mode,
                uid: 0,
                gid: 0,
                file_size: contents.len() as u32,
                id,
                flags: 0,
                flags_extended: 0,
                path: path.as_bytes().to_vec(),
            })
            .unwrap();
    }
    let tree = repo.find_tree(index.write_tree_to(repo).unwrap()).unwrap();
    let sig = repo.signature().unwrap();
    let parents: Vec<git2::Commit> = parents.iter().map(|id| repo.find_commit(*id).unwrap()).collect();
    let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
    repo.commit(None, &sig, &sig, message, &tree, &parent_refs).unwrap()
}

fn set_head(repo: &Repository, tip: Oid) {
    repo.reference("refs/heads/main", tip, true, "test").unwrap();
    repo.set_head("refs/heads/main").unwrap();
}

fn summaries(entries: &[FileHistoryEntry]) -> Vec<&str> {
    entries.iter().map(|e| e.commit.summary.as_str()).collect()
}

const FIVE_LINES: &str = "l1\nl2\nl3\nl4\nl5\n";
const FIVE_LINES_EDITED: &str = "l1\nl2\nl3\nl4\nCHANGED\n";

#[test]
fn lists_only_commits_that_change_the_file_newest_first() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(&repo, &[("a.txt", "1", REGULAR), ("b.txt", "1", REGULAR)], &[], "c1");
    let c2 = commit_tree(&repo, &[("a.txt", "2", REGULAR), ("b.txt", "1", REGULAR)], &[c1], "c2");
    let c3 = commit_tree(&repo, &[("a.txt", "2", REGULAR), ("b.txt", "2", REGULAR)], &[c2], "c3");
    let c4 = commit_tree(&repo, &[("a.txt", "3", REGULAR), ("b.txt", "2", REGULAR)], &[c3], "c4");
    set_head(&repo, c4);

    let history = file_history(&repo, "a.txt", 100).unwrap();

    assert_eq!(summaries(&history), vec!["c4", "c2", "c1"]);
    assert!(history.iter().all(|e| e.path == "a.txt" && e.old_path.is_none()));
    assert_eq!(history[0].commit.branch_refs, vec!["main".to_string()]);
}

#[test]
fn follows_a_pure_rename() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(&repo, &[("old.txt", FIVE_LINES, REGULAR)], &[], "add");
    let c2 = commit_tree(&repo, &[("new.txt", FIVE_LINES, REGULAR)], &[c1], "rename");
    let c3 = commit_tree(&repo, &[("new.txt", FIVE_LINES_EDITED, REGULAR)], &[c2], "edit");
    set_head(&repo, c3);

    let history = file_history(&repo, "new.txt", 100).unwrap();

    assert_eq!(summaries(&history), vec!["edit", "rename", "add"]);
    assert_eq!(history[0].path, "new.txt");
    assert_eq!(history[0].old_path, None);
    assert_eq!(history[1].path, "new.txt");
    assert_eq!(history[1].old_path.as_deref(), Some("old.txt"));
    assert_eq!(history[2].path, "old.txt");
    assert_eq!(history[2].old_path, None);
}

#[test]
fn follows_a_rename_with_an_edit_in_the_same_commit() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(&repo, &[("old.txt", FIVE_LINES, REGULAR)], &[], "add");
    let c2 = commit_tree(&repo, &[("new.txt", FIVE_LINES_EDITED, REGULAR)], &[c1], "rename and edit");
    set_head(&repo, c2);

    let history = file_history(&repo, "new.txt", 100).unwrap();

    assert_eq!(summaries(&history), vec!["rename and edit", "add"]);
    assert_eq!(history[0].old_path.as_deref(), Some("old.txt"));
    assert_eq!(history[1].path, "old.txt");
}

#[test]
fn follows_each_lineage_with_its_own_path_across_a_merge() {
    let (_dir, repo) = init_repo();
    let base = commit_tree(&repo, &[("old.txt", FIVE_LINES, REGULAR)], &[], "base");
    let side = commit_tree(&repo, &[("new.txt", FIVE_LINES, REGULAR)], &[base], "rename on side");
    let main = commit_tree(&repo, &[("old.txt", FIVE_LINES, REGULAR), ("z.txt", "z", REGULAR)], &[base], "unrelated on main");
    let merge = commit_tree(&repo, &[("new.txt", FIVE_LINES, REGULAR), ("z.txt", "z", REGULAR)], &[main, side], "merge");
    set_head(&repo, merge);

    let history = file_history(&repo, "new.txt", 100).unwrap();

    // The merge is TREESAME to `side` for new.txt, so it is hidden; `main` is TREESAME to `base`.
    assert_eq!(summaries(&history), vec!["rename on side", "base"]);
    assert_eq!(history[0].old_path.as_deref(), Some("old.txt"));
    assert_eq!(history[1].path, "old.txt");
}

#[test]
fn hides_a_merge_that_matches_one_parent() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(&repo, &[("f.txt", "1", REGULAR)], &[], "c1");
    let side = commit_tree(&repo, &[("f.txt", "2", REGULAR)], &[c1], "side edit");
    let main = commit_tree(&repo, &[("f.txt", "1", REGULAR), ("g.txt", "1", REGULAR)], &[c1], "main other");
    let merge = commit_tree(&repo, &[("f.txt", "2", REGULAR), ("g.txt", "1", REGULAR)], &[main, side], "merge");
    set_head(&repo, merge);

    let history = file_history(&repo, "f.txt", 100).unwrap();

    assert_eq!(summaries(&history), vec!["side edit", "c1"]);
}

#[test]
fn shows_a_merge_whose_resolution_differs_from_every_parent() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(&repo, &[("f.txt", "1", REGULAR)], &[], "c1");
    let side = commit_tree(&repo, &[("f.txt", "2", REGULAR)], &[c1], "side edit");
    let main = commit_tree(&repo, &[("f.txt", "3", REGULAR)], &[c1], "main edit");
    let merge = commit_tree(&repo, &[("f.txt", "4", REGULAR)], &[main, side], "resolved merge");
    set_head(&repo, merge);

    let history = file_history(&repo, "f.txt", 100).unwrap();

    assert_eq!(history[0].commit.summary, "resolved merge");
    let mut rest: Vec<&str> = summaries(&history)[1..].to_vec();
    rest.sort();
    assert_eq!(rest, vec!["c1", "main edit", "side edit"]);
}

#[test]
fn stops_at_a_re_add_after_a_delete() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(&repo, &[("f.txt", "first life", REGULAR), ("k.txt", "k", REGULAR)], &[], "c1");
    let c2 = commit_tree(&repo, &[("k.txt", "k", REGULAR)], &[c1], "delete");
    let c3 = commit_tree(&repo, &[("f.txt", "second life\nunrelated\n", REGULAR), ("k.txt", "k", REGULAR)], &[c2], "re-add");
    set_head(&repo, c3);

    let history = file_history(&repo, "f.txt", 100).unwrap();

    assert_eq!(summaries(&history), vec!["re-add"]);
}

#[test]
fn lists_a_mode_only_change() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(&repo, &[("run.sh", "echo hi\n", REGULAR)], &[], "add");
    let c2 = commit_tree(&repo, &[("run.sh", "echo hi\n", EXECUTABLE)], &[c1], "chmod");
    set_head(&repo, c2);

    let history = file_history(&repo, "run.sh", 100).unwrap();

    assert_eq!(summaries(&history), vec!["chmod", "add"]);
}

#[test]
fn matches_glob_characters_and_spaces_literally() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(&repo, &[("a[1]*.txt", "1", REGULAR), ("a1x.txt", "1", REGULAR), ("my file.txt", "1", REGULAR)], &[], "c1");
    let c2 = commit_tree(&repo, &[("a[1]*.txt", "1", REGULAR), ("a1x.txt", "2", REGULAR), ("my file.txt", "2", REGULAR)], &[c1], "c2");
    set_head(&repo, c2);

    assert_eq!(summaries(&file_history(&repo, "a[1]*.txt", 100).unwrap()), vec!["c1"]);
    assert_eq!(summaries(&file_history(&repo, "my file.txt", 100).unwrap()), vec!["c2", "c1"]);
}

#[test]
fn a_directory_path_has_no_history() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(&repo, &[("src/a.txt", "1", REGULAR)], &[], "c1");
    let c2 = commit_tree(&repo, &[("src/a.txt", "2", REGULAR)], &[c1], "c2");
    set_head(&repo, c2);

    assert!(file_history(&repo, "src", 100).unwrap().is_empty());
}

#[test]
fn respects_the_limit() {
    let (_dir, repo) = init_repo();
    let mut tip = commit_tree(&repo, &[("f.txt", "0", REGULAR)], &[], "v0");
    for n in 1..5 {
        tip = commit_tree(&repo, &[("f.txt", &n.to_string(), REGULAR)], &[tip], &format!("v{n}"));
    }
    set_head(&repo, tip);

    assert_eq!(summaries(&file_history(&repo, "f.txt", 2).unwrap()), vec!["v4", "v3"]);
}

#[test]
fn unborn_head_and_missing_path_give_empty_history() {
    let (_dir, repo) = init_repo();
    assert!(file_history(&repo, "f.txt", 100).unwrap().is_empty());

    let c1 = commit_tree(&repo, &[("f.txt", "1", REGULAR)], &[], "c1");
    set_head(&repo, c1);
    assert!(file_history(&repo, "missing.txt", 100).unwrap().is_empty());
}

#[test]
fn rejects_paths_outside_the_repository() {
    let (_dir, repo) = init_repo();
    for bad in ["", "../x.txt", "/abs.txt", "a/../../x.txt"] {
        assert!(
            matches!(file_history(&repo, bad, 100), Err(HistoryError::InvalidPath)),
            "{bad:?} should be rejected"
        );
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --test history`
Expected: compile error `could not find history in git_core`.

- [ ] **Step 3: Extract `RefDecorations` in `graph.rs`**

Replace the two tips-by-OID loops and the per-commit `GraphCommit` construction in `graph_log` with:

```rust
/// Local/remote branch names keyed by the commit they point at — the badges on graph rows.
pub(crate) struct RefDecorations {
    local: HashMap<Oid, Vec<String>>,
    remote: HashMap<Oid, Vec<String>>,
}

impl RefDecorations {
    pub(crate) fn load(repo: &Repository) -> Result<Self, git2::Error> {
        let mut local: HashMap<Oid, Vec<String>> = HashMap::new();
        for entry in repo.branches(Some(BranchType::Local))? {
            let (branch, _) = entry?;
            let Ok(Some(name)) = branch.name() else {
                continue;
            };
            if let Some(oid) = branch.get().target() {
                local.entry(oid).or_default().push(name.to_string());
            }
        }

        let mut remote: HashMap<Oid, Vec<String>> = HashMap::new();
        for entry in repo.branches(Some(BranchType::Remote))? {
            let (branch, _) = entry?;
            // Symbolic refs like `origin/HEAD` point at another ref rather than a commit
            // directly and would otherwise duplicate whatever branch they alias — skip them so
            // they don't produce a fake "origin/HEAD" badge on every default-branch commit.
            if branch.get().kind() != Some(ReferenceType::Direct) {
                continue;
            }
            let Ok(Some(name)) = branch.name() else {
                continue;
            };
            if let Some(oid) = branch.get().target() {
                remote.entry(oid).or_default().push(name.to_string());
            }
        }
        Ok(Self { local, remote })
    }

    pub(crate) fn graph_commit(&self, commit: &git2::Commit) -> GraphCommit {
        let oid = commit.id();
        let id = oid.to_string();
        GraphCommit {
            short_id: id[..7].to_string(),
            summary: commit.summary().ok().flatten().unwrap_or_default().to_string(),
            author_name: commit.author().name().ok().unwrap_or_default().to_string(),
            author_email: commit.author().email().ok().unwrap_or_default().to_string(),
            timestamp: commit.time().seconds(),
            parent_ids: commit.parent_ids().map(|p| p.to_string()).collect(),
            branch_refs: self.local.get(&oid).cloned().unwrap_or_default(),
            remote_branch_refs: self.remote.get(&oid).cloned().unwrap_or_default(),
            id,
        }
    }
}
```

Note: `commit.summary()` / `author().name()` here return `Result<Option<&str>, _>` / `Result<&str, _>` exactly as the current code uses them — keep whatever call shape compiles in the current git2 version (copy it from the lines you are replacing).

`graph_log` becomes:

```rust
pub fn graph_log(
    repo: &Repository,
    limit: usize,
    selected_branches: Option<&[String]>,
) -> Result<Vec<GraphCommit>, GraphError> {
    let decorations = RefDecorations::load(repo)?;

    let mut revwalk = repo.revwalk()?;
    revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
    match selected_branches {
        // (keep the existing comment block here unchanged)
        None => revwalk.push_glob("refs/heads/*")?,
        Some(names) => {
            for name in names {
                revwalk.push_ref(&format!("refs/heads/{name}"))?;
            }
        }
    }

    let mut commits = Vec::new();
    for oid_result in revwalk.take(limit) {
        let commit = repo.find_commit(oid_result?)?;
        commits.push(decorations.graph_commit(&commit));
    }
    Ok(commits)
}
```

Run: `cargo test -p git-core --test graph`
Expected: PASS (pure refactor).

- [ ] **Step 4: Implement `history.rs`**

Add `pub mod history;` to `crates/git-core/src/lib.rs` (alphabetical, after `pub mod graph;`). Create `crates/git-core/src/history.rs`:

```rust
use std::collections::HashMap;
use std::path::{Component, Path};

use git2::{Delta, DiffFindOptions, ErrorCode, Oid, Repository, Sort, Tree};
use thiserror::Error;

use crate::graph::{GraphCommit, RefDecorations};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHistoryEntry {
    pub commit: GraphCommit,
    /// Path of the file in this commit.
    pub path: String,
    /// Set when this commit renamed `old_path` to `path` (relative to its first parent).
    pub old_path: Option<String>,
}

#[derive(Debug, Error)]
pub enum HistoryError {
    #[error("Path must be relative to the repository root.")]
    InvalidPath,
    #[error(transparent)]
    Git(#[from] git2::Error),
}

/// Commits reachable from HEAD that changed `path`, newest first, following renames.
///
/// The tracked path is carried from each commit to its parents (renamed where the parent has
/// the file under another name), so a rename on one branch doesn't rename the file on another.
/// A commit is listed when it differs from every parent at its path (git's TREESAME rule): a
/// merge that took one side's version unchanged is hidden, the side commit is shown instead.
pub fn file_history(
    repo: &Repository,
    path: &str,
    limit: usize,
) -> Result<Vec<FileHistoryEntry>, HistoryError> {
    validate_path(path)?;
    let head = match repo.head() {
        Ok(head) => head,
        Err(error) if matches!(error.code(), ErrorCode::UnbornBranch | ErrorCode::NotFound) => {
            return Ok(Vec::new())
        }
        Err(error) => return Err(error.into()),
    };
    let head_id = head.peel_to_commit()?.id();
    let decorations = RefDecorations::load(repo)?;

    let mut walk = repo.revwalk()?;
    // Topological: every child is visited before its parents, so a parent's tracked path is
    // final by the time the walk reaches it.
    walk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
    walk.push(head_id)?;

    let mut tracked: HashMap<Oid, String> = HashMap::from([(head_id, path.to_string())]);
    let mut entries = Vec::new();
    for oid in walk {
        if entries.len() >= limit || tracked.is_empty() {
            break;
        }
        let oid = oid?;
        let Some(current_path) = tracked.remove(&oid) else {
            continue;
        };
        let commit = repo.find_commit(oid)?;
        let tree = commit.tree()?;
        let Some(blob) = blob_at(&tree, &current_path) else {
            continue;
        };

        let mut same_as_a_parent = false;
        let mut renamed_from = None;
        for (index, parent) in commit.parents().enumerate() {
            let parent_tree = parent.tree()?;
            let parent_path = match blob_at(&parent_tree, &current_path) {
                Some(parent_blob) => {
                    same_as_a_parent |= parent_blob == blob;
                    Some(current_path.clone())
                }
                None => {
                    let source = rename_source(repo, &parent_tree, &tree, &current_path)?;
                    if index == 0 {
                        renamed_from.clone_from(&source);
                    }
                    source
                }
            };
            if let Some(parent_path) = parent_path {
                // First (nearest) child wins when two children track the same parent.
                tracked.entry(parent.id()).or_insert(parent_path);
            }
        }

        if !same_as_a_parent {
            entries.push(FileHistoryEntry {
                commit: decorations.graph_commit(&commit),
                path: current_path,
                old_path: renamed_from,
            });
        }
    }
    Ok(entries)
}

fn validate_path(path: &str) -> Result<(), HistoryError> {
    let relative = Path::new(path);
    if path.is_empty() || !relative.components().all(|c| matches!(c, Component::Normal(_))) {
        return Err(HistoryError::InvalidPath);
    }
    Ok(())
}

/// Blob id and file mode at `path`, or `None` when absent or not a file (e.g. a directory).
fn blob_at(tree: &Tree, path: &str) -> Option<(Oid, i32)> {
    let entry = tree.get_path(Path::new(path)).ok()?;
    (entry.kind() == Some(git2::ObjectType::Blob)).then(|| (entry.id(), entry.filemode()))
}

/// The path `new_path` was renamed from between `old` and `new`, if libgit2 sees a rename.
fn rename_source(
    repo: &Repository,
    old: &Tree,
    new: &Tree,
    new_path: &str,
) -> Result<Option<String>, git2::Error> {
    let mut diff = repo.diff_tree_to_tree(Some(old), Some(new), None)?;
    let mut find = DiffFindOptions::new();
    find.renames(true);
    diff.find_similar(Some(&mut find))?;
    Ok(diff
        .deltas()
        .find(|d| d.status() == Delta::Renamed && d.new_file().path() == Some(Path::new(new_path)))
        .and_then(|d| d.old_file().path().map(|p| p.to_string_lossy().into_owned())))
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p git-core --test history && cargo test -p git-core --test graph`
Expected: all PASS.

- [ ] **Step 6: Lint and commit**

```bash
cargo clippy -p git-core --all-targets -- -D warnings && cargo fmt --all
git add crates/git-core/src/graph.rs crates/git-core/src/history.rs crates/git-core/src/lib.rs crates/git-core/tests/history.rs
git commit -m "feat(git-core): file history walk following renames"
```

---

### Task 2: `git-core` rename diff

**Files:**
- Modify: `crates/git-core/src/diff.rs` (after `commit_diff`, ~line 124)
- Test: `crates/git-core/tests/history.rs` (append)

**Interfaces:**
- Consumes: `diff::DiffOptions`, `DiffHunk`, `DiffLineOrigin` (existing).
- Produces: `pub fn diff::commit_rename_diff(repo: &Repository, commit_id: &str, old_path: &str, new_path: &str, opts: &DiffOptions) -> Result<Vec<DiffHunk>, DiffError>`.

- [ ] **Step 1: Write the failing tests** (append to `crates/git-core/tests/history.rs`)

```rust
use git_core::diff::{commit_rename_diff, DiffLineOrigin, DiffOptions};

#[test]
fn rename_diff_shows_only_the_edited_lines() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(&repo, &[("old.txt", FIVE_LINES, REGULAR)], &[], "add");
    let c2 = commit_tree(&repo, &[("new.txt", FIVE_LINES_EDITED, REGULAR)], &[c1], "rename and edit");

    let hunks = commit_rename_diff(&repo, &c2.to_string(), "old.txt", "new.txt", &DiffOptions::default()).unwrap();

    let changed: Vec<(DiffLineOrigin, &str)> = hunks
        .iter()
        .flat_map(|h| h.lines.iter())
        .filter(|l| l.origin != DiffLineOrigin::Context)
        .map(|l| (l.origin, l.content.trim_end()))
        .collect();
    assert_eq!(changed, vec![(DiffLineOrigin::Remove, "l5"), (DiffLineOrigin::Add, "CHANGED")]);
}

#[test]
fn pure_rename_diff_has_no_hunks() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(&repo, &[("old.txt", FIVE_LINES, REGULAR)], &[], "add");
    let c2 = commit_tree(&repo, &[("new.txt", FIVE_LINES, REGULAR)], &[c1], "rename");

    let hunks = commit_rename_diff(&repo, &c2.to_string(), "old.txt", "new.txt", &DiffOptions::default()).unwrap();

    assert!(hunks.is_empty());
}

#[test]
fn rename_diff_matches_glob_characters_literally() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(&repo, &[("o[1]*.txt", FIVE_LINES, REGULAR), ("o1x.txt", "noise\n", REGULAR)], &[], "add");
    let c2 = commit_tree(&repo, &[("n[1]*.txt", FIVE_LINES_EDITED, REGULAR)], &[c1], "rename");

    let hunks = commit_rename_diff(&repo, &c2.to_string(), "o[1]*.txt", "n[1]*.txt", &DiffOptions::default()).unwrap();

    let all: Vec<&str> = hunks.iter().flat_map(|h| h.lines.iter()).map(|l| l.content.trim_end()).collect();
    assert!(!all.contains(&"noise"));
    assert!(all.contains(&"CHANGED"));
}
```

If `DiffLineOrigin` lacks `Debug`/`PartialEq`, it already derives `Debug, Clone, Copy, PartialEq, Eq` (see `diff.rs:12`) — no change needed.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --test history rename_diff`
Expected: compile error `cannot find function commit_rename_diff`.

- [ ] **Step 3: Implement** (in `crates/git-core/src/diff.rs`, right after `commit_diff`)

```rust
/// Diff of a file that `commit_id` renamed from `old_path` to `new_path`, against the first
/// parent, with rename detection so edits show as edits rather than a whole-file add. A pure
/// rename yields no hunks.
pub fn commit_rename_diff(
    repo: &Repository,
    commit_id: &str,
    old_path: &str,
    new_path: &str,
    opts: &DiffOptions,
) -> Result<Vec<DiffHunk>, DiffError> {
    let (old_tree, new_tree) = commit_and_parent_trees(repo, commit_id)?;
    let mut git_opts = git2::DiffOptions::new();
    // Literal paths, not globs — see `working_diff_options`.
    git_opts
        .pathspec(old_path)
        .pathspec(new_path)
        .disable_pathspec_match(true);
    apply_display_options(&mut git_opts, opts);
    let mut diff =
        repo.diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), Some(&mut git_opts))?;
    let mut find = git2::DiffFindOptions::new();
    find.renames(true);
    diff.find_similar(Some(&mut find))?;
    hunks_from_diff(&diff)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p git-core --test history`
Expected: all PASS.

- [ ] **Step 5: Lint and commit**

```bash
cargo clippy -p git-core --all-targets -- -D warnings && cargo fmt --all
git add crates/git-core/src/diff.rs crates/git-core/tests/history.rs
git commit -m "feat(git-core): rename-aware commit diff"
```

---

### Task 3: `repo-service` worker commands

**Files:**
- Modify: `crates/repo-service/src/worker/mod.rs` (enum `Command` ~line 70–105, match ~line 550–570, tests ~line 2147)
- Modify: `crates/repo-service/src/worker/status.rs` (free fns ~line 66, `WorkerHandle` methods ~line 336)

**Interfaces:**
- Consumes: `git_core::history::{file_history, FileHistoryEntry}`, `git_core::diff::commit_rename_diff`.
- Produces on `WorkerHandle`:
  - `pub fn get_file_history(&self, path: String, limit: usize) -> Result<Vec<FileHistoryEntry>, String>`
  - `pub fn get_commit_rename_diff(&self, commit_id: String, old_path: String, new_path: String, opts: git_core::diff::DiffOptions) -> Result<Vec<DiffHunk>, String>`

- [ ] **Step 1: Write the failing test** (in `mod.rs` test module, next to `get_blame_reflects_a_commit`)

```rust
    #[test]
    fn get_file_history_and_rename_diff_round_trip_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "file.txt", "l1\nl2\nl3\nl4\nl5\n");
        commit_all(&repo, "add file");
        write_file(dir.path(), "other.txt", "x\n");
        commit_all(&repo, "unrelated");
        write_file(dir.path(), "file.txt", "l1\nl2\nl3\nl4\nCHANGED\n");
        commit_all(&repo, "edit file");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        let history = handle.get_file_history("file.txt".into(), 10).unwrap();

        let summaries: Vec<&str> = history.iter().map(|e| e.commit.summary.as_str()).collect();
        assert_eq!(summaries, vec!["edit file", "add file"]);

        // No rename in this repo: diffing the same path as both sides behaves like commit_diff.
        let hunks = handle
            .get_commit_rename_diff(
                history[0].commit.id.clone(),
                "file.txt".into(),
                "file.txt".into(),
                git_core::diff::DiffOptions::default(),
            )
            .unwrap();
        assert_eq!(hunks.len(), 1);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p repo-service get_file_history_and_rename_diff`
Expected: compile error `no method named get_file_history`.

- [ ] **Step 3: Implement**

`mod.rs`, in `enum Command` after `GetBlame { .. }`:

```rust
    GetFileHistory {
        path: String,
        limit: usize,
        reply: Sender<Result<Vec<git_core::history::FileHistoryEntry>, String>>,
    },
    GetCommitRenameDiff {
        commit_id: String,
        old_path: String,
        new_path: String,
        opts: git_core::diff::DiffOptions,
        reply: Sender<Result<Vec<DiffHunk>, String>>,
    },
```

`mod.rs`, in the dispatch `match` after the `Command::GetBlame` arm:

```rust
                    Command::GetFileHistory { path, limit, reply } => {
                        status::get_file_history(&repo, path, limit, reply)
                    }
                    Command::GetCommitRenameDiff {
                        commit_id,
                        old_path,
                        new_path,
                        opts,
                        reply,
                    } => status::get_commit_rename_diff(
                        &repo, commit_id, old_path, new_path, opts, reply,
                    ),
```

`status.rs`, after the free fn `get_blame`:

```rust
pub(super) fn get_file_history(
    repo: &git2::Repository,
    path: String,
    limit: usize,
    reply: Sender<Result<Vec<git_core::history::FileHistoryEntry>, String>>,
) {
    let _ = reply.send(
        git_core::history::file_history(repo, &path, limit).map_err(|error| error.to_string()),
    );
}

pub(super) fn get_commit_rename_diff(
    repo: &git2::Repository,
    commit_id: String,
    old_path: String,
    new_path: String,
    opts: git_core::diff::DiffOptions,
    reply: Sender<Result<Vec<DiffHunk>, String>>,
) {
    let _ = reply.send(
        git_core::diff::commit_rename_diff(repo, &commit_id, &old_path, &new_path, &opts)
            .map_err(|error| error.to_string()),
    );
}
```

`status.rs`, in `impl WorkerHandle` after `get_blame`:

```rust
    pub fn get_file_history(
        &self,
        path: String,
        limit: usize,
    ) -> Result<Vec<git_core::history::FileHistoryEntry>, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetFileHistory {
                path,
                limit,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn get_commit_rename_diff(
        &self,
        commit_id: String,
        old_path: String,
        new_path: String,
        opts: git_core::diff::DiffOptions,
    ) -> Result<Vec<DiffHunk>, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetCommitRenameDiff {
                commit_id,
                old_path,
                new_path,
                opts,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p repo-service`
Expected: PASS.

- [ ] **Step 5: Lint and commit**

```bash
cargo clippy -p repo-service --all-targets -- -D warnings && cargo fmt --all
git add crates/repo-service/src/worker/mod.rs crates/repo-service/src/worker/status.rs
git commit -m "feat(repo-service): file history and rename diff worker commands"
```

---

### Task 4: Tauri commands

**Files:**
- Modify: `crates/tauri-app/src/commands/mod.rs` (DTO after `GraphCommitDto` ~line 466; `pub use status::{..}` ~line 42; tests ~line 1180)
- Modify: `crates/tauri-app/src/commands/status.rs` (after `get_blame` ~line 128)
- Modify: `crates/tauri-app/src/main.rs` (import ~line 11, `generate_handler!` ~line 65)

**Interfaces:**
- Consumes: `WorkerHandle::get_file_history`, `WorkerHandle::get_commit_rename_diff` (Task 3); `GraphCommitDto`, `DiffHunkDto`, `DiffOptionsDto` (existing).
- Produces: Tauri commands `get_file_history(repoPath, path, limit)` → `FileHistoryEntryDto[]`, `get_commit_rename_diff(repoPath, commitId, oldPath, newPath, opts)` → `DiffHunkDto[]`.

- [ ] **Step 1: Write the failing test** (in `commands/mod.rs` test module, after `graph_commit_dto_serializes_camel_case_fields`)

```rust
    #[test]
    fn file_history_entry_dto_serializes_camel_case_and_null_old_path() {
        let commit = GraphCommit {
            id: "abc123".into(),
            short_id: "abc123".into(),
            summary: "Edit".into(),
            author_name: "Rene".into(),
            author_email: "rene@example.com".into(),
            timestamp: 1_700_000_000,
            parent_ids: vec![],
            branch_refs: vec![],
            remote_branch_refs: vec![],
        };
        let plain = FileHistoryEntryDto::from(git_core::history::FileHistoryEntry {
            commit: commit.clone(),
            path: "new.txt".into(),
            old_path: None,
        });
        let renamed = FileHistoryEntryDto::from(git_core::history::FileHistoryEntry {
            commit,
            path: "new.txt".into(),
            old_path: Some("old.txt".into()),
        });

        let plain = serde_json::to_value(plain).unwrap();
        assert_eq!(plain["path"], "new.txt");
        assert_eq!(plain["oldPath"], serde_json::Value::Null);
        assert!(plain.as_object().unwrap().contains_key("oldPath"));
        assert_eq!(plain["commit"]["shortId"], "abc123");
        assert_eq!(serde_json::to_value(renamed).unwrap()["oldPath"], "old.txt");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p tauri-app file_history_entry_dto`
Expected: compile error `cannot find type FileHistoryEntryDto`.

- [ ] **Step 3: Implement**

`commands/mod.rs`, after `impl From<git_core::graph::GraphCommit> for GraphCommitDto`:

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHistoryEntryDto {
    pub commit: GraphCommitDto,
    pub path: String,
    // Serialized as `null` (not omitted) — the TS type is `string | null`.
    pub old_path: Option<String>,
}

impl From<git_core::history::FileHistoryEntry> for FileHistoryEntryDto {
    fn from(e: git_core::history::FileHistoryEntry) -> Self {
        FileHistoryEntryDto {
            commit: GraphCommitDto::from(e.commit),
            path: e.path,
            old_path: e.old_path,
        }
    }
}
```

`commands/status.rs`: add `FileHistoryEntryDto` to the `use super::{..}` list, then after `get_blame`:

```rust
#[tauri::command]
pub async fn get_file_history(
    repo_path: String,
    path: String,
    limit: usize,
    state: State<'_, AppState>,
) -> Result<Vec<FileHistoryEntryDto>, String> {
    Ok(worker_handle(&state, &repo_path)?
        .get_file_history(path, limit)?
        .into_iter()
        .map(FileHistoryEntryDto::from)
        .collect())
}

#[tauri::command]
pub async fn get_commit_rename_diff(
    repo_path: String,
    commit_id: String,
    old_path: String,
    new_path: String,
    opts: DiffOptionsDto,
    state: State<'_, AppState>,
) -> Result<Vec<DiffHunkDto>, String> {
    Ok(worker_handle(&state, &repo_path)?
        .get_commit_rename_diff(commit_id, old_path, new_path, opts.into())?
        .into_iter()
        .map(DiffHunkDto::from)
        .collect())
}
```

`commands/mod.rs` `pub use status::{..}`: add `get_commit_rename_diff, get_file_history` (keep alphabetical; `cargo fmt` reflows).

`main.rs`: add `get_commit_rename_diff, get_file_history` to the `use commands::{..}` import and to `tauri::generate_handler![..]` right after `get_commit_diff,`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p tauri-app && cargo build -p tauri-app`
Expected: PASS, builds.

- [ ] **Step 5: Lint and commit**

```bash
cargo clippy -p tauri-app --all-targets -- -D warnings && cargo fmt --all
git add crates/tauri-app/src
git commit -m "feat(tauri-app): file history and rename diff commands"
```

---

### Task 5: VSCode sidecar dispatch

**Files:**
- Modify: `crates/vscode-sidecar/src/dispatch.rs` (dispatch table ~line 69; DTOs after `GraphCommitDto` impl ~line 280; handlers after `get_commit_diff` ~line 368)
- Test: `crates/vscode-sidecar/tests/protocol_roundtrip.rs` (append)

**Interfaces:**
- Consumes: Task 3 worker methods; sidecar-local `GraphCommitDto`, `DiffHunkDto`, `DiffOptionsDto`.
- Produces: JSON-RPC methods `get_file_history` `{repoPath, path, limit}` and `get_commit_rename_diff` `{repoPath, commitId, oldPath, newPath, opts}`.

- [ ] **Step 1: Write the failing test** (append to `protocol_roundtrip.rs`)

```rust
#[test]
fn file_history_and_rename_diff_round_trip_through_the_sidecar() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let (dir, repo) = init_repo();
    write_file(dir.path(), "old.txt", "l1\nl2\nl3\nl4\nl5\n");
    commit_all(&repo, "add");
    std::fs::remove_file(dir.path().join("old.txt")).unwrap();
    write_file(dir.path(), "new.txt", "l1\nl2\nl3\nl4\nCHANGED\n");
    {
        let mut index = repo.index().unwrap();
        index.remove_path(std::path::Path::new("old.txt")).unwrap();
        index.write().unwrap();
    }
    commit_all(&repo, "rename and edit");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let history = sidecar.call(
        2,
        "get_file_history",
        serde_json::json!({"repoPath": repo_path, "path": "new.txt", "limit": 10}),
    );
    let entries = history["result"].as_array().expect("history result array");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["commit"]["summary"], "rename and edit");
    assert_eq!(entries[0]["oldPath"], "old.txt");
    assert_eq!(entries[1]["path"], "old.txt");
    assert!(entries[1]["oldPath"].is_null());

    let diff = sidecar.call(
        3,
        "get_commit_rename_diff",
        serde_json::json!({
            "repoPath": repo_path,
            "commitId": entries[0]["commit"]["id"],
            "oldPath": "old.txt",
            "newPath": "new.txt",
            "opts": {"ignoreWhitespace": false, "contextLines": 3},
        }),
    );
    let hunks = diff["result"].as_array().expect("rename diff result array");
    assert_eq!(hunks.len(), 1);
}
```

`commit_all` in this file: if it is a local helper using `add_all`, the explicit `index.remove_path` above is what records the delete. If the file has no `commit_all`, use the one the existing `working_and_commit_diff_round_trip_through_the_sidecar` test calls.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vscode-sidecar --test protocol_roundtrip file_history_and_rename_diff`
Expected: FAIL — response carries a JSON-RPC "unknown method" error, so `history["result"].as_array()` panics.

- [ ] **Step 3: Implement** (`dispatch.rs`)

Dispatch table, after `"get_commit_diff" => ..`:

```rust
        "get_file_history" => get_file_history(params, repos),
        "get_commit_rename_diff" => get_commit_rename_diff(params, repos),
```

After `get_commit_diff` fn:

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileHistoryEntryDto {
    commit: GraphCommitDto,
    path: String,
    // Serialized as `null` (not omitted) — the TS type is `string | null`.
    old_path: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetFileHistoryParams {
    repo_path: String,
    path: String,
    limit: usize,
}

fn get_file_history(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: GetFileHistoryParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let entries: Vec<FileHistoryEntryDto> = worker_handle(repos, &params.repo_path)?
        .get_file_history(params.path, params.limit)?
        .into_iter()
        .map(|e| FileHistoryEntryDto {
            commit: GraphCommitDto::from(e.commit),
            path: e.path,
            old_path: e.old_path,
        })
        .collect();
    serde_json::to_value(entries).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetCommitRenameDiffParams {
    repo_path: String,
    commit_id: String,
    old_path: String,
    new_path: String,
    opts: DiffOptionsDto,
}

fn get_commit_rename_diff(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: GetCommitRenameDiffParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let hunks: Vec<DiffHunkDto> = worker_handle(repos, &params.repo_path)?
        .get_commit_rename_diff(
            params.commit_id,
            params.old_path,
            params.new_path,
            params.opts.into(),
        )?
        .into_iter()
        .map(DiffHunkDto::from)
        .collect();
    serde_json::to_value(hunks).map_err(|error| error.to_string())
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vscode-sidecar`
Expected: PASS.

- [ ] **Step 5: Lint and commit**

```bash
cargo clippy -p vscode-sidecar --all-targets -- -D warnings && cargo fmt --all
git add crates/vscode-sidecar
git commit -m "feat(vscode-sidecar): file history and rename diff methods"
```

---

### Task 6: Frontend IPC types and clients

**Files:**
- Modify: `frontend/src/ipc/RepoClient.ts` (type after `GraphCommit` ~line 134; methods after `getCommitDiff` ~line 262)
- Modify: `frontend/src/ipc/tauriRepoClient.ts` (~line 99), `frontend/src/ipc/vscodeRepoClient.ts` (~line 193)
- Modify (mocks — every object literal typed as a full `RepoClient`): `frontend/src/App.test.tsx`, `frontend/src/state/useAppState.test.ts` (two literals, ~line 124 and ~line 1123), `frontend/src/components/RepoPicker.test.tsx`, `frontend/src/components/ConflictResolutionPane.test.tsx`, `frontend/src/components/RebasePlanner.test.tsx`, `frontend/src/components/DiffPane.test.tsx`

**Interfaces:**
- Produces:
  ```ts
  export interface FileHistoryEntry { commit: GraphCommit; path: string; oldPath: string | null; }
  getFileHistory(repoPath: string, path: string, limit: number): Promise<FileHistoryEntry[]>;
  getCommitRenameDiff(repoPath: string, commitId: string, oldPath: string, newPath: string, opts: DiffOptions): Promise<DiffHunk[]>;
  ```

- [ ] **Step 1: Add the type and interface methods** (`RepoClient.ts`)

After `interface GraphCommit { .. }`:

```ts
/** One commit in a file's history. `path` is the file's name in that commit; `oldPath` is set
 *  when the commit renamed `oldPath` to `path`. */
export interface FileHistoryEntry {
  commit: GraphCommit;
  path: string;
  oldPath: string | null;
}
```

In `interface RepoClient`, after `getCommitDiff(..)`:

```ts
  getFileHistory(repoPath: string, path: string, limit: number): Promise<FileHistoryEntry[]>;
  getCommitRenameDiff(repoPath: string, commitId: string, oldPath: string, newPath: string, opts: DiffOptions): Promise<DiffHunk[]>;
```

- [ ] **Step 2: Run the type check to see every place that must change**

Run: `cd frontend && pnpm exec tsc -b`
Expected: FAIL — "Property 'getFileHistory' is missing" in both clients and in each mock listed above.

- [ ] **Step 3: Implement clients and mocks**

`tauriRepoClient.ts`, after `getCommitDiff`:

```ts
  getFileHistory: (repoPath: string, path: string, limit: number) =>
    loggedInvoke<FileHistoryEntry[]>("get_file_history", { repoPath, path, limit }),
  getCommitRenameDiff: (repoPath: string, commitId: string, oldPath: string, newPath: string, opts: DiffOptions) =>
    loggedInvoke<DiffHunk[]>("get_commit_rename_diff", { repoPath, commitId, oldPath, newPath, opts }),
```

`vscodeRepoClient.ts`, after `getCommitDiff`:

```ts
  getFileHistory: (repoPath: string, path: string, limit: number) =>
    call<FileHistoryEntry[]>("get_file_history", { repoPath, path, limit }),
  getCommitRenameDiff: (repoPath: string, commitId: string, oldPath: string, newPath: string, opts: DiffOptions) =>
    call<DiffHunk[]>("get_commit_rename_diff", { repoPath, commitId, oldPath, newPath, opts }),
```

Add `FileHistoryEntry` to each file's `import type { .. } from "./RepoClient"`.

Mocks: in each listed literal, next to its `getCommitDiff` entry, add two lines using that file's own "not used" helper (`unused` in `App.test.tsx`, `DiffPane.test.tsx`, `ConflictResolutionPane.test.tsx`, `RebasePlanner.test.tsx`; `async () => unimplemented()` in `useAppState.test.ts` and `RepoPicker.test.tsx`):

```ts
    getFileHistory: unused,
    getCommitRenameDiff: unused,
```

(or `getFileHistory: async () => unimplemented(), getCommitRenameDiff: async () => unimplemented(),` in the two files that use `unimplemented`).

- [ ] **Step 4: Verify**

Run: `cd frontend && pnpm exec tsc -b && pnpm test -- --run && pnpm lint`
Expected: type check clean, all tests PASS, lint clean.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/ipc frontend/src/App.test.tsx frontend/src/state/useAppState.test.ts frontend/src/components/RepoPicker.test.tsx frontend/src/components/ConflictResolutionPane.test.tsx frontend/src/components/RebasePlanner.test.tsx frontend/src/components/DiffPane.test.tsx
git commit -m "feat(frontend): file history RepoClient methods"
```

---

### Task 7: `useFileHistory` hook

**Files:**
- Create: `frontend/src/state/useFileHistory.ts`
- Test: `frontend/src/state/useFileHistory.test.ts`

**Interfaces:**
- Consumes: `RepoClient.getFileHistory` (Task 6); `SelectedRow` from `./useAppState`; `describeError`, `DescribedError` from `../lib/errorMessages`.
- Produces:
  ```ts
  export const FILE_HISTORY_PAGE_SIZE = 300;
  export interface FileHistoryState { path: string; entries: FileHistoryEntry[]; limit: number; hasMore: boolean; loading: boolean; error: DescribedError | null; }
  export interface UseFileHistoryResult { history: FileHistoryState | null; open(path: string): void; close(): void; loadMore(): void; retry(): void; }
  export function useFileHistory(client: RepoClient, repoPath: string, refreshGeneration: number, selectedRow: SelectedRow, selectRow: (row: SelectedRow) => void): UseFileHistoryResult;
  ```

- [ ] **Step 1: Write the failing tests**

```ts
import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { FileHistoryEntry, RepoClient } from "../ipc/RepoClient";
import type { SelectedRow } from "./useAppState";
import { FILE_HISTORY_PAGE_SIZE, useFileHistory } from "./useFileHistory";

function entry(id: string, path = "f.txt", oldPath: string | null = null): FileHistoryEntry {
  return {
    commit: {
      id,
      shortId: id.slice(0, 7),
      summary: `commit ${id}`,
      authorName: "A",
      authorEmail: "a@example.com",
      timestamp: 1_700_000_000,
      parentIds: [],
      branchRefs: [],
      remoteBranchRefs: [],
    },
    path,
    oldPath,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

function setup(getFileHistory: RepoClient["getFileHistory"], selectedRow: SelectedRow = "uncommitted") {
  const client = { getFileHistory } as unknown as RepoClient;
  const selectRow = vi.fn();
  const hook = renderHook(
    ({ generation, row }: { generation: number; row: SelectedRow }) =>
      useFileHistory(client, "/repo", generation, row, selectRow),
    { initialProps: { generation: 0, row: selectedRow } },
  );
  return { ...hook, selectRow };
}

describe("useFileHistory", () => {
  it("opens with one page and selects the newest entry", async () => {
    const getFileHistory = vi.fn(async () => [entry("c2"), entry("c1")]);
    const { result, selectRow } = setup(getFileHistory);

    act(() => result.current.open("f.txt"));

    await waitFor(() => expect(result.current.history?.loading).toBe(false));
    expect(getFileHistory).toHaveBeenCalledWith("/repo", "f.txt", FILE_HISTORY_PAGE_SIZE);
    expect(result.current.history?.entries.map((e) => e.commit.id)).toEqual(["c2", "c1"]);
    expect(result.current.history?.hasMore).toBe(false);
    expect(selectRow).toHaveBeenCalledWith({ commitId: "c2" });
  });

  it("close clears history and restores the row selected before opening", async () => {
    const { result, selectRow } = setup(vi.fn(async () => [entry("c1")]), { commitId: "before" });

    act(() => result.current.open("f.txt"));
    await waitFor(() => expect(result.current.history?.loading).toBe(false));
    act(() => result.current.close());

    expect(result.current.history).toBeNull();
    expect(selectRow).toHaveBeenLastCalledWith({ commitId: "before" });
  });

  it("loadMore re-requests with a larger limit and reports hasMore", async () => {
    const full = Array.from({ length: FILE_HISTORY_PAGE_SIZE }, (_, i) => entry(`c${i}`));
    const getFileHistory = vi.fn(async () => full);
    const { result } = setup(getFileHistory);

    act(() => result.current.open("f.txt"));
    await waitFor(() => expect(result.current.history?.hasMore).toBe(true));
    act(() => result.current.loadMore());

    await waitFor(() =>
      expect(getFileHistory).toHaveBeenLastCalledWith("/repo", "f.txt", FILE_HISTORY_PAGE_SIZE * 2),
    );
  });

  it("drops a stale response when another file was opened meanwhile", async () => {
    const forA = deferred<FileHistoryEntry[]>();
    const forB = deferred<FileHistoryEntry[]>();
    const getFileHistory = vi.fn((_repo: string, path: string) => (path === "a.txt" ? forA.promise : forB.promise));
    const { result } = setup(getFileHistory);

    act(() => result.current.open("a.txt"));
    act(() => result.current.open("b.txt"));
    await act(async () => forB.resolve([entry("b1", "b.txt")]));
    await act(async () => forA.resolve([entry("a1", "a.txt")]));

    expect(result.current.history?.path).toBe("b.txt");
    expect(result.current.history?.entries.map((e) => e.commit.id)).toEqual(["b1"]);
  });

  it("refetches the open history when the repo refreshes", async () => {
    const getFileHistory = vi
      .fn<RepoClient["getFileHistory"]>()
      .mockResolvedValueOnce([entry("c1")])
      .mockResolvedValueOnce([entry("c2"), entry("c1")]);
    const { result, rerender } = setup(getFileHistory);

    act(() => result.current.open("f.txt"));
    await waitFor(() => expect(result.current.history?.entries).toHaveLength(1));
    rerender({ generation: 1, row: { commitId: "c1" } });

    await waitFor(() => expect(result.current.history?.entries.map((e) => e.commit.id)).toEqual(["c2", "c1"]));
  });

  it("does nothing on refresh while closed", () => {
    const getFileHistory = vi.fn(async () => []);
    const { rerender } = setup(getFileHistory);

    rerender({ generation: 1, row: "uncommitted" });

    expect(getFileHistory).not.toHaveBeenCalled();
  });

  it("keeps the error and retries with the same path and limit", async () => {
    const getFileHistory = vi
      .fn<RepoClient["getFileHistory"]>()
      .mockRejectedValueOnce(new Error("Path must be relative to the repository root."))
      .mockResolvedValueOnce([entry("c1")]);
    const { result } = setup(getFileHistory);

    act(() => result.current.open("../x"));
    await waitFor(() => expect(result.current.history?.error?.message).toContain("relative"));
    act(() => result.current.retry());

    await waitFor(() => expect(result.current.history?.error).toBeNull());
    expect(getFileHistory).toHaveBeenLastCalledWith("/repo", "../x", FILE_HISTORY_PAGE_SIZE);
  });
});
```

If `describeError` rewrites the message (check `frontend/src/lib/errorMessages.ts`), change the `toContain("relative")` assertion to what it returns for a plain `Error`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && pnpm test -- --run src/state/useFileHistory.test.ts`
Expected: FAIL — `Failed to resolve import "./useFileHistory"`.

- [ ] **Step 3: Implement** (`frontend/src/state/useFileHistory.ts`)

```ts
import { useCallback, useEffect, useRef, useState } from "react";
import type { FileHistoryEntry, RepoClient } from "../ipc/RepoClient";
import { describeError, type DescribedError } from "../lib/errorMessages";
import type { SelectedRow } from "./useAppState";

// Same page size as the commit graph.
export const FILE_HISTORY_PAGE_SIZE = 300;

export interface FileHistoryState {
  path: string;
  entries: FileHistoryEntry[];
  limit: number;
  // True when the last load filled its limit, so older changes probably exist.
  hasMore: boolean;
  loading: boolean;
  error: DescribedError | null;
}

export interface UseFileHistoryResult {
  /** `null` while the graph shows the normal history. */
  history: FileHistoryState | null;
  open(path: string): void;
  close(): void;
  loadMore(): void;
  retry(): void;
}

export function useFileHistory(
  client: RepoClient,
  repoPath: string,
  refreshGeneration: number,
  selectedRow: SelectedRow,
  selectRow: (row: SelectedRow) => void,
): UseFileHistoryResult {
  const [history, setHistory] = useState<FileHistoryState | null>(null);
  // What is open right now, readable from callbacks without re-creating them.
  const current = useRef<{ path: string; limit: number } | null>(null);
  // Bumped by every load and by close; a response from an older load is dropped.
  const generation = useRef(0);
  const previousRow = useRef<SelectedRow | null>(null);
  const selectFirst = useRef(false);
  const selectedRowRef = useRef(selectedRow);
  useEffect(() => {
    selectedRowRef.current = selectedRow;
  }, [selectedRow]);

  const load = useCallback(
    async (path: string, limit: number) => {
      const gen = ++generation.current;
      current.current = { path, limit };
      setHistory((prev) => ({
        path,
        limit,
        entries: prev?.path === path ? prev.entries : [],
        hasMore: prev?.path === path ? prev.hasMore : false,
        loading: true,
        error: null,
      }));
      try {
        const entries = await client.getFileHistory(repoPath, path, limit);
        if (gen !== generation.current) return;
        setHistory({ path, limit, entries, hasMore: entries.length >= limit, loading: false, error: null });
        if (selectFirst.current) {
          selectFirst.current = false;
          if (entries.length > 0) selectRow({ commitId: entries[0].commit.id });
        }
      } catch (err: unknown) {
        if (gen !== generation.current) return;
        selectFirst.current = false;
        setHistory((prev) => (prev === null ? null : { ...prev, loading: false, error: describeError(err) }));
      }
    },
    [client, repoPath, selectRow],
  );

  const open = useCallback(
    (path: string) => {
      if (current.current === null) previousRow.current = selectedRowRef.current;
      selectFirst.current = true;
      void load(path, FILE_HISTORY_PAGE_SIZE);
    },
    [load],
  );

  const close = useCallback(() => {
    generation.current++;
    current.current = null;
    selectFirst.current = false;
    setHistory(null);
    const prev = previousRow.current;
    previousRow.current = null;
    if (prev !== null) selectRow(prev);
  }, [selectRow]);

  const loadMore = useCallback(() => {
    const open = current.current;
    if (open !== null) void load(open.path, open.limit + FILE_HISTORY_PAGE_SIZE);
  }, [load]);

  const retry = useCallback(() => {
    const open = current.current;
    if (open !== null) void load(open.path, open.limit);
  }, [load]);

  // Any repo refresh (commit, checkout, fetch…) can change the file's history.
  useEffect(() => {
    const open = current.current;
    if (open !== null) void load(open.path, open.limit);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refreshGeneration]);

  return { history, open, close, loadMore, retry };
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd frontend && pnpm test -- --run src/state/useFileHistory.test.ts && pnpm lint`
Expected: PASS, lint clean.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/state/useFileHistory.ts frontend/src/state/useFileHistory.test.ts
git commit -m "feat(frontend): file history state hook"
```

---

### Task 8: `FileHistoryList` component

**Files:**
- Create: `frontend/src/components/FileHistoryList.tsx`, `frontend/src/components/FileHistoryList.module.css`
- Test: `frontend/src/components/FileHistoryList.test.tsx`

**Interfaces:**
- Consumes: `FileHistoryState` (Task 7), `SelectedRow`, `ListRow`, `InlineError`, `formatShortDate`, `CommitGraph.module.css` classes `list`, `branchBadge`, `commitSummary`, `author`, `date`, `loadMore`.
- Produces:
  ```ts
  export function FileHistoryList(props: {
    history: FileHistoryState;
    selectedRow: SelectedRow;
    onSelectRow: (row: SelectedRow) => void;
    onClose: () => void;
    onLoadMore: () => void;
    onRetry: () => void;
  }): JSX.Element;
  ```
  Accessible names used by later tasks/E2E: toolbar "File history", close button "Close file history", listbox "File history commits".

- [ ] **Step 1: Write the failing tests**

```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { FileHistoryEntry } from "../ipc/RepoClient";
import type { FileHistoryState } from "../state/useFileHistory";
import { FileHistoryList } from "./FileHistoryList";

function entry(id: string, path = "f.txt", oldPath: string | null = null): FileHistoryEntry {
  return {
    commit: {
      id,
      shortId: id.slice(0, 7),
      summary: `commit ${id}`,
      authorName: "A",
      authorEmail: "a@example.com",
      timestamp: 1_700_000_000,
      parentIds: [],
      branchRefs: [],
      remoteBranchRefs: [],
    },
    path,
    oldPath,
  };
}

function state(overrides: Partial<FileHistoryState> = {}): FileHistoryState {
  return { path: "f.txt", entries: [], limit: 300, hasMore: false, loading: false, error: null, ...overrides };
}

function renderList(history: FileHistoryState, overrides: Partial<React.ComponentProps<typeof FileHistoryList>> = {}) {
  const props = {
    history,
    selectedRow: "uncommitted" as const,
    onSelectRow: vi.fn(),
    onClose: vi.fn(),
    onLoadMore: vi.fn(),
    onRetry: vi.fn(),
    ...overrides,
  };
  render(<FileHistoryList {...props} />);
  return props;
}

describe("FileHistoryList", () => {
  it("shows the path in the banner and closes from the × button", () => {
    const props = renderList(state({ entries: [entry("c1")] }));

    expect(screen.getByRole("toolbar", { name: "File history" })).toHaveTextContent("History: f.txt");
    fireEvent.click(screen.getByRole("button", { name: "Close file history" }));
    expect(props.onClose).toHaveBeenCalled();
  });

  it("lists entries, marks the selected one and selects on click", () => {
    const props = renderList(state({ entries: [entry("c2"), entry("c1")] }), { selectedRow: { commitId: "c1" } });

    const options = screen.getAllByRole("option");
    expect(options).toHaveLength(2);
    expect(options[1]).toHaveAttribute("aria-selected", "true");
    fireEvent.click(screen.getByText(/commit c2/));
    expect(props.onSelectRow).toHaveBeenCalledWith({ commitId: "c2" });
  });

  it("labels rename entries with the old path", () => {
    renderList(state({ entries: [entry("c2", "new.txt", "old.txt")] }));

    expect(screen.getByText("renamed from old.txt")).toBeInTheDocument();
  });

  it("moves selection with arrow keys and closes on Escape", () => {
    const props = renderList(state({ entries: [entry("c2"), entry("c1")] }), { selectedRow: { commitId: "c2" } });
    const list = screen.getByRole("listbox", { name: "File history commits" });

    fireEvent.keyDown(list, { key: "ArrowDown" });
    expect(props.onSelectRow).toHaveBeenCalledWith({ commitId: "c1" });
    fireEvent.keyDown(list, { key: "Escape" });
    expect(props.onClose).toHaveBeenCalled();
  });

  it("offers Load more when more history exists", () => {
    const props = renderList(state({ entries: [entry("c1")], hasMore: true }));

    fireEvent.click(screen.getByRole("button", { name: "Load more" }));
    expect(props.onLoadMore).toHaveBeenCalled();
  });

  it("shows an empty state once loaded with no entries", () => {
    renderList(state());

    expect(screen.getByText(/No committed history for/)).toHaveTextContent("No committed history for f.txt.");
  });

  it("shows no empty state while loading", () => {
    renderList(state({ loading: true }));

    expect(screen.queryByText(/No committed history/)).toBeNull();
  });

  it("shows an error with Retry", () => {
    const props = renderList(state({ error: { message: "boom" } }));

    expect(screen.getByRole("alert")).toHaveTextContent("boom");
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(props.onRetry).toHaveBeenCalled();
  });
});
```

If `InlineError`'s retry button is labelled differently, match its real label (see `frontend/src/components/primitives/InlineError.tsx`).

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && pnpm test -- --run src/components/FileHistoryList.test.tsx`
Expected: FAIL — cannot resolve `./FileHistoryList`.

- [ ] **Step 3: Implement**

`FileHistoryList.module.css`:

```css
.banner {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-3);
  border-bottom: 1px solid var(--color-border);
  font-size: var(--text-sm);
}

.path {
  font-family: var(--font-mono, monospace);
  overflow-wrap: anywhere;
}

.renameChip {
  font-size: var(--text-xs);
  padding: 1px var(--space-2);
  border-radius: var(--radius-pill);
  color: var(--color-text-muted);
  border: 1px dashed var(--color-border);
  flex-shrink: 0;
}

.empty {
  padding: var(--space-3);
  color: var(--color-text-muted);
  font-size: var(--text-sm);
}
```

`FileHistoryList.tsx`:

```tsx
import type { KeyboardEvent } from "react";
import { formatShortDate } from "../lib/formatDate";
import type { SelectedRow } from "../state/useAppState";
import type { FileHistoryState } from "../state/useFileHistory";
import { InlineError } from "./primitives/InlineError";
import { ListRow } from "./primitives/ListRow";
import graphStyles from "./CommitGraph.module.css";
import styles from "./FileHistoryList.module.css";

/** The commit graph's place while a file's history is open: flat rows, no lanes (history
 *  parents are rarely adjacent rows). */
export function FileHistoryList({
  history,
  selectedRow,
  onSelectRow,
  onClose,
  onLoadMore,
  onRetry,
}: {
  history: FileHistoryState;
  selectedRow: SelectedRow;
  onSelectRow: (row: SelectedRow) => void;
  onClose: () => void;
  onLoadMore: () => void;
  onRetry: () => void;
}) {
  const selectedId = typeof selectedRow === "object" ? selectedRow.commitId : null;
  const selectedIndex = history.entries.findIndex((e) => e.commit.id === selectedId);

  const handleKeyDown = (event: KeyboardEvent<HTMLUListElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
      return;
    }
    const step =
      event.key === "ArrowDown" || event.key === "j" ? 1 : event.key === "ArrowUp" || event.key === "k" ? -1 : 0;
    if (step === 0 || history.entries.length === 0) return;
    event.preventDefault();
    const next =
      selectedIndex === -1 ? 0 : Math.min(Math.max(selectedIndex + step, 0), history.entries.length - 1);
    onSelectRow({ commitId: history.entries[next].commit.id });
  };

  return (
    <>
      <div className={styles.banner} role="toolbar" aria-label="File history">
        <span>
          History: <code className={styles.path}>{history.path}</code>
        </span>
        <button type="button" aria-label="Close file history" onClick={onClose}>
          ×
        </button>
      </div>
      {history.error !== null && (
        <InlineError message={history.error.message} hint={history.error.hint} onDismiss={onClose} onRetry={onRetry} />
      )}
      {history.error === null && !history.loading && history.entries.length === 0 && (
        <p className={styles.empty}>
          No committed history for <code className={styles.path}>{history.path}</code>.
        </p>
      )}
      <ul
        className={graphStyles.list}
        role="listbox"
        aria-label="File history commits"
        tabIndex={0}
        onKeyDown={handleKeyDown}
        aria-keyshortcuts="ArrowUp ArrowDown Escape"
      >
        {history.entries.map((entry) => (
          <ListRow
            key={entry.commit.id}
            className="commit-row"
            data-commit-id={entry.commit.id}
            selected={entry.commit.id === selectedId}
            onClick={() => onSelectRow({ commitId: entry.commit.id })}
          >
            {entry.commit.branchRefs.map((ref) => (
              <span key={ref} className={graphStyles.branchBadge}>
                {ref}
              </span>
            ))}
            <span className={graphStyles.commitSummary}>
              {entry.commit.shortId} {entry.commit.summary}
            </span>
            {entry.oldPath !== null && <span className={styles.renameChip}>renamed from {entry.oldPath}</span>}
            <span className={graphStyles.author} title={entry.commit.authorEmail}>
              {entry.commit.authorName}
            </span>
            <span className={graphStyles.date} title={new Date(entry.commit.timestamp * 1000).toLocaleString()}>
              {formatShortDate(entry.commit.timestamp)}
            </span>
          </ListRow>
        ))}
      </ul>
      {history.hasMore && (
        <div className={graphStyles.loadMore}>
          <span>Showing latest {history.entries.length} changes</span>
          <button type="button" onClick={onLoadMore}>
            Load more
          </button>
        </div>
      )}
    </>
  );
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd frontend && pnpm test -- --run src/components/FileHistoryList.test.tsx && pnpm lint`
Expected: PASS, lint clean.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/FileHistoryList.tsx frontend/src/components/FileHistoryList.module.css frontend/src/components/FileHistoryList.test.tsx
git commit -m "feat(frontend): file history list"
```

---

### Task 9: `DiffPane` — History buttons, focus mode, rename diff

**Files:**
- Modify: `frontend/src/components/DiffPane.tsx` — `UncommittedFileSection` (~line 81, header buttons ~line 271), `DiffPane` props (~line 427) and its `UncommittedDiffPane`/`CommitDiffPane` renders (~line 505–535), `UncommittedDiffPane` props (~line 544), `CommitFileSection` (~line 1002), `CommitDiffPane` (~line 1121)
- Test: `frontend/src/components/DiffPane.test.tsx`

**Interfaces:**
- Consumes: `RepoClient.getCommitRenameDiff` (Task 6), `isUntracked` from `./FileActionsMenu`.
- Produces on `DiffPane`:
  ```ts
  export type FileHistoryFocus = { path: string; oldPath: string | null };
  // new optional props
  fileHistoryFocus?: FileHistoryFocus | null;
  onOpenFileHistory?: (path: string) => void;
  ```

- [ ] **Step 1: Write the failing tests** (add a `describe("file history", ..)` block inside `describe("DiffPane", ..)`)

```tsx
  describe("file history", () => {
    it("offers History on tracked working-tree files only", () => {
      const onOpenFileHistory = vi.fn();
      renderUncommitted(
        fakeClient({}),
        [
          { path: "a.txt", staged: false, kind: "Modified" },
          { path: "new.txt", staged: false, kind: "New" },
        ],
        { onOpenFileHistory },
      );

      fireEvent.click(screen.getByRole("button", { name: "History of a.txt" }));
      expect(onOpenFileHistory).toHaveBeenCalledWith("a.txt");
      expect(screen.queryByRole("button", { name: "History of new.txt" })).toBeNull();
    });

    it("offers History on commit files", async () => {
      const onOpenFileHistory = vi.fn();
      renderCommit(
        fakeClient({ getCommitFiles: async () => ["f.txt"], getCommitMessage: async () => "" }),
        "abc123",
        { onOpenFileHistory },
      );

      fireEvent.click(await screen.findByRole("button", { name: "History of f.txt" }));
      expect(onOpenFileHistory).toHaveBeenCalledWith("f.txt");
    });

    it("shows only the history file in focus mode, with a way to show all files", async () => {
      renderCommit(
        fakeClient({ getCommitFiles: async () => ["f.txt", "g.txt", "h.txt"], getCommitMessage: async () => "" }),
        "abc123",
        { fileHistoryFocus: { path: "g.txt", oldPath: null } },
      );

      expect(await screen.findByText("g.txt")).toBeInTheDocument();
      expect(screen.queryByText("f.txt")).toBeNull();
      fireEvent.click(screen.getByRole("button", { name: "Show all 3 files" }));
      expect(await screen.findByText("f.txt")).toBeInTheDocument();
    });

    it("uses the rename diff and labels the old path for a rename entry", async () => {
      const getCommitRenameDiff = vi.fn(async () => []);
      const getCommitDiff = vi.fn(async () => []);
      renderCommit(
        fakeClient({
          getCommitFiles: async () => ["old.txt", "new.txt"],
          getCommitMessage: async () => "",
          getCommitRenameDiff,
          getCommitDiff,
        }),
        "abc123",
        { fileHistoryFocus: { path: "new.txt", oldPath: "old.txt" } },
      );

      expect(await screen.findByText("Renamed from old.txt")).toBeInTheDocument();
      await waitFor(() =>
        expect(getCommitRenameDiff).toHaveBeenCalledWith(TEST_REPO_PATH, "abc123", "old.txt", "new.txt", expect.anything()),
      );
      expect(getCommitDiff).not.toHaveBeenCalled();
    });
  });
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && pnpm test -- --run src/components/DiffPane.test.tsx -t "file history"`
Expected: FAIL — no "History of a.txt" button / unknown props.

- [ ] **Step 3: Implement**

3a. Near the `DiscardRequest` export, add:

```ts
/** The file whose history the graph is showing, at the selected commit. */
export type FileHistoryFocus = { path: string; oldPath: string | null };
```

3b. `DiffPane`: add to its destructured props and prop type

```ts
  // Set while the graph shows a file's history: the commit view shows only that file.
  fileHistoryFocus?: FileHistoryFocus | null;
  onOpenFileHistory?: (path: string) => void;
```

Pass `onOpenFileHistory={onOpenFileHistory}` to `<UncommittedDiffPane ..>`, and to `<CommitDiffPane ..>` pass both `onOpenFileHistory={onOpenFileHistory}` and `focus={fileHistoryFocus ?? null}`. Add `onOpenFileHistory?: (path: string) => void;` to `UncommittedDiffPane`'s props and pass it on to each `<UncommittedFileSection ..>`; add the same prop to `UncommittedFileSection`.

3c. `UncommittedFileSection` header, right after the Blame button:

```tsx
          {onOpenFileHistory !== undefined && !isUntracked(entry) && (
            <button
              type="button"
              aria-label={`History of ${entry.path}`}
              onClick={(event) => {
                event.stopPropagation();
                onOpenFileHistory(entry.path);
              }}
            >
              History
            </button>
          )}
```

3d. `CommitFileSection`: add props `oldPath?: string | null` and `onOpenFileHistory?: (path: string) => void`. In `loadDiff` replace the `client.getCommitDiff(..)` call with:

```ts
    const request =
      oldPath != null
        ? client.getCommitRenameDiff(repoPath, commitId, oldPath, path, diffOptions)
        : client.getCommitDiff(repoPath, commitId, path, diffOptions);
    request
      .then((next) => {
```

(keep the existing `.then`/`.catch` bodies) and add `oldPath` to that effect's dependency array. In the header, replace `<span className={styles.path}>{path}</span>` and the actions with:

```tsx
        <span className={styles.path}>{path}</span>
        {oldPath != null && <span className={styles.renamedFrom}>Renamed from {oldPath}</span>}
        <div className={styles.rowActions}>
          <button type="button" onClick={() => setMode("blame")}>
            Blame
          </button>
          {onOpenFileHistory !== undefined && (
            <button type="button" aria-label={`History of ${path}`} onClick={() => onOpenFileHistory(path)}>
              History
            </button>
          )}
        </div>
```

Add to `DiffPane.module.css`:

```css
.renamedFrom {
  font-size: var(--text-xs);
  color: var(--color-text-muted);
  white-space: nowrap;
}
```

3e. `CommitDiffPane`: add props `focus: FileHistoryFocus | null` and `onOpenFileHistory?: (path: string) => void`. Add state `const [showAll, setShowAll] = useState(false);` (the component is keyed by `commitId`, so it resets per commit). Before the `return`, compute:

```ts
  const focused = focus !== null && !showAll;
```

Replace the file-list rendering with:

```tsx
      {focused ? (
        <>
          <ul className={styles.fileList}>
            <CommitFileSection
              key={focus.path}
              repoPath={repoPath}
              client={client}
              commitId={commitId}
              path={focus.path}
              oldPath={focus.oldPath}
              collapsed={false}
              onToggleCollapse={() => setShowAll(true)}
              onSelectRow={onSelectRow}
              onOpenFileHistory={onOpenFileHistory}
            />
          </ul>
          {files.length > 1 && (
            <button type="button" onClick={() => setShowAll(true)}>
              Show all {files.length} files
            </button>
          )}
        </>
      ) : (
        <>
          {files.length > 0 && (
            <div className={styles.groupHeading}>
              <CollapseAllToggle allCollapsed={allCollapsed} onToggle={toggleCollapseAll} />
            </div>
          )}
          <ul className={styles.fileList}>
            {files.map((path) => (
              <CommitFileSection
                key={path}
                repoPath={repoPath}
                client={client}
                commitId={commitId}
                path={path}
                collapsed={collapsedPaths.has(path)}
                onToggleCollapse={() => toggleCollapse(path)}
                onSelectRow={onSelectRow}
                onOpenFileHistory={onOpenFileHistory}
              />
            ))}
          </ul>
        </>
      )}
```

Note: `files.length` counts a rename as two entries (delete + add, from `commit_files`), so "Show all N files" uses the raw count — consistent with what the full view then lists.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd frontend && pnpm test -- --run src/components/DiffPane.test.tsx && pnpm lint`
Expected: PASS (whole file), lint clean.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/DiffPane.tsx frontend/src/components/DiffPane.module.css frontend/src/components/DiffPane.test.tsx
git commit -m "feat(frontend): history buttons and file-history focus in diff pane"
```

---

### Task 10: Wire into `App` and the command palette

**Files:**
- Modify: `frontend/src/App.tsx` — `RepoWorkspace` (~line 53): hook call, `buildCommands` diff commands (~line 315), graph slot (~line 493), `DiffPane` props (~line 520)
- Modify: `frontend/src/lib/commands.ts` — `DiffCommands` (~line 124), `buildCommands` diff block (~line 416)
- Test: `frontend/src/lib/commands.test.ts`, `frontend/src/App.test.tsx`

**Interfaces:**
- Consumes: `useFileHistory` (Task 7), `FileHistoryList` (Task 8), `DiffPane` `fileHistoryFocus`/`onOpenFileHistory` + `FileHistoryFocus` (Task 9).
- Produces: `DiffCommands.openCurrentFileHistory(): void`; palette command id `file-history-current-file`.

- [ ] **Step 1: Write the failing tests**

Palette — `frontend/src/lib/commands.test.ts`, `describe("diff option and discard commands", ..)` (~line 755). First add `openCurrentFileHistory: vi.fn(),` to the `diffCommands` object inside its `build` helper (after `requestDiscardAllUnstaged: vi.fn(),`), then add:

```ts
  it("offers file history of the current file, even while an operation is running", () => {
    const state = makeAppState({ pending: true, status: [{ path: "a.txt", staged: false, kind: "Modified" }] });
    const { commands, diffCommands } = build(state, { currentFile: "a.txt" });

    const command = commands.find((c) => c.id === "file-history-current-file");
    expect(command?.label).toBe("File history of current file");
    command!.run();
    expect(diffCommands.openCurrentFileHistory).toHaveBeenCalled();
  });

  it("omits file history when there is no current file", () => {
    const { commands } = build(makeAppState());

    expect(commands.some((c) => c.id === "file-history-current-file")).toBe(false);
  });
```

App (in `App.test.tsx`):

```tsx
  it("opens a working-tree file's history in place of the graph and closes it again", async () => {
    const graphCommit = {
      id: "c2",
      shortId: "c2",
      summary: "edit a",
      authorName: "A",
      authorEmail: "a@example.com",
      timestamp: 1_700_000_000,
      parentIds: [],
      branchRefs: [],
      remoteBranchRefs: [],
    };
    const getFileHistory = vi.fn(async () => [{ commit: graphCommit, path: "a.txt", oldPath: null }]);
    const client = fakeClient({
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [{ path: "a.txt", staged: false, kind: "Modified" }],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
      getFileHistory,
      getCommitFiles: async () => ["a.txt"],
      getCommitMessage: async () => "edit a",
    });

    render(<App client={client} />);
    fireEvent.click(await screen.findByRole("button", { name: "History of a.txt" }));

    expect(await screen.findByRole("toolbar", { name: "File history" })).toHaveTextContent("History: a.txt");
    expect(screen.queryByRole("listbox", { name: "Commit history" })).toBeNull();
    expect(getFileHistory).toHaveBeenCalledWith("/repos/a", "a.txt", 300);
    expect(await screen.findByRole("option", { selected: true })).toHaveTextContent("edit a");

    fireEvent.click(screen.getByRole("button", { name: "Close file history" }));
    expect(await screen.findByRole("listbox", { name: "Commit history" })).toBeInTheDocument();
  });
```

If `App.test.tsx`'s `fakeClient` default for any method this flow calls (e.g. `getCommitDiff`, `getHeadInfo`) is `unused`, add it to the overrides with a harmless value — the failure message names the method.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && pnpm test -- --run src/lib src/App.test.tsx`
Expected: FAIL — no `file-history-current-file` command; no "History of a.txt" button in App.

- [ ] **Step 3: Implement**

`commands.ts` — `DiffCommands`:

```ts
  /** Opens the current file's history in place of the commit graph. */
  openCurrentFileHistory(): void;
```

`commands.ts` — inside `if (diffCommands !== undefined) {`, after the `commands.push(..)` of the four settings commands and **before** `if (!repositoryOperationDisabled) {` (history is read-only, so it is not gated):

```ts
    if (diffCommands.currentFile !== null) {
      commands.push({
        id: "file-history-current-file",
        label: "File history of current file",
        keywords: ["history", "log", "file", "follow", "rename"],
        run: diffCommands.openCurrentFileHistory,
      });
    }
```

`App.tsx` — imports:

```ts
import { FileHistoryList } from "./components/FileHistoryList";
import { useFileHistory } from "./state/useFileHistory";
```

`App.tsx` — in `RepoWorkspace`, after `appState` is created:

```ts
  const fileHistory = useFileHistory(
    client,
    repoPath,
    appState.state.refreshGeneration,
    appState.state.selectedRow,
    appState.selectRow,
  );
  const selectedRow = appState.state.selectedRow;
  const historyEntry =
    fileHistory.history !== null && typeof selectedRow === "object"
      ? fileHistory.history.entries.find((e) => e.commit.id === selectedRow.commitId) ?? null
      : null;
```

`App.tsx` — in the `buildCommands(..)` diff-commands object, add:

```ts
                openCurrentFileHistory: () => {
                  if (currentFile !== null) fileHistory.open(currentFile);
                },
```

`App.tsx` — in the `left` slot, put a conditional in front of the existing `<CommitGraph .. />` element and move that element, with all its current props unchanged, into the `:` branch:

```tsx
              {fileHistory.history !== null ? (
                <FileHistoryList
                  history={fileHistory.history}
                  selectedRow={appState.state.selectedRow}
                  onSelectRow={appState.selectRow}
                  onClose={fileHistory.close}
                  onLoadMore={fileHistory.loadMore}
                  onRetry={fileHistory.retry}
                />
              ) : (
                <CommitGraph hasMore={appState.state.hasMoreHistory} /* …rest of the existing props… */ />
              )}
```

The `/* …rest… */` marks where the existing props stay — do not retype them; cut the current element and paste it into the else branch. The direct `!== null` check (not `historyOpen`) lets TypeScript narrow `history`.

`App.tsx` — on `<DiffPane ..>`:

```tsx
                commits={fileHistory.history?.entries.map((e) => e.commit) ?? appState.state.commits}
                fileHistoryFocus={historyEntry === null ? null : { path: historyEntry.path, oldPath: historyEntry.oldPath }}
                onOpenFileHistory={fileHistory.open}
```

(replace the existing `commits={appState.state.commits}` line).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd frontend && pnpm exec tsc -b && pnpm test -- --run && pnpm lint`
Expected: type check clean, all tests PASS, lint clean.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/App.tsx frontend/src/App.test.tsx frontend/src/lib/commands.ts frontend/src/lib/commands.test.ts
git commit -m "feat(frontend): open file history from diff pane and palette"
```

---

### Task 11: E2E flows and docs

**Files:**
- Create: `e2e/specs/file-history.spec.ts`
- Create: `extension/e2e/src/specs/file-history.spec.ts`
- Modify: `docs/USER_GUIDE.md` (new section after "Browsing history and diffs", ~line 117), `CHANGELOG.md` (`## [Unreleased]`), `docs/superpowers/specs/2026-10-16-sm-parity-batch2b-design.md` (entry points / state / palette wording per "Plan-level refinements")

**Interfaces:**
- Consumes: accessible names from Tasks 8–9: button "History of <path>", toolbar "File history", button "Close file history", listbox "File history commits", text "Renamed from <old>".

- [ ] **Step 1: Write the Tauri E2E spec** (`e2e/specs/file-history.spec.ts`)

```ts
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const OLD_NAME = "history-old.txt";
const NEW_NAME = "history-new.txt";
// Spec-unique throwaway file; see blame-viewer.spec.ts for why each spec needs its own.
const PRIME_FILE = "history-prime.txt";
const BODY = "l1\nl2\nl3\nl4\nl5\n";

function git(...args: string[]) {
  execFileSync("git", args, { cwd: E2E_REPO_PATH, stdio: "inherit" });
}

async function clickViaScript(selector: string) {
  const el = await $(selector);
  await el.waitForExist({ timeout: 10000 });
  await browser.execute((node) => (node as HTMLElement).scrollIntoView({ block: "center" }), el);
  await browser.execute((node) => (node as HTMLElement).click(), el);
}

describe("Browsitory file history", () => {
  before(() => {
    fs.writeFileSync(path.join(E2E_REPO_PATH, OLD_NAME), BODY);
    git("add", OLD_NAME);
    git("commit", "-m", "e2e: history add");
    git("mv", OLD_NAME, NEW_NAME);
    fs.writeFileSync(path.join(E2E_REPO_PATH, NEW_NAME), "l1\nl2\nl3\nl4\nCHANGED\n");
    git("add", NEW_NAME);
    git("commit", "-m", "e2e: history rename");
    // Leave the file modified so it shows in the working tree with a History button.
    fs.writeFileSync(path.join(E2E_REPO_PATH, NEW_NAME), "l1\nl2\nl3\nl4\nCHANGED\nl6\n");
    fs.writeFileSync(path.join(E2E_REPO_PATH, PRIME_FILE), "prime\n");
  });

  it("opens a file's history, shows the rename diff, and closes back to the graph", async () => {
    await clickViaScript(`button[aria-label="History of ${NEW_NAME}"]`);

    const banner = await $(`[role="toolbar"][aria-label="File history"]`);
    await banner.waitForExist({ timeout: 10000 });
    await expect(banner).toHaveText(expect.stringContaining(NEW_NAME));

    const renameRow = await $("li*=e2e: history rename");
    await renameRow.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), renameRow);
    await (await $(`span=Renamed from ${OLD_NAME}`)).waitForExist({ timeout: 10000 });

    const addRow = await $("li*=e2e: history add");
    await addRow.waitForExist({ timeout: 10000 });

    await clickViaScript(`button[aria-label="Close file history"]`);
    await (await $(`ul[aria-label="Commit history"]`)).waitForExist({ timeout: 10000 });
  });
});
```

If the working tree list hasn't picked up the git-CLI commits when the spec starts, add the same "stage + commit `PRIME_FILE` through the UI" prelude as `blame-viewer.spec.ts` before the first click.

- [ ] **Step 2: Write the VSCode E2E smoke spec** (`extension/e2e/src/specs/file-history.spec.ts`)

```ts
import { execFileSync } from "node:child_process";
import * as vscode from "vscode";
import { connectToWebview } from "../support/webviewPage";

describe("Browsitory VSCode extension file history", () => {
  it("opens a file's history from the diff pane", async function () {
    // Same budget rationale as first-flow.spec.ts (webview iframe can take ~24s to appear).
    this.timeout(120000);

    const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
    if (!workspaceFolder) throw new Error("no workspace folder open");
    const cwd = workspaceFolder.uri.fsPath;
    const git = (...args: string[]) => execFileSync("git", args, { cwd }).toString().trim();
    const file = vscode.Uri.joinPath(workspaceFolder.uri, "vscode-history.txt");

    await vscode.workspace.fs.writeFile(file, Buffer.from("one\n"));
    git("add", "vscode-history.txt");
    git("commit", "-m", "vscode-e2e: history first");
    await vscode.workspace.fs.writeFile(file, Buffer.from("two\n"));

    await vscode.commands.executeCommand("browsitory.open");
    const cdpPort = process.env["BROWSITORY_VSCODE_E2E_CDP_PORT"];
    if (!cdpPort) throw new Error("BROWSITORY_VSCODE_E2E_CDP_PORT not set");
    const webview = await connectToWebview(`http://127.0.0.1:${cdpPort}`);

    try {
      await webview.waitForSelector(`button[aria-label="History of vscode-history.txt"]`);
      await webview.click(`button[aria-label="History of vscode-history.txt"]`);
      await webview.waitForSelector(`[role="toolbar"][aria-label="File history"]`);
      await webview.waitForText("li", "vscode-e2e: history first", 15000);
    } finally {
      await webview.close();
      // Leave no open panel behind: later specs re-run `browsitory.open`.
      await vscode.commands.executeCommand("workbench.action.closeAllEditors");
      git("checkout", "--", "vscode-history.txt");
    }
  });
});
```

- [ ] **Step 3: Run both E2E suites** (order-sensitive build steps, from repo root — see CLAUDE.md "Commands")

```bash
cd frontend && VITE_E2E_REPO_PATH=/tmp/browsitory-e2e-repo pnpm build && cd ..
cargo build --workspace --features tauri-app/custom-protocol,tauri-app/forge-fixture-override
cd e2e && pnpm test -- --spec specs/file-history.spec.ts && cd ..

cargo build --workspace
cd frontend && pnpm run generate:release-notes && VITE_E2E_REPO_PATH=/tmp/browsitory-vscode-e2e-repo pnpm exec vite build --config vite.vscode.config.ts && cd ..
cd extension && pnpm run compile && cd ..
cd extension/e2e && pnpm test && cd ../..
```

Expected: both PASS (headless: prefix the `pnpm test` commands with `xvfb-run -a`).

- [ ] **Step 4: Docs**

`docs/USER_GUIDE.md` — new section after "Browsing history and diffs":

```markdown
## File history

Click **History** in a file's header — in the working tree (any tracked file) or in a commit's
file list — or run **File history of current file** from the command palette. The commit list
is replaced by the commits that changed that file, newest first, starting from the checked-out
commit (HEAD). The graph's branch selection does not apply here.

Renames are followed: commits before a rename list the file under its old name, and the commit
that renamed it is marked "renamed from …". Its diff shows the real edits, not a whole-file add.
A merge is listed only when its version of the file differs from every parent.

Selecting a commit shows only that file's diff; **Show all N files** shows the whole commit.
Use ↑/↓ (or j/k) to move, **Load more** for older changes, and **×** or Esc to return to the
full history with your previous selection.
```

`CHANGELOG.md` — under `## [Unreleased]`, before the batch 2a heading:

```markdown
### File history

- **History** button on every tracked file (working tree and commit view) and a "File history
  of current file" palette command: the commit list shows only the commits that changed the
  file, following renames, with that file's diff for the selected commit.
- Rename commits show the edited lines instead of a whole-file add.
```

Spec (`docs/superpowers/specs/2026-10-16-sm-parity-batch2b-design.md`): replace the "Entry points" and "State (`useAppState`)" subsections with the five bullets from this plan's "Plan-level refinements to the spec" section.

- [ ] **Step 5: Full verification and commit**

```bash
cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd frontend && pnpm lint && pnpm test -- --run && cd ..
cd extension && pnpm test -- --run && cd ..
git add e2e/specs/file-history.spec.ts extension/e2e/src/specs/file-history.spec.ts docs/USER_GUIDE.md CHANGELOG.md docs/superpowers/specs/2026-10-16-sm-parity-batch2b-design.md
git commit -m "test(e2e): file history flows; docs: file history guide and changelog"
```

Expected: every command passes before the commit.

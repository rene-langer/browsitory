# Feature Parity Batch 4d (Compare) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Compare two commits, a branch against the current branch since they diverged, and a commit against the working tree, read-only in the diff pane, in both the Tauri app and the VSCode extension.

**Architecture:** `git-core::diff` gains rename-aware file lists (`ChangedFile`) and per-file hunks for a commit range and for commit → working tree, plus `branch_compare_range` (merge base with HEAD → branch tip). `repo-service` adds five read-only worker commands (`worker/compare.rs`). Tauri (`commands/compare.rs`) and the sidecar expose them, and `RepoClient` gains five methods. In the frontend, `useCompare` holds the compare target. `App` swaps `DiffPane` for a new `CompareDiffPane` while a compare is open. Both panes render files through `ReadOnlyFileSection`, which is extracted from `DiffPane`'s `CommitFileSection`. Entry points: the multi-select toolbar and menu (`CommitGraph`), the single-commit menu, and the branch menus (`BranchTree`).

**Tech Stack:** Rust (`git2` 0.21, `thiserror`, `serde` in tauri-app/sidecar), React + TypeScript, Vitest + Testing Library, WebdriverIO (tauri-driver).

**Spec:** `docs/superpowers/specs/2026-10-27-parity-batch4d-design.md`

## Global Constraints

- No new Rust or npm dependency. `docs/LICENSE_COMPLIANCE.md` stays as it is.
- Per-file diffs return `DiffHunk[]` (there is no `FileDiff` type). File lists return `ChangedFile { path, old_path }` (`{ path, oldPath }` on the wire, with `oldPath` serialized as `null`, not omitted).
- Rename detection: `git2::DiffFindOptions::new().renames(true)` (default similarity), on both file lists, and on a per-file diff only when `old_path` is given.
- Range ends are 40-hex commit ids parsed with `Oid::from_str`. Branch compares take a full ref name (`refs/heads/…`, `refs/remotes/…`).
- Working-tree compare: `diff_tree_to_workdir_with_index` with `include_untracked(true)`, `recurse_untracked_dirs(true)`, `show_untracked_content(true)`.
- Error copy, exactly: `There is no commit checked out to compare with.` / `<short ref> and HEAD have no common ancestor.` / `No branch named "<short ref>".` (`<short ref>` = the ref name without `refs/heads/` or `refs/remotes/`).
- UI copy, exactly: `Compare these commits`, `Diff against working tree`, `Compare with current branch`, `Compare with HEAD`; section aria-label `Comparison`; headings `Comparing <from>..<to>`, `Changes on <label> since it diverged`, `Comparing <from> with the working tree`; `From`, `To`, `(merge base)`, `Working tree`; buttons `Swap` (aria-label `Swap sides`) and `×` (aria-label `Close compare`); status `1 file changed`, `N files changed`, `No differences.`, `Finding where <label> diverged…`.
- `COMPARE_EXPAND_LIMIT = 50`: above this many files, every file section starts collapsed.
- Wire methods: `get_range_files {repoPath, fromId, toId}` → `ChangedFile[]`; `get_range_diff {repoPath, fromId, toId, path, oldPath, opts}` → `DiffHunk[]`; `get_working_tree_compare_files {repoPath, commitId}` → `ChangedFile[]`; `get_working_tree_compare_diff {repoPath, commitId, path, oldPath, opts}` → `DiffHunk[]`; `get_branch_compare_range {repoPath, refName}` → `{fromId, toId}`.
- Compare records no undo entry. `undo::plan_for` already returns `None` for unknown commands (`_ => return None`), so do not add an arm.
- Tests: `git-core`/`repo-service` use real temp-dir repos. Frontend tests mock `RepoClient` only.
- Every push touching `crates/`, `frontend/src/` or `e2e/` needs a `CHANGELOG.md` update (pre-push hook). The changelog lands in Task 11, so push only after Task 11.

## Review Focus

1. **Rename across a range** is one entry whose diff shows only the edit, not a delete + add. Pinned in Task 1 (`a_rename_across_the_range_is_one_entry_with_only_the_edit`).
2. **Older..newer and Swap.** The commit further down the graph is `from`, whatever the click order. Swap flips it. Pinned in Task 9 (`orders the two commits older..newer whatever the click order`) and Task 7 (`swap flips from and to`).
3. **Branch compare ignores HEAD-only commits** (merge base, not tip-to-tip). Pinned in Task 2 (`branch_compare_uses_the_merge_base_and_ignores_head_only_commits`).
4. **A compare must not resurface** after the user clicked elsewhere and then back. Pinned in Task 7 (`closes when the selected row changes and does not come back`).
5. **The extraction keeps the commit view unchanged.** `DiffPane.test.tsx` passes untouched after Task 6.

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/git-core/src/diff.rs` | `DiffError` variants, `ChangedFile`, `CompareRange`, `range_diff_files`, `range_diff`, `compare_working_tree_files`, `compare_working_tree`, `branch_compare_range`, private helpers |
| `crates/git-core/tests/common/mod.rs` | `commit_tree` helper (moved from `tests/history.rs`) |
| `crates/git-core/tests/history.rs` | uses `common::commit_tree` |
| `crates/git-core/tests/compare.rs` (new) | real-repo compare tests |
| `crates/repo-service/src/worker/compare.rs` (new) | handlers + `WorkerHandle` methods |
| `crates/repo-service/src/worker/mod.rs` | `mod compare;`, five `Command` variants, dispatch arms, worker tests |
| `crates/tauri-app/src/commands/compare.rs` (new), `commands/mod.rs`, `main.rs` | commands, `ChangedFileDto`, `CompareRangeDto` + test |
| `crates/vscode-sidecar/src/dispatch.rs`, `tests/protocol_roundtrip.rs` | methods + DTOs + round-trip |
| `frontend/src/ipc/RepoClient.ts`, `tauriRepoClient.ts`, `vscodeRepoClient.ts` (+ tests, fixtures) | `ChangedFile`, `CompareRange`, five methods |
| `frontend/src/components/CollapseToggle.tsx` (new) | `CollapseToggle`, `CollapseAllToggle` (moved from `DiffPane.tsx`) |
| `frontend/src/components/ReadOnlyFileSection.tsx` (new) + test | per-file diff/blame/history section (was `CommitFileSection`) |
| `frontend/src/components/DiffPane.tsx` | uses the two moved pieces |
| `frontend/src/state/useAppState.ts` | exports `rowsEqual` (moved from `CommitGraph.tsx`) |
| `frontend/src/state/useCompare.ts` (new) + test | compare target, branch resolution, close-on-selection-change |
| `frontend/src/components/CompareDiffPane.tsx`, `.module.css`, `.test.tsx` (new) | compare header + file sections |
| `frontend/src/components/CommitGraph.tsx` (+ test) | `Compare these commits`, `Diff against working tree` |
| `frontend/src/components/BranchTree.tsx` (+ test) | `Compare with current branch` / `Compare with HEAD` |
| `frontend/src/App.tsx`, `App.test.tsx` | wiring |
| `e2e/specs/compare.spec.ts` (new) | E2E |
| `CHANGELOG.md`, `docs/USER_GUIDE.md` | docs |

---

### Task 0: Pre-flight (4b and 4c have merged)

Batches 4b (refs/remotes) and 4c (merge/stash options) land on `main` before this plan runs. This plan was written against `e1e2c8a`.

- [ ] **Step 1: Rebase**

```bash
git fetch origin
git rebase origin/main
```

- [ ] **Step 2: Re-check the names this plan relies on**

Run each and compare with the plan. Where something moved or was renamed, use the new name in every later task and note the change in the task's commit message:

```bash
graft skeleton crates/git-core/src/diff.rs             # commit_diff, commit_rename_diff, commit_files, commit_and_parent_trees, hunks_from_diff, apply_display_options, DiffError (one variant?)
graft skeleton frontend/src/components/DiffPane.tsx    # CommitFileSection, CommitDiffPane, CollapseToggle, CollapseAllToggle
graft skeleton frontend/src/components/CommitGraph.tsx # rowsEqual, singleCommitItems, multiSelectItems
graft skeleton frontend/src/components/BranchTree.tsx  # branchContextItems, remoteBranchItems
graft ask "where does the stash diff view render in the diff pane" --source
grep -n "FileDiff\|ChangedFile" crates/git-core/src/*.rs frontend/src/ipc/RepoClient.ts
grep -n "tags" frontend/src/ipc/RepoClient.ts crates/git-core/src/graph.rs
```

Things to check:
- **4c read-only section.** If 4c already extracted `CommitFileSection` into a shared read-only component (for the stash diff), reuse it in Task 6 instead of creating `ReadOnlyFileSection`. Give it the `fetchDiff`/`diffKey`/`blameAt` props if it lacks them.
- **4c stash view state.** If 4c added a stash-view state that replaces the diff pane, Task 10 must make it and the compare mutually exclusive (opening one closes the other).
- **4c `FileDiff`.** If 4c added a type with `path`/`old_path`, keep `ChangedFile` separate (4c's type carries hunks), but match its field names.
- **4b `GraphCommit.tags`.** Every `GraphCommit` literal in this plan's frontend tests needs `tags: []`, and Rust `GraphCommit` literals need `tags: vec![]`.
- **4b branch menus.** Note where 4b added "Push to…" and "Delete <remote>/<branch>…". Task 9 inserts after "Merge into current branch" (local) and after "Set as upstream for current branch" (remote).

- [ ] **Step 3: Baseline is green**

Run: `cargo test --workspace && (cd frontend && pnpm test -- --run)`
Expected: PASS. Fix nothing here. If `main` is red, stop and report.

---

### Task 1: `git-core` range compare (`range_diff_files`, `range_diff`)

**Files:**
- Modify: `crates/git-core/src/diff.rs`
- Modify: `crates/git-core/tests/common/mod.rs` (add `commit_tree`), `crates/git-core/tests/history.rs` (delete its local `commit_tree`, import it from `common`)
- Create: `crates/git-core/tests/compare.rs`

**Interfaces:**
- Produces (used by Tasks 2–4):

```rust
pub struct ChangedFile { pub path: String, pub old_path: Option<String> }   // Debug, Clone, PartialEq, Eq
pub fn range_diff_files(repo: &Repository, from_id: &str, to_id: &str) -> Result<Vec<ChangedFile>, DiffError>;
pub fn range_diff(repo: &Repository, from_id: &str, to_id: &str, path: &str, old_path: Option<&str>, opts: &DiffOptions) -> Result<Vec<DiffHunk>, DiffError>;
// private, reused by Task 2:
fn commit_tree<'repo>(repo: &'repo Repository, commit_id: &str) -> Result<git2::Tree<'repo>, DiffError>;
fn literal_path_options(path: &str, old_path: Option<&str>, opts: &DiffOptions) -> git2::DiffOptions;
fn detect_renames(diff: &mut git2::Diff) -> Result<(), DiffError>;
fn changed_files(diff: &git2::Diff) -> Vec<ChangedFile>;
```

- [ ] **Step 1: Move the `commit_tree` test helper into `common`**

Cut `fn commit_tree(...)` (the whole function, with its doc comment) from `crates/git-core/tests/history.rs`. Paste it into `crates/git-core/tests/common/mod.rs` as `pub fn commit_tree`, with `#[allow(dead_code)]` like the other helpers, and add `use git2::Oid;` at the top of `common/mod.rs`. In `history.rs`, change `use common::init_repo;` to `use common::{commit_tree, init_repo};`. Keep `REGULAR`/`EXECUTABLE` in `history.rs`.

Run: `cargo test -p git-core --test history`
Expected: PASS (pure move).

- [ ] **Step 2: Write the failing range tests**

Create `crates/git-core/tests/compare.rs`:

```rust
mod common;

use common::{commit_tree, init_repo};
use git2::Oid;
use git_core::diff::{range_diff, range_diff_files, ChangedFile, DiffHunk, DiffLineOrigin, DiffOptions};

const REGULAR: u32 = 0o100644;
const FIVE_LINES: &str = "l1\nl2\nl3\nl4\nl5\n";

fn changed(path: &str) -> ChangedFile {
    ChangedFile { path: path.into(), old_path: None }
}

/// (origin, content without the line ending) of every added or removed line.
fn edits(hunks: &[DiffHunk]) -> Vec<(DiffLineOrigin, String)> {
    hunks
        .iter()
        .flat_map(|h| h.lines.iter())
        .filter(|l| l.origin != DiffLineOrigin::Context)
        .map(|l| (l.origin, l.content.trim_end().to_string()))
        .collect()
}

fn sorted(mut files: Vec<ChangedFile>) -> Vec<ChangedFile> {
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files
}

/// c1: a=one, b ; c2: a=two, b, c ; c3: a=three, c, d (b deleted).
fn three_commits(repo: &git2::Repository) -> (Oid, Oid, Oid) {
    let c1 = commit_tree(repo, &[("a.txt", "one\n", REGULAR), ("b.txt", "b\n", REGULAR)], &[], "c1");
    let c2 = commit_tree(
        repo,
        &[("a.txt", "two\n", REGULAR), ("b.txt", "b\n", REGULAR), ("c.txt", "c\n", REGULAR)],
        &[c1],
        "c2",
    );
    let c3 = commit_tree(
        repo,
        &[("a.txt", "three\n", REGULAR), ("c.txt", "c\n", REGULAR), ("d.txt", "d\n", REGULAR)],
        &[c2],
        "c3",
    );
    (c1, c2, c3)
}

#[test]
fn range_files_of_non_adjacent_commits_list_every_net_change() {
    let (_dir, repo) = init_repo();
    let (c1, _c2, c3) = three_commits(&repo);

    let files = range_diff_files(&repo, &c1.to_string(), &c3.to_string()).unwrap();

    assert_eq!(
        sorted(files),
        vec![changed("a.txt"), changed("b.txt"), changed("c.txt"), changed("d.txt")]
    );
}

#[test]
fn range_diff_shows_the_net_change_not_each_step() {
    let (_dir, repo) = init_repo();
    let (c1, _c2, c3) = three_commits(&repo);

    let hunks = range_diff(&repo, &c1.to_string(), &c3.to_string(), "a.txt", None, &DiffOptions::default()).unwrap();

    assert_eq!(
        edits(&hunks),
        vec![(DiffLineOrigin::Remove, "one".into()), (DiffLineOrigin::Add, "three".into())]
    );
}

#[test]
fn reversed_range_swaps_adds_and_removes() {
    let (_dir, repo) = init_repo();
    let (c1, _c2, c3) = three_commits(&repo);
    let opts = DiffOptions::default();

    let a = range_diff(&repo, &c3.to_string(), &c1.to_string(), "a.txt", None, &opts).unwrap();
    assert_eq!(
        edits(&a),
        vec![(DiffLineOrigin::Remove, "three".into()), (DiffLineOrigin::Add, "one".into())]
    );
    let d = range_diff(&repo, &c3.to_string(), &c1.to_string(), "d.txt", None, &opts).unwrap();
    assert_eq!(edits(&d), vec![(DiffLineOrigin::Remove, "d".into())]);
}

#[test]
fn a_range_from_a_commit_to_itself_is_empty() {
    let (_dir, repo) = init_repo();
    let (_c1, c2, _c3) = three_commits(&repo);

    assert!(range_diff_files(&repo, &c2.to_string(), &c2.to_string()).unwrap().is_empty());
}

#[test]
fn a_file_deleted_in_the_range_diffs_as_all_removed() {
    let (_dir, repo) = init_repo();
    let (c1, _c2, c3) = three_commits(&repo);

    let hunks = range_diff(&repo, &c1.to_string(), &c3.to_string(), "b.txt", None, &DiffOptions::default()).unwrap();

    assert_eq!(edits(&hunks), vec![(DiffLineOrigin::Remove, "b".into())]);
}

#[test]
fn a_rename_across_the_range_is_one_entry_with_only_the_edit() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(&repo, &[("old.txt", FIVE_LINES, REGULAR)], &[], "add");
    let c2 = commit_tree(&repo, &[("new.txt", FIVE_LINES, REGULAR)], &[c1], "rename");
    let c3 = commit_tree(
        &repo,
        &[("new.txt", "l1\nl2\nl3\nl4\nCHANGED\n", REGULAR), ("other.txt", "x\n", REGULAR)],
        &[c2],
        "edit",
    );

    let files = range_diff_files(&repo, &c1.to_string(), &c3.to_string()).unwrap();
    assert_eq!(
        sorted(files),
        vec![
            ChangedFile { path: "new.txt".into(), old_path: Some("old.txt".into()) },
            changed("other.txt"),
        ]
    );

    let hunks = range_diff(
        &repo,
        &c1.to_string(),
        &c3.to_string(),
        "new.txt",
        Some("old.txt"),
        &DiffOptions::default(),
    )
    .unwrap();
    assert_eq!(
        edits(&hunks),
        vec![(DiffLineOrigin::Remove, "l5".into()), (DiffLineOrigin::Add, "CHANGED".into())]
    );
}

#[test]
fn an_unknown_commit_is_an_error() {
    let (_dir, repo) = init_repo();
    let (c1, _c2, _c3) = three_commits(&repo);

    assert!(range_diff_files(&repo, &"0".repeat(40), &c1.to_string()).is_err());
}
```

- [ ] **Step 3: Run to verify they fail**

Run: `cargo test -p git-core --test compare`
Expected: FAIL to compile (`range_diff_files`, `range_diff`, `ChangedFile` not found).

- [ ] **Step 4: Implement**

In `crates/git-core/src/diff.rs`, after `DiffOptions`'s `impl Default`:

```rust
/// One changed path in a compare. `old_path` is set when rename detection paired a delete and
/// an add; `path` is then the new name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedFile {
    pub path: String,
    pub old_path: Option<String>,
}
```

Add the private helpers after `apply_display_options`:

```rust
/// Literal pathspec for `path` (and `old_path`, for a rename) plus the display options.
fn literal_path_options(path: &str, old_path: Option<&str>, opts: &DiffOptions) -> git2::DiffOptions {
    let mut git_opts = git2::DiffOptions::new();
    // Literal paths, not globs — see `working_diff_options`.
    if let Some(old_path) = old_path {
        git_opts.pathspec(old_path);
    }
    git_opts.pathspec(path).disable_pathspec_match(true);
    apply_display_options(&mut git_opts, opts);
    git_opts
}

/// Pairs deletes and adds into renames (git's default 50% similarity, no copies).
fn detect_renames(diff: &mut git2::Diff) -> Result<(), DiffError> {
    let mut find = git2::DiffFindOptions::new();
    find.renames(true);
    diff.find_similar(Some(&mut find))?;
    Ok(())
}

fn changed_files(diff: &git2::Diff) -> Vec<ChangedFile> {
    let lossy = |p: &std::path::Path| p.to_string_lossy().into_owned();
    diff.deltas()
        .filter_map(|delta| {
            let path = delta.new_file().path().or_else(|| delta.old_file().path()).map(lossy)?;
            let old_path = if delta.status() == git2::Delta::Renamed {
                delta.old_file().path().map(lossy)
            } else {
                None
            };
            Some(ChangedFile { path, old_path })
        })
        .collect()
}

fn commit_tree<'repo>(repo: &'repo Repository, commit_id: &str) -> Result<git2::Tree<'repo>, DiffError> {
    let oid = git2::Oid::from_str(commit_id)?;
    Ok(repo.find_commit(oid)?.tree()?)
}
```

Switch `commit_diff` to `let mut git_opts = literal_path_options(path, None, opts);` and `commit_rename_diff` to `let mut git_opts = literal_path_options(new_path, Some(old_path), opts);` followed by `detect_renames(&mut diff)?;` in place of its inline `DiffFindOptions` block. Behavior is unchanged: the same pathspecs and the same find options.

Add the public functions after `commit_files`:

```rust
/// Paths that differ between the trees of `from_id` and `to_id` (`git diff from to`), with
/// renames detected. The commits need not be related.
pub fn range_diff_files(repo: &Repository, from_id: &str, to_id: &str) -> Result<Vec<ChangedFile>, DiffError> {
    let old_tree = commit_tree(repo, from_id)?;
    let new_tree = commit_tree(repo, to_id)?;
    let mut diff = repo.diff_tree_to_tree(Some(&old_tree), Some(&new_tree), None)?;
    detect_renames(&mut diff)?;
    Ok(changed_files(&diff))
}

/// One file's diff between `from_id` and `to_id`. With `old_path` (a rename reported by
/// `range_diff_files`), both paths are diffed with rename detection, like `commit_rename_diff`.
pub fn range_diff(
    repo: &Repository,
    from_id: &str,
    to_id: &str,
    path: &str,
    old_path: Option<&str>,
    opts: &DiffOptions,
) -> Result<Vec<DiffHunk>, DiffError> {
    let old_tree = commit_tree(repo, from_id)?;
    let new_tree = commit_tree(repo, to_id)?;
    let mut git_opts = literal_path_options(path, old_path, opts);
    let mut diff = repo.diff_tree_to_tree(Some(&old_tree), Some(&new_tree), Some(&mut git_opts))?;
    if old_path.is_some() {
        detect_renames(&mut diff)?;
    }
    hunks_from_diff(&diff)
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p git-core`
Expected: PASS (including `tests/diff.rs` and `tests/history.rs`, which cover the refactored `commit_diff`/`commit_rename_diff`).

- [ ] **Step 6: Lint and commit**

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`

```bash
git add crates/git-core
git commit -m "feat(git-core): rename-aware diff between two commits"
```

---

### Task 2: `git-core` working-tree compare and branch range

**Files:**
- Modify: `crates/git-core/src/diff.rs`
- Test: `crates/git-core/tests/compare.rs`

**Interfaces:**
- Consumes: `commit_tree`, `literal_path_options`, `detect_renames`, `changed_files` (Task 1).
- Produces (used by Tasks 3–4):

```rust
pub enum DiffError { Diff(git2::Error), UnbornHead, NoMergeBase(String), UnknownRef(String) }
pub struct CompareRange { pub from_id: String, pub to_id: String }   // Debug, Clone, PartialEq, Eq
pub fn compare_working_tree_files(repo: &Repository, commit_id: &str) -> Result<Vec<ChangedFile>, DiffError>;
pub fn compare_working_tree(repo: &Repository, commit_id: &str, path: &str, old_path: Option<&str>, opts: &DiffOptions) -> Result<Vec<DiffHunk>, DiffError>;
pub fn branch_compare_range(repo: &Repository, ref_name: &str) -> Result<CompareRange, DiffError>;
```

- [ ] **Step 1: Write the failing tests**

Append to `crates/git-core/tests/compare.rs`. Extend its `use` lines:

```rust
use common::{commit_all, commit_tree, init_repo, write_file};
use git_core::diff::{
    branch_compare_range, compare_working_tree, compare_working_tree_files, range_diff, range_diff_files,
    ChangedFile, CompareRange, DiffError, DiffHunk, DiffLineOrigin, DiffOptions,
};
```

```rust
fn head_id(repo: &git2::Repository) -> String {
    repo.head().unwrap().peel_to_commit().unwrap().id().to_string()
}

#[test]
fn working_tree_lists_staged_unstaged_untracked_and_deleted_files() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    write_file(dir.path(), "gone.txt", "gone\n");
    commit_all(&repo, "base");
    let base = head_id(&repo);
    write_file(dir.path(), "a.txt", "a changed\n");
    write_file(dir.path(), "s.txt", "staged\n");
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("s.txt")).unwrap();
    index.write().unwrap();
    write_file(dir.path(), "u.txt", "untracked\n");
    std::fs::remove_file(dir.path().join("gone.txt")).unwrap();

    let files = compare_working_tree_files(&repo, &base).unwrap();
    assert_eq!(
        sorted(files),
        vec![changed("a.txt"), changed("gone.txt"), changed("s.txt"), changed("u.txt")]
    );

    let untracked = compare_working_tree(&repo, &base, "u.txt", None, &DiffOptions::default()).unwrap();
    assert_eq!(edits(&untracked), vec![(DiffLineOrigin::Add, "untracked".into())]);
}

#[test]
fn working_tree_against_an_older_commit_includes_later_commits() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "one\n");
    commit_all(&repo, "first");
    let first = head_id(&repo);
    write_file(dir.path(), "a.txt", "two\n");
    commit_all(&repo, "second");
    write_file(dir.path(), "a.txt", "three\n");

    let hunks = compare_working_tree(&repo, &first, "a.txt", None, &DiffOptions::default()).unwrap();

    assert_eq!(
        edits(&hunks),
        vec![(DiffLineOrigin::Remove, "one".into()), (DiffLineOrigin::Add, "three".into())]
    );
}

/// base ← main (adds main.txt), base ← feature (adds f.txt); HEAD on main.
fn diverged(repo: &git2::Repository) -> (Oid, Oid, Oid) {
    let base = commit_tree(repo, &[("shared.txt", "s\n", REGULAR)], &[], "base");
    let main = commit_tree(repo, &[("shared.txt", "s\n", REGULAR), ("main.txt", "m\n", REGULAR)], &[base], "main");
    let feature = commit_tree(repo, &[("shared.txt", "s\n", REGULAR), ("f.txt", "f\n", REGULAR)], &[base], "feature");
    repo.reference("refs/heads/main", main, true, "test").unwrap();
    repo.reference("refs/heads/feature", feature, true, "test").unwrap();
    repo.set_head("refs/heads/main").unwrap();
    (base, main, feature)
}

#[test]
fn branch_compare_uses_the_merge_base_and_ignores_head_only_commits() {
    let (_dir, repo) = init_repo();
    let (base, _main, feature) = diverged(&repo);

    let range = branch_compare_range(&repo, "refs/heads/feature").unwrap();
    assert_eq!(range, CompareRange { from_id: base.to_string(), to_id: feature.to_string() });

    let files = range_diff_files(&repo, &range.from_id, &range.to_id).unwrap();
    assert_eq!(files, vec![changed("f.txt")]);
}

#[test]
fn a_remote_tracking_branch_compares_the_same_way() {
    let (_dir, repo) = init_repo();
    let (base, _main, feature) = diverged(&repo);
    repo.reference("refs/remotes/origin/feature", feature, true, "test").unwrap();

    let range = branch_compare_range(&repo, "refs/remotes/origin/feature").unwrap();

    assert_eq!(range, CompareRange { from_id: base.to_string(), to_id: feature.to_string() });
}

#[test]
fn a_detached_head_compares_with_head() {
    let (_dir, repo) = init_repo();
    let (base, main, feature) = diverged(&repo);
    repo.set_head_detached(main).unwrap();

    let range = branch_compare_range(&repo, "refs/heads/feature").unwrap();

    assert_eq!(range, CompareRange { from_id: base.to_string(), to_id: feature.to_string() });
}

#[test]
fn unrelated_history_has_no_merge_base() {
    let (_dir, repo) = init_repo();
    diverged(&repo);
    let orphan = commit_tree(&repo, &[("x.txt", "x\n", REGULAR)], &[], "orphan");
    repo.reference("refs/heads/orphan", orphan, true, "test").unwrap();

    let error = branch_compare_range(&repo, "refs/heads/orphan").unwrap_err();

    assert!(matches!(error, DiffError::NoMergeBase(_)));
    assert_eq!(error.to_string(), "orphan and HEAD have no common ancestor.");
}

#[test]
fn an_unknown_branch_is_reported_by_its_short_name() {
    let (_dir, repo) = init_repo();
    diverged(&repo);

    let error = branch_compare_range(&repo, "refs/heads/nope").unwrap_err();

    assert_eq!(error.to_string(), "No branch named \"nope\".");
}

#[test]
fn an_unborn_head_has_nothing_to_compare_with() {
    let (_dir, repo) = init_repo();
    let only = commit_tree(&repo, &[("x.txt", "x\n", REGULAR)], &[], "only");
    repo.reference("refs/heads/feature", only, true, "test").unwrap();
    // HEAD still points at the unborn default branch.

    let error = branch_compare_range(&repo, "refs/heads/feature").unwrap_err();

    assert!(matches!(error, DiffError::UnbornHead));
    assert_eq!(error.to_string(), "There is no commit checked out to compare with.");
}
```

Note `init_repo` sets `core.autocrlf=false`, so the working-tree contents compare byte for byte on Windows CI too.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p git-core --test compare`
Expected: FAIL to compile (missing functions, `CompareRange`, variants).

- [ ] **Step 3: Implement**

Extend `DiffError`:

```rust
#[derive(Debug, Error)]
pub enum DiffError {
    #[error("failed to compute diff: {0}")]
    Diff(#[from] git2::Error),
    #[error("There is no commit checked out to compare with.")]
    UnbornHead,
    /// The short ref name (without `refs/heads/` / `refs/remotes/`).
    #[error("{0} and HEAD have no common ancestor.")]
    NoMergeBase(String),
    #[error("No branch named \"{0}\".")]
    UnknownRef(String),
}
```

Add after `ChangedFile`:

```rust
/// Resolved ends of a branch compare: the merge base with HEAD, and the branch tip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompareRange {
    pub from_id: String,
    pub to_id: String,
}
```

Add after `range_diff`:

```rust
/// The working tree compares like the Uncommitted view: staged, unstaged and untracked files.
fn include_untracked_content(git_opts: &mut git2::DiffOptions) {
    git_opts
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .show_untracked_content(true);
}

/// Paths that differ between `commit_id` and the working tree (`git diff <commit>` plus
/// untracked files), with renames detected among tracked files.
pub fn compare_working_tree_files(repo: &Repository, commit_id: &str) -> Result<Vec<ChangedFile>, DiffError> {
    let tree = commit_tree(repo, commit_id)?;
    let mut git_opts = git2::DiffOptions::new();
    include_untracked_content(&mut git_opts);
    let mut diff = repo.diff_tree_to_workdir_with_index(Some(&tree), Some(&mut git_opts))?;
    detect_renames(&mut diff)?;
    Ok(changed_files(&diff))
}

/// One file's diff from `commit_id` to the working tree; `old_path` as in `range_diff`.
pub fn compare_working_tree(
    repo: &Repository,
    commit_id: &str,
    path: &str,
    old_path: Option<&str>,
    opts: &DiffOptions,
) -> Result<Vec<DiffHunk>, DiffError> {
    let tree = commit_tree(repo, commit_id)?;
    let mut git_opts = literal_path_options(path, old_path, opts);
    include_untracked_content(&mut git_opts);
    let mut diff = repo.diff_tree_to_workdir_with_index(Some(&tree), Some(&mut git_opts))?;
    if old_path.is_some() {
        detect_renames(&mut diff)?;
    }
    hunks_from_diff(&diff)
}

/// `merge-base(HEAD, ref)..ref`: what `ref_name` (a full `refs/heads/…` or `refs/remotes/…`
/// name) changed since it diverged from HEAD. A detached HEAD is used as is.
pub fn branch_compare_range(repo: &Repository, ref_name: &str) -> Result<CompareRange, DiffError> {
    let short = ref_name
        .strip_prefix("refs/heads/")
        .or_else(|| ref_name.strip_prefix("refs/remotes/"))
        .unwrap_or(ref_name)
        .to_string();
    let head = match repo.head() {
        Ok(head) => head.peel_to_commit()?.id(),
        Err(e) if matches!(e.code(), git2::ErrorCode::UnbornBranch | git2::ErrorCode::NotFound) => {
            return Err(DiffError::UnbornHead)
        }
        Err(e) => return Err(e.into()),
    };
    let tip = match repo.find_reference(ref_name) {
        Ok(reference) => reference.peel_to_commit()?.id(),
        Err(e) if e.code() == git2::ErrorCode::NotFound => return Err(DiffError::UnknownRef(short)),
        Err(e) => return Err(e.into()),
    };
    let base = match repo.merge_base(head, tip) {
        Ok(base) => base,
        Err(e) if e.code() == git2::ErrorCode::NotFound => return Err(DiffError::NoMergeBase(short)),
        Err(e) => return Err(e.into()),
    };
    Ok(CompareRange { from_id: base.to_string(), to_id: tip.to_string() })
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p git-core`
Expected: PASS.

- [ ] **Step 5: Lint and commit**

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`

```bash
git add crates/git-core
git commit -m "feat(git-core): compare a commit with the working tree and a branch since it diverged"
```

---

### Task 3: `repo-service` compare commands

**Files:**
- Create: `crates/repo-service/src/worker/compare.rs`
- Modify: `crates/repo-service/src/worker/mod.rs` (add `mod compare;` to the module list, five `Command` variants after `GetCommitRenameDiff`, dispatch arms after the `GetCommitRenameDiff` arm, and tests after `get_file_history_and_rename_diff_round_trip_through_the_worker`)

**Interfaces:**
- Consumes: Task 1–2 functions.
- Produces (used by Task 4):

```rust
impl WorkerHandle {
    pub fn get_range_files(&self, from_id: String, to_id: String) -> Result<Vec<ChangedFile>, String>;
    pub fn get_range_diff(&self, from_id: String, to_id: String, path: String, old_path: Option<String>, opts: DiffOptions) -> Result<Vec<DiffHunk>, String>;
    pub fn get_working_tree_compare_files(&self, commit_id: String) -> Result<Vec<ChangedFile>, String>;
    pub fn get_working_tree_compare_diff(&self, commit_id: String, path: String, old_path: Option<String>, opts: DiffOptions) -> Result<Vec<DiffHunk>, String>;
    pub fn get_branch_compare_range(&self, ref_name: String) -> Result<CompareRange, String>;
}
```

- [ ] **Step 1: Write the failing worker tests**

In `mod.rs`'s test module:

```rust
    #[test]
    fn range_and_working_tree_compares_round_trip_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "one\n");
        commit_all(&repo, "first");
        let first = repo.head().unwrap().peel_to_commit().unwrap().id().to_string();
        write_file(dir.path(), "a.txt", "two\n");
        commit_all(&repo, "second");
        let second = repo.head().unwrap().peel_to_commit().unwrap().id().to_string();
        write_file(dir.path(), "a.txt", "three\n");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        let a = git_core::diff::ChangedFile { path: "a.txt".into(), old_path: None };
        let opts = git_core::diff::DiffOptions::default();

        assert_eq!(handle.get_range_files(first.clone(), second.clone()).unwrap(), vec![a.clone()]);
        let hunks = handle
            .get_range_diff(first.clone(), second, "a.txt".into(), None, opts)
            .unwrap();
        assert!(hunks[0].lines.iter().any(|l| l.content == "two\n"));

        assert_eq!(handle.get_working_tree_compare_files(first.clone()).unwrap(), vec![a]);
        let hunks = handle
            .get_working_tree_compare_diff(first, "a.txt".into(), None, opts)
            .unwrap();
        assert!(hunks[0].lines.iter().any(|l| l.content == "three\n"));
    }

    #[test]
    fn branch_compare_range_round_trips_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "one\n");
        commit_all(&repo, "base");
        let base = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("feature", &base, false).unwrap();
        write_file(dir.path(), "a.txt", "two\n");
        commit_all(&repo, "main only");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();

        let range = handle.get_branch_compare_range("refs/heads/feature".into()).unwrap();
        assert_eq!(range.from_id, base.id().to_string());
        assert_eq!(range.to_id, base.id().to_string());
        assert_eq!(
            handle.get_branch_compare_range("refs/heads/nope".into()).unwrap_err(),
            "No branch named \"nope\"."
        );
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p repo-service compare`
Expected: FAIL to compile (`get_range_files` etc. missing).

- [ ] **Step 3: Implement**

`mod.rs`, the `Command` variants (after `GetCommitRenameDiff`):

```rust
    GetRangeFiles {
        from_id: String,
        to_id: String,
        reply: Sender<Result<Vec<git_core::diff::ChangedFile>, String>>,
    },
    GetRangeDiff {
        from_id: String,
        to_id: String,
        path: String,
        old_path: Option<String>,
        opts: git_core::diff::DiffOptions,
        reply: Sender<Result<Vec<DiffHunk>, String>>,
    },
    GetWorkingTreeCompareFiles {
        commit_id: String,
        reply: Sender<Result<Vec<git_core::diff::ChangedFile>, String>>,
    },
    GetWorkingTreeCompareDiff {
        commit_id: String,
        path: String,
        old_path: Option<String>,
        opts: git_core::diff::DiffOptions,
        reply: Sender<Result<Vec<DiffHunk>, String>>,
    },
    GetBranchCompareRange {
        ref_name: String,
        reply: Sender<Result<git_core::diff::CompareRange, String>>,
    },
```

Dispatch arms (after the `GetCommitRenameDiff` arm):

```rust
                    Command::GetRangeFiles { from_id, to_id, reply } => {
                        compare::get_range_files(&repo, from_id, to_id, reply)
                    }
                    Command::GetRangeDiff { from_id, to_id, path, old_path, opts, reply } => {
                        compare::get_range_diff(&repo, from_id, to_id, path, old_path, opts, reply)
                    }
                    Command::GetWorkingTreeCompareFiles { commit_id, reply } => {
                        compare::get_working_tree_compare_files(&repo, commit_id, reply)
                    }
                    Command::GetWorkingTreeCompareDiff { commit_id, path, old_path, opts, reply } => {
                        compare::get_working_tree_compare_diff(&repo, commit_id, path, old_path, opts, reply)
                    }
                    Command::GetBranchCompareRange { ref_name, reply } => {
                        compare::get_branch_compare_range(&repo, ref_name, reply)
                    }
```

`worker/compare.rs`:

```rust
//! Read-only compares: two commits, a commit and the working tree, and a branch since it
//! diverged from HEAD. None of these record an undo entry.

use std::sync::mpsc::Sender;

use git_core::diff::{ChangedFile, CompareRange, DiffHunk, DiffOptions};

use super::{Command, WorkerHandle};

pub(super) fn get_range_files(
    repo: &git2::Repository,
    from_id: String,
    to_id: String,
    reply: Sender<Result<Vec<ChangedFile>, String>>,
) {
    let _ = reply.send(
        git_core::diff::range_diff_files(repo, &from_id, &to_id).map_err(|error| error.to_string()),
    );
}

pub(super) fn get_range_diff(
    repo: &git2::Repository,
    from_id: String,
    to_id: String,
    path: String,
    old_path: Option<String>,
    opts: DiffOptions,
    reply: Sender<Result<Vec<DiffHunk>, String>>,
) {
    let _ = reply.send(
        git_core::diff::range_diff(repo, &from_id, &to_id, &path, old_path.as_deref(), &opts)
            .map_err(|error| error.to_string()),
    );
}

pub(super) fn get_working_tree_compare_files(
    repo: &git2::Repository,
    commit_id: String,
    reply: Sender<Result<Vec<ChangedFile>, String>>,
) {
    let _ = reply.send(
        git_core::diff::compare_working_tree_files(repo, &commit_id)
            .map_err(|error| error.to_string()),
    );
}

pub(super) fn get_working_tree_compare_diff(
    repo: &git2::Repository,
    commit_id: String,
    path: String,
    old_path: Option<String>,
    opts: DiffOptions,
    reply: Sender<Result<Vec<DiffHunk>, String>>,
) {
    let _ = reply.send(
        git_core::diff::compare_working_tree(repo, &commit_id, &path, old_path.as_deref(), &opts)
            .map_err(|error| error.to_string()),
    );
}

pub(super) fn get_branch_compare_range(
    repo: &git2::Repository,
    ref_name: String,
    reply: Sender<Result<CompareRange, String>>,
) {
    let _ = reply.send(
        git_core::diff::branch_compare_range(repo, &ref_name).map_err(|error| error.to_string()),
    );
}

impl WorkerHandle {
    fn request<T>(&self, command: impl FnOnce(Sender<Result<T, String>>) -> Command) -> Result<T, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(command(tx))
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn get_range_files(&self, from_id: String, to_id: String) -> Result<Vec<ChangedFile>, String> {
        self.request(|reply| Command::GetRangeFiles { from_id, to_id, reply })
    }

    pub fn get_range_diff(
        &self,
        from_id: String,
        to_id: String,
        path: String,
        old_path: Option<String>,
        opts: DiffOptions,
    ) -> Result<Vec<DiffHunk>, String> {
        self.request(|reply| Command::GetRangeDiff { from_id, to_id, path, old_path, opts, reply })
    }

    pub fn get_working_tree_compare_files(&self, commit_id: String) -> Result<Vec<ChangedFile>, String> {
        self.request(|reply| Command::GetWorkingTreeCompareFiles { commit_id, reply })
    }

    pub fn get_working_tree_compare_diff(
        &self,
        commit_id: String,
        path: String,
        old_path: Option<String>,
        opts: DiffOptions,
    ) -> Result<Vec<DiffHunk>, String> {
        self.request(|reply| Command::GetWorkingTreeCompareDiff { commit_id, path, old_path, opts, reply })
    }

    pub fn get_branch_compare_range(&self, ref_name: String) -> Result<CompareRange, String> {
        self.request(|reply| Command::GetBranchCompareRange { ref_name, reply })
    }
}
```

If `WorkerHandle` already has a generic request helper by another name (check with `graft skeleton crates/repo-service/src/worker/mod.rs`), use it and drop `request`. Otherwise keep `request` private to this module, as above.

- [ ] **Step 4: Run tests**

Run: `cargo test -p repo-service`
Expected: PASS.

- [ ] **Step 5: Lint and commit**

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`

```bash
git add crates/repo-service
git commit -m "feat(repo-service): range, working-tree and branch compare worker commands"
```

---

### Task 4: Tauri commands and sidecar methods

**Files:**
- Create: `crates/tauri-app/src/commands/compare.rs`
- Modify: `crates/tauri-app/src/commands/mod.rs` (add `mod compare;` and `pub use compare::{get_branch_compare_range, get_range_diff, get_range_files, get_working_tree_compare_diff, get_working_tree_compare_files};`, then `ChangedFileDto`/`CompareRangeDto` after `SearchPageDto`'s `impl`, and a test after `search_page_dto_serializes_camel_case_and_null_cursor`). Also modify `crates/tauri-app/src/main.rs` (the `use` list and `generate_handler![…]`, next to `get_commit_rename_diff`).
- Modify: `crates/vscode-sidecar/src/dispatch.rs` (method table after `"get_commit_rename_diff"`, functions + DTOs after `fn get_commit_rename_diff`), `crates/vscode-sidecar/tests/protocol_roundtrip.rs` (new test after `working_and_commit_diff_round_trip_through_the_sidecar`)

**Interfaces:**
- Consumes: Task 3's `WorkerHandle` methods.
- Produces: the wire methods in Global Constraints.

- [ ] **Step 1: Write the failing Tauri DTO test**

```rust
    #[test]
    fn compare_dtos_serialize_camel_case_with_a_null_old_path() {
        let plain = serde_json::to_value(ChangedFileDto::from(git_core::diff::ChangedFile {
            path: "a.txt".into(),
            old_path: None,
        }))
        .unwrap();
        assert_eq!(plain, serde_json::json!({ "path": "a.txt", "oldPath": null }));

        let renamed = serde_json::to_value(ChangedFileDto::from(git_core::diff::ChangedFile {
            path: "new.txt".into(),
            old_path: Some("old.txt".into()),
        }))
        .unwrap();
        assert_eq!(renamed["oldPath"], "old.txt");

        let range = serde_json::to_value(CompareRangeDto::from(git_core::diff::CompareRange {
            from_id: "base".into(),
            to_id: "tip".into(),
        }))
        .unwrap();
        assert_eq!(range, serde_json::json!({ "fromId": "base", "toId": "tip" }));
    }
```

Add `ChangedFileDto, CompareRangeDto` to the test module's `use super::{…}` list.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p tauri-app compare_dtos`
Expected: FAIL to compile.

- [ ] **Step 3: Implement the Tauri side**

`commands/mod.rs`:

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFileDto {
    pub path: String,
    // Serialized as `null` (not omitted) — the TS type is `string | null`.
    pub old_path: Option<String>,
}

impl From<git_core::diff::ChangedFile> for ChangedFileDto {
    fn from(f: git_core::diff::ChangedFile) -> Self {
        ChangedFileDto { path: f.path, old_path: f.old_path }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareRangeDto {
    pub from_id: String,
    pub to_id: String,
}

impl From<git_core::diff::CompareRange> for CompareRangeDto {
    fn from(r: git_core::diff::CompareRange) -> Self {
        CompareRangeDto { from_id: r.from_id, to_id: r.to_id }
    }
}
```

`commands/compare.rs`:

```rust
use super::{worker_handle, AppState, ChangedFileDto, CompareRangeDto, DiffHunkDto, DiffOptionsDto};
use tauri::State;

#[tauri::command]
pub async fn get_range_files(
    repo_path: String,
    from_id: String,
    to_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<ChangedFileDto>, String> {
    Ok(worker_handle(&state, &repo_path)?
        .get_range_files(from_id, to_id)?
        .into_iter()
        .map(ChangedFileDto::from)
        .collect())
}

#[tauri::command]
pub async fn get_range_diff(
    repo_path: String,
    from_id: String,
    to_id: String,
    path: String,
    old_path: Option<String>,
    opts: DiffOptionsDto,
    state: State<'_, AppState>,
) -> Result<Vec<DiffHunkDto>, String> {
    Ok(worker_handle(&state, &repo_path)?
        .get_range_diff(from_id, to_id, path, old_path, opts.into())?
        .into_iter()
        .map(DiffHunkDto::from)
        .collect())
}

#[tauri::command]
pub async fn get_working_tree_compare_files(
    repo_path: String,
    commit_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<ChangedFileDto>, String> {
    Ok(worker_handle(&state, &repo_path)?
        .get_working_tree_compare_files(commit_id)?
        .into_iter()
        .map(ChangedFileDto::from)
        .collect())
}

#[tauri::command]
pub async fn get_working_tree_compare_diff(
    repo_path: String,
    commit_id: String,
    path: String,
    old_path: Option<String>,
    opts: DiffOptionsDto,
    state: State<'_, AppState>,
) -> Result<Vec<DiffHunkDto>, String> {
    Ok(worker_handle(&state, &repo_path)?
        .get_working_tree_compare_diff(commit_id, path, old_path, opts.into())?
        .into_iter()
        .map(DiffHunkDto::from)
        .collect())
}

#[tauri::command]
pub async fn get_branch_compare_range(
    repo_path: String,
    ref_name: String,
    state: State<'_, AppState>,
) -> Result<CompareRangeDto, String> {
    Ok(worker_handle(&state, &repo_path)?
        .get_branch_compare_range(ref_name)?
        .into())
}
```

Register all five in `main.rs` (`use` list and `generate_handler!`).

- [ ] **Step 4: Write the failing sidecar round-trip test**

```rust
#[test]
fn compare_methods_round_trip_through_the_sidecar() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "one\n");
    commit_all(&repo, "first");
    let first = repo.head().unwrap().peel_to_commit().unwrap().id().to_string();
    write_file(dir.path(), "a.txt", "two\n");
    commit_all(&repo, "second");
    let second = repo.head().unwrap().peel_to_commit().unwrap().id().to_string();
    write_file(dir.path(), "a.txt", "three\n");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let opts = serde_json::json!({"ignoreWhitespace": false, "contextLines": 3});
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let files = sidecar.call(
        2,
        "get_range_files",
        serde_json::json!({"repoPath": repo_path, "fromId": first, "toId": second}),
    );
    assert_eq!(files["result"], serde_json::json!([{"path": "a.txt", "oldPath": null}]));

    let range_diff = sidecar.call(
        3,
        "get_range_diff",
        serde_json::json!({"repoPath": repo_path, "fromId": first, "toId": second, "path": "a.txt", "oldPath": null, "opts": opts}),
    );
    assert_eq!(range_diff["result"].as_array().unwrap().len(), 1);

    let wt_files = sidecar.call(
        4,
        "get_working_tree_compare_files",
        serde_json::json!({"repoPath": repo_path, "commitId": first}),
    );
    assert_eq!(wt_files["result"][0]["path"], "a.txt");

    let wt_diff = sidecar.call(
        5,
        "get_working_tree_compare_diff",
        serde_json::json!({"repoPath": repo_path, "commitId": first, "path": "a.txt", "oldPath": null, "opts": opts}),
    );
    assert_eq!(wt_diff["result"][0]["lines"][1]["content"], "three\n");

    let range = sidecar.call(
        6,
        "get_branch_compare_range",
        serde_json::json!({"repoPath": repo_path, "refName": "refs/heads/nope"}),
    );
    assert_eq!(range["error"]["message"], "No branch named \"nope\".");
}
```

The `lines[1]` index assumes the hunk is `Remove "one"`, `Add "three"`. If the assertion reads a different line, assert with `.iter().any(...)` instead, as the worker test does.

- [ ] **Step 5: Implement the sidecar methods**

Method table entries:

```rust
        "get_range_files" => get_range_files(params, repos),
        "get_range_diff" => get_range_diff(params, repos),
        "get_working_tree_compare_files" => get_working_tree_compare_files(params, repos),
        "get_working_tree_compare_diff" => get_working_tree_compare_diff(params, repos),
        "get_branch_compare_range" => get_branch_compare_range(params, repos),
```

Functions and DTOs (after `fn get_commit_rename_diff`):

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChangedFileDto {
    path: String,
    old_path: Option<String>,
}

impl From<git_core::diff::ChangedFile> for ChangedFileDto {
    fn from(f: git_core::diff::ChangedFile) -> Self {
        ChangedFileDto { path: f.path, old_path: f.old_path }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CompareRangeDto {
    from_id: String,
    to_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetRangeFilesParams {
    repo_path: String,
    from_id: String,
    to_id: String,
}

fn get_range_files(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: GetRangeFilesParams = serde_json::from_value(params).map_err(|error| error.to_string())?;
    let files: Vec<ChangedFileDto> = worker_handle(repos, &params.repo_path)?
        .get_range_files(params.from_id, params.to_id)?
        .into_iter()
        .map(ChangedFileDto::from)
        .collect();
    serde_json::to_value(files).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetRangeDiffParams {
    repo_path: String,
    from_id: String,
    to_id: String,
    path: String,
    old_path: Option<String>,
    opts: DiffOptionsDto,
}

fn get_range_diff(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: GetRangeDiffParams = serde_json::from_value(params).map_err(|error| error.to_string())?;
    let hunks: Vec<DiffHunkDto> = worker_handle(repos, &params.repo_path)?
        .get_range_diff(params.from_id, params.to_id, params.path, params.old_path, params.opts.into())?
        .into_iter()
        .map(DiffHunkDto::from)
        .collect();
    serde_json::to_value(hunks).map_err(|error| error.to_string())
}

fn get_working_tree_compare_files(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: GetCommitFilesParams = serde_json::from_value(params).map_err(|error| error.to_string())?;
    let files: Vec<ChangedFileDto> = worker_handle(repos, &params.repo_path)?
        .get_working_tree_compare_files(params.commit_id)?
        .into_iter()
        .map(ChangedFileDto::from)
        .collect();
    serde_json::to_value(files).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetWorkingTreeCompareDiffParams {
    repo_path: String,
    commit_id: String,
    path: String,
    old_path: Option<String>,
    opts: DiffOptionsDto,
}

fn get_working_tree_compare_diff(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: GetWorkingTreeCompareDiffParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let hunks: Vec<DiffHunkDto> = worker_handle(repos, &params.repo_path)?
        .get_working_tree_compare_diff(params.commit_id, params.path, params.old_path, params.opts.into())?
        .into_iter()
        .map(DiffHunkDto::from)
        .collect();
    serde_json::to_value(hunks).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetBranchCompareRangeParams {
    repo_path: String,
    ref_name: String,
}

fn get_branch_compare_range(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: GetBranchCompareRangeParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let range = worker_handle(repos, &params.repo_path)?.get_branch_compare_range(params.ref_name)?;
    serde_json::to_value(CompareRangeDto { from_id: range.from_id, to_id: range.to_id })
        .map_err(|error| error.to_string())
}
```

(`GetCommitFilesParams` is the existing `{ repoPath, commitId }` params struct.)

- [ ] **Step 6: Run tests**

Run: `cargo test -p tauri-app compare_dtos && cargo test -p vscode-sidecar`
Expected: PASS.

- [ ] **Step 7: Lint and commit**

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`

```bash
git add crates/tauri-app crates/vscode-sidecar
git commit -m "feat: expose compare commands over Tauri and the VSCode sidecar"
```

---

### Task 5: `RepoClient` compare methods in both clients

**Files:**
- Modify: `frontend/src/ipc/RepoClient.ts` (`ChangedFile`, `CompareRange` after `FileHistoryEntry`; methods after `getCommitFiles`), `frontend/src/ipc/tauriRepoClient.ts` and `frontend/src/ipc/vscodeRepoClient.ts` (after `getCommitFiles`)
- Test: `frontend/src/ipc/tauriRepoClient.test.ts`, `frontend/src/ipc/vscodeRepoClient.test.ts`
- Fixtures: every full `RepoClient` literal that `tsc` reports (at least `App.test.tsx`, `useAppState.test.ts`, `DiffPane.test.tsx`, `ConflictResolutionPane.test.tsx`, `RebasePlanner.test.tsx`, `RepoPicker.test.tsx`)

**Interfaces:**
- Produces (used by Tasks 6–10):

```ts
export interface ChangedFile { path: string; oldPath: string | null; }
export interface CompareRange { fromId: string; toId: string; }
getRangeFiles(repoPath: string, fromId: string, toId: string): Promise<ChangedFile[]>;
getRangeDiff(repoPath: string, fromId: string, toId: string, path: string, oldPath: string | null, opts: DiffOptions): Promise<DiffHunk[]>;
getWorkingTreeCompareFiles(repoPath: string, commitId: string): Promise<ChangedFile[]>;
getWorkingTreeCompareDiff(repoPath: string, commitId: string, path: string, oldPath: string | null, opts: DiffOptions): Promise<DiffHunk[]>;
getBranchCompareRange(repoPath: string, refName: string): Promise<CompareRange>;
```

- [ ] **Step 1: Write the failing client tests**

`tauriRepoClient.test.ts`, inside the describe that holds the search test:

```ts
  it("invokes the compare commands with camelCase args", async () => {
    const opts = { ignoreWhitespace: false, contextLines: 3 };
    vi.mocked(invoke).mockResolvedValue([]);
    await tauriRepoClient.getRangeFiles("/repo", "a", "b");
    expect(invoke).toHaveBeenLastCalledWith("get_range_files", { repoPath: "/repo", fromId: "a", toId: "b" });
    await tauriRepoClient.getRangeDiff("/repo", "a", "b", "new.txt", "old.txt", opts);
    expect(invoke).toHaveBeenLastCalledWith("get_range_diff", {
      repoPath: "/repo",
      fromId: "a",
      toId: "b",
      path: "new.txt",
      oldPath: "old.txt",
      opts,
    });
    await tauriRepoClient.getWorkingTreeCompareFiles("/repo", "a");
    expect(invoke).toHaveBeenLastCalledWith("get_working_tree_compare_files", { repoPath: "/repo", commitId: "a" });
    await tauriRepoClient.getWorkingTreeCompareDiff("/repo", "a", "x.txt", null, opts);
    expect(invoke).toHaveBeenLastCalledWith("get_working_tree_compare_diff", {
      repoPath: "/repo",
      commitId: "a",
      path: "x.txt",
      oldPath: null,
      opts,
    });
    vi.mocked(invoke).mockResolvedValue({ fromId: "base", toId: "tip" });
    await expect(tauriRepoClient.getBranchCompareRange("/repo", "refs/heads/feature")).resolves.toEqual({
      fromId: "base",
      toId: "tip",
    });
    expect(invoke).toHaveBeenLastCalledWith("get_branch_compare_range", { repoPath: "/repo", refName: "refs/heads/feature" });
  });
```

`vscodeRepoClient.test.ts`, after "wires getCommitFiles":

```ts
  it("wires the compare methods", async () => {
    const opts = { ignoreWhitespace: true, contextLines: 5 };
    const files = vscodeRepoClient.getRangeFiles("/repo", "a", "b");
    expect(postMessage).toHaveBeenLastCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "get_range_files",
      params: { repoPath: "/repo", fromId: "a", toId: "b" },
    });
    respond(1, [{ path: "new.txt", oldPath: "old.txt" }]);
    await expect(files).resolves.toEqual([{ path: "new.txt", oldPath: "old.txt" }]);

    void vscodeRepoClient.getRangeDiff("/repo", "a", "b", "new.txt", "old.txt", opts);
    expect(postMessage).toHaveBeenLastCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "get_range_diff",
      params: { repoPath: "/repo", fromId: "a", toId: "b", path: "new.txt", oldPath: "old.txt", opts },
    });
    void vscodeRepoClient.getWorkingTreeCompareFiles("/repo", "a");
    expect(postMessage).toHaveBeenLastCalledWith({
      jsonrpc: "2.0",
      id: 3,
      method: "get_working_tree_compare_files",
      params: { repoPath: "/repo", commitId: "a" },
    });
    void vscodeRepoClient.getWorkingTreeCompareDiff("/repo", "a", "x.txt", null, opts);
    expect(postMessage).toHaveBeenLastCalledWith({
      jsonrpc: "2.0",
      id: 4,
      method: "get_working_tree_compare_diff",
      params: { repoPath: "/repo", commitId: "a", path: "x.txt", oldPath: null, opts },
    });
    const range = vscodeRepoClient.getBranchCompareRange("/repo", "refs/remotes/origin/main");
    expect(postMessage).toHaveBeenLastCalledWith({
      jsonrpc: "2.0",
      id: 5,
      method: "get_branch_compare_range",
      params: { repoPath: "/repo", refName: "refs/remotes/origin/main" },
    });
    respond(5, { fromId: "base", toId: "tip" });
    await expect(range).resolves.toEqual({ fromId: "base", toId: "tip" });
  });
```

If the file's `respond` helper or request ids work differently (check "wires searchCommits and cancelSearch"), follow that test's pattern.

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/ipc`
Expected: FAIL (`getRangeFiles is not a function`).

- [ ] **Step 3: Implement**

`RepoClient.ts`:

```ts
/** One changed path in a compare; `oldPath` is set when the file was renamed (then `path` is the new name). */
export interface ChangedFile {
  path: string;
  oldPath: string | null;
}

/** Resolved ends of a branch compare: the merge base with HEAD, and the branch tip. */
export interface CompareRange {
  fromId: string;
  toId: string;
}
```

Methods, after `getCommitFiles`:

```ts
  /** Files that differ between two commits' trees (`git diff from to`), renames detected. */
  getRangeFiles(repoPath: string, fromId: string, toId: string): Promise<ChangedFile[]>;
  /** One file's diff between two commits; pass `oldPath` from `getRangeFiles` for a rename. */
  getRangeDiff(repoPath: string, fromId: string, toId: string, path: string, oldPath: string | null, opts: DiffOptions): Promise<DiffHunk[]>;
  /** Files that differ between a commit and the working tree (staged, unstaged and untracked). */
  getWorkingTreeCompareFiles(repoPath: string, commitId: string): Promise<ChangedFile[]>;
  getWorkingTreeCompareDiff(repoPath: string, commitId: string, path: string, oldPath: string | null, opts: DiffOptions): Promise<DiffHunk[]>;
  /** `merge-base(HEAD, refName)..refName` for a full ref name (`refs/heads/…`, `refs/remotes/…`). */
  getBranchCompareRange(repoPath: string, refName: string): Promise<CompareRange>;
```

`tauriRepoClient.ts` (use `loggedInvoke`, like `getCommitFiles`):

```ts
  getRangeFiles: (repoPath: string, fromId: string, toId: string) =>
    loggedInvoke<ChangedFile[]>("get_range_files", { repoPath, fromId, toId }),
  getRangeDiff: (repoPath: string, fromId: string, toId: string, path: string, oldPath: string | null, opts: DiffOptions) =>
    loggedInvoke<DiffHunk[]>("get_range_diff", { repoPath, fromId, toId, path, oldPath, opts }),
  getWorkingTreeCompareFiles: (repoPath: string, commitId: string) =>
    loggedInvoke<ChangedFile[]>("get_working_tree_compare_files", { repoPath, commitId }),
  getWorkingTreeCompareDiff: (repoPath: string, commitId: string, path: string, oldPath: string | null, opts: DiffOptions) =>
    loggedInvoke<DiffHunk[]>("get_working_tree_compare_diff", { repoPath, commitId, path, oldPath, opts }),
  getBranchCompareRange: (repoPath: string, refName: string) =>
    loggedInvoke<CompareRange>("get_branch_compare_range", { repoPath, refName }),
```

`vscodeRepoClient.ts`: the same five with `call<…>(method, params)`.

- [ ] **Step 4: Fix full `RepoClient` fixtures**

Run: `cd frontend && pnpm exec tsc --noEmit -p .`
For every literal that errors, add `getRangeFiles: async () => [], getRangeDiff: async () => [], getWorkingTreeCompareFiles: async () => [], getWorkingTreeCompareDiff: async () => [], getBranchCompareRange: async () => ({ fromId: "", toId: "" }),`. In fixtures built on `unused` (`App.test.tsx`, `DiffPane.test.tsx`), use `unused` for all five. Repeat until `tsc` is clean.

- [ ] **Step 5: Run tests and lint**

Run: `cd frontend && pnpm test -- --run && pnpm lint`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): RepoClient compare methods in both clients"
```

---

### Task 6: Extract `ReadOnlyFileSection` (and `CollapseToggle`)

Pure refactor plus one new capability (the diff source is a callback). `DiffPane.test.tsx` must pass **unchanged**.

**Files:**
- Create: `frontend/src/components/CollapseToggle.tsx`, `frontend/src/components/ReadOnlyFileSection.tsx`, `frontend/src/components/ReadOnlyFileSection.test.tsx`
- Modify: `frontend/src/components/DiffPane.tsx` (delete `CollapseToggle`, `CollapseAllToggle`, `CommitFileSection`; import the moved pieces; `CommitDiffPane` renders `ReadOnlyFileSection`)

**Interfaces:**
- Produces (used by Task 8):

```ts
export function CollapseToggle(props: { collapsed: boolean; path: string; onToggle: () => void }): JSX.Element;
export function CollapseAllToggle(props: { allCollapsed: boolean; onToggle: () => void }): JSX.Element;
export function ReadOnlyFileSection(props: {
  repoPath: string;
  client: RepoClient;
  path: string;
  oldPath?: string | null;
  /** Identifies the diff source; a change refetches. */
  diffKey: string;
  fetchDiff: (opts: DiffOptions) => Promise<DiffHunk[]>;
  /** Revision Blame runs at (a commit id or "HEAD"). */
  blameAt: string;
  collapsed: boolean;
  onToggleCollapse: () => void;
  onSelectRow: (row: SelectedRow) => void;
  onOpenFileHistory?: (path: string) => void;
}): JSX.Element;
```

- [ ] **Step 1: Write the failing section tests**

`ReadOnlyFileSection.test.tsx`:

```tsx
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RepoClient } from "../ipc/RepoClient";
import { ReadOnlyFileSection } from "./ReadOnlyFileSection";

function renderSection(overrides: Partial<Parameters<typeof ReadOnlyFileSection>[0]> = {}) {
  const props: Parameters<typeof ReadOnlyFileSection>[0] = {
    repoPath: "/repo",
    client: { getBlame: vi.fn(async () => []) } as unknown as RepoClient,
    path: "a.txt",
    diffKey: "k1",
    fetchDiff: vi.fn(async () => []),
    blameAt: "abc",
    collapsed: false,
    onToggleCollapse: vi.fn(),
    onSelectRow: vi.fn(),
    ...overrides,
  };
  return { ...render(<ReadOnlyFileSection {...props} />), props };
}

describe("ReadOnlyFileSection", () => {
  it("fetches through fetchDiff with the diff settings and refetches when diffKey changes", async () => {
    const { props, rerender } = renderSection();
    await waitFor(() => expect(props.fetchDiff).toHaveBeenCalledWith({ ignoreWhitespace: false, contextLines: 3 }));

    rerender(<ReadOnlyFileSection {...props} diffKey="k2" />);
    await waitFor(() => expect(props.fetchDiff).toHaveBeenCalledTimes(2));
  });

  it("does not fetch while collapsed", () => {
    const { props } = renderSection({ collapsed: true });
    expect(props.fetchDiff).not.toHaveBeenCalled();
  });

  it("blames at blameAt", async () => {
    const getBlame = vi.fn(async () => []);
    renderSection({ client: { getBlame } as unknown as RepoClient, blameAt: "HEAD" });
    fireEvent.click(screen.getByRole("button", { name: "Blame" }));
    await waitFor(() => expect(getBlame).toHaveBeenCalledWith("/repo", "HEAD", "a.txt"));
  });

  it("shows the rename and a History button when given", () => {
    const onOpenFileHistory = vi.fn();
    renderSection({ path: "new.txt", oldPath: "old.txt", onOpenFileHistory });
    expect(screen.getByText("Renamed from old.txt")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "History of new.txt" }));
    expect(onOpenFileHistory).toHaveBeenCalledWith("new.txt");
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/components/ReadOnlyFileSection`
Expected: FAIL (module not found).

- [ ] **Step 3: Implement**

- `CollapseToggle.tsx`: move `CollapseToggle` and `CollapseAllToggle` verbatim from `DiffPane.tsx`, `export` both, and import `ChevronDown`, `ChevronRight` and `styles from "./DiffPane.module.css"` (the `collapseToggle` class stays in that stylesheet). Import them in `DiffPane.tsx`, and drop `ChevronDown`/`ChevronRight` from its lucide import if nothing else uses them.
- `ReadOnlyFileSection.tsx`: move `CommitFileSection`'s body verbatim, then change only:
  - the props to the interface above;
  - `loadDiff`: `const request = fetchDiff(diffOptions);` (in place of the `getCommitRenameDiff`/`getCommitDiff` choice);
  - the diff effect's deps: `[diffKey, path, oldPath, mode, collapsed, settings.ignoreWhitespace, settings.contextLines]` (keep the existing `eslint-disable-next-line react-hooks/exhaustive-deps`; `fetchDiff` is a fresh closure each render and `diffKey` stands for it);
  - `loadBlame`: `client.getBlame(repoPath, blameAt, path)`, and the blame effect's deps `[repoPath, client, blameAt, path, mode]`.
  Keep the doc comment short: "One file's diff in a read-only view (a commit or a compare), with Blame and History. `fetchDiff` runs again whenever `diffKey`, the path or the diff settings change."
- `DiffPane.tsx` `CommitDiffPane`: add

```tsx
  const fetchCommitDiff = (path: string, oldPath: string | null) => (opts: DiffOptions) =>
    oldPath != null
      ? client.getCommitRenameDiff(repoPath, commitId, oldPath, path, opts)
      : client.getCommitDiff(repoPath, commitId, path, opts);
```

  and replace both `<CommitFileSection …/>` usages with `<ReadOnlyFileSection … diffKey={`commit:${commitId}`} blameAt={commitId} fetchDiff={fetchCommitDiff(path, null)} />` (the focused one with `fetchCommitDiff(focus.path, focus.oldPath)` and `oldPath={focus.oldPath}`), keeping every other prop as it was.

- [ ] **Step 4: Run tests and lint**

Run: `cd frontend && pnpm test -- --run && pnpm lint`
Expected: PASS, with `DiffPane.test.tsx` untouched.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components
git commit -m "refactor(frontend): extract ReadOnlyFileSection from the commit diff pane"
```

---

### Task 7: `useCompare` hook

**Files:**
- Modify: `frontend/src/state/useAppState.ts` (export `rowsEqual`, moved from `CommitGraph.tsx`), `frontend/src/components/CommitGraph.tsx` (import it)
- Create: `frontend/src/state/useCompare.ts`, `frontend/src/state/useCompare.test.ts`

**Interfaces:**
- Consumes: `RepoClient.getBranchCompareRange` (Task 5).
- Produces (used by Tasks 8, 10):

```ts
export function rowsEqual(a: SelectedRow, b: SelectedRow): boolean;   // useAppState.ts
export type CompareTarget =
  | { kind: "commits"; fromId: string; toId: string }
  | { kind: "branch"; refName: string; label: string }
  | { kind: "workingTree"; commitId: string };
export interface CompareState {
  target: CompareTarget;
  range: { fromId: string; toId: string | null } | null;
  error: DescribedError | null;
}
export interface UseCompareResult {
  compare: CompareState | null;
  compareCommits(fromId: string, toId: string): void;
  compareBranch(refName: string, label: string): void;
  compareWorkingTree(commitId: string): void;
  swap(): void;
  close(): void;
  retry(): void;
}
export function useCompare(client: RepoClient, repoPath: string, refreshGeneration: number, selectedRow: SelectedRow): UseCompareResult;
```

- [ ] **Step 1: Move `rowsEqual`**

Cut `function rowsEqual` from `CommitGraph.tsx`, paste it into `useAppState.ts` after `export type SelectedRow`, as `export function rowsEqual`. Import it in `CommitGraph.tsx`.

Run: `cd frontend && pnpm test -- --run src/components/CommitGraph`
Expected: PASS.

- [ ] **Step 2: Write the failing hook tests**

`useCompare.test.ts`:

```ts
import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { CompareRange, RepoClient } from "../ipc/RepoClient";
import type { SelectedRow } from "./useAppState";
import { useCompare } from "./useCompare";

function setup(getBranchCompareRange: RepoClient["getBranchCompareRange"] = vi.fn()) {
  const client = { getBranchCompareRange } as unknown as RepoClient;
  return renderHook(
    ({ generation, row }: { generation: number; row: SelectedRow }) => useCompare(client, "/repo", generation, row),
    { initialProps: { generation: 0, row: { commitId: "b" } as SelectedRow } },
  );
}

describe("useCompare", () => {
  it("opens a commit compare and closes it", () => {
    const { result } = setup();
    act(() => result.current.compareCommits("a", "b"));
    expect(result.current.compare).toEqual({
      target: { kind: "commits", fromId: "a", toId: "b" },
      range: { fromId: "a", toId: "b" },
      error: null,
    });
    act(() => result.current.close());
    expect(result.current.compare).toBeNull();
  });

  it("swap flips from and to", () => {
    const { result } = setup();
    act(() => result.current.compareCommits("a", "b"));
    act(() => result.current.swap());
    expect(result.current.compare?.range).toEqual({ fromId: "b", toId: "a" });
  });

  it("a working-tree compare has no toId", () => {
    const { result } = setup();
    act(() => result.current.compareWorkingTree("a"));
    expect(result.current.compare?.range).toEqual({ fromId: "a", toId: null });
  });

  it("closes when the selected row changes and does not come back", () => {
    const { result, rerender } = setup();
    act(() => result.current.compareCommits("a", "b"));
    rerender({ generation: 0, row: { commitId: "c" } });
    expect(result.current.compare).toBeNull();
    rerender({ generation: 0, row: { commitId: "b" } });
    expect(result.current.compare).toBeNull();
  });

  it("resolves a branch compare and re-resolves on refresh", async () => {
    const getBranchCompareRange = vi
      .fn<(repoPath: string, refName: string) => Promise<CompareRange>>()
      .mockResolvedValueOnce({ fromId: "base", toId: "tip1" })
      .mockResolvedValueOnce({ fromId: "base", toId: "tip2" });
    const { result, rerender } = setup(getBranchCompareRange);
    act(() => result.current.compareBranch("refs/heads/feature", "feature"));
    expect(result.current.compare?.range).toBeNull();
    await waitFor(() => expect(result.current.compare?.range).toEqual({ fromId: "base", toId: "tip1" }));
    expect(getBranchCompareRange).toHaveBeenCalledWith("/repo", "refs/heads/feature");

    rerender({ generation: 1, row: { commitId: "b" } });
    await waitFor(() => expect(result.current.compare?.range).toEqual({ fromId: "base", toId: "tip2" }));
  });

  it("drops a stale branch resolution", async () => {
    let resolveFirst: (range: CompareRange) => void = () => {};
    const getBranchCompareRange = vi
      .fn<(repoPath: string, refName: string) => Promise<CompareRange>>()
      .mockImplementationOnce(() => new Promise((resolve) => (resolveFirst = resolve)))
      .mockResolvedValueOnce({ fromId: "base", toId: "other" });
    const { result } = setup(getBranchCompareRange);
    act(() => result.current.compareBranch("refs/heads/one", "one"));
    act(() => result.current.compareBranch("refs/heads/two", "two"));
    await waitFor(() => expect(result.current.compare?.range).toEqual({ fromId: "base", toId: "other" }));
    await act(async () => resolveFirst({ fromId: "base", toId: "stale" }));
    expect(result.current.compare?.range).toEqual({ fromId: "base", toId: "other" });
  });

  it("reports a resolve error and retries", async () => {
    const getBranchCompareRange = vi
      .fn<(repoPath: string, refName: string) => Promise<CompareRange>>()
      .mockRejectedValueOnce("orphan and HEAD have no common ancestor.")
      .mockResolvedValueOnce({ fromId: "base", toId: "tip" });
    const { result } = setup(getBranchCompareRange);
    act(() => result.current.compareBranch("refs/heads/orphan", "orphan"));
    await waitFor(() =>
      expect(result.current.compare?.error?.message).toBe("orphan and HEAD have no common ancestor."),
    );
    act(() => result.current.retry());
    await waitFor(() => expect(result.current.compare?.range).toEqual({ fromId: "base", toId: "tip" }));
    expect(result.current.compare?.error).toBeNull();
  });
});
```

- [ ] **Step 3: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/state/useCompare`
Expected: FAIL (module not found).

- [ ] **Step 4: Implement**

`useCompare.ts`:

```ts
import { useCallback, useEffect, useRef, useState } from "react";
import type { CompareRange, RepoClient } from "../ipc/RepoClient";
import { describeError, type DescribedError } from "../lib/errorMessages";
import { rowsEqual, type SelectedRow } from "./useAppState";

export type CompareTarget =
  | { kind: "commits"; fromId: string; toId: string }
  | { kind: "branch"; refName: string; label: string }
  | { kind: "workingTree"; commitId: string };

export interface CompareState {
  target: CompareTarget;
  /** `toId === null` means the working tree. `null` while a branch compare resolves or failed. */
  range: { fromId: string; toId: string | null } | null;
  error: DescribedError | null;
}

export interface UseCompareResult {
  /** `null` while the diff pane shows the selected row as usual. */
  compare: CompareState | null;
  compareCommits(fromId: string, toId: string): void;
  compareBranch(refName: string, label: string): void;
  compareWorkingTree(commitId: string): void;
  swap(): void;
  close(): void;
  retry(): void;
}

interface Opened {
  target: CompareTarget;
  // The selection the compare was opened under; selecting anything else closes it.
  row: SelectedRow;
}

interface Resolved {
  refName: string;
  // The `retryToken` this result was produced under; an older error is hidden while a retry runs.
  token: number;
  range: CompareRange | null;
  error: DescribedError | null;
}

export function useCompare(
  client: RepoClient,
  repoPath: string,
  refreshGeneration: number,
  selectedRow: SelectedRow,
): UseCompareResult {
  const [opened, setOpened] = useState<Opened | null>(null);
  const [resolved, setResolved] = useState<Resolved | null>(null);
  const [retryToken, setRetryToken] = useState(0);
  const selectedRowRef = useRef(selectedRow);
  useEffect(() => {
    selectedRowRef.current = selectedRow;
  }, [selectedRow]);

  // A different selection closes the compare for good (it must not come back when the user
  // returns to that row). Adjusted during render, not in an effect: `react-hooks/set-state-in-effect`
  // forbids a synchronous setState there, and this also hides it in the same render.
  if (opened !== null && !rowsEqual(opened.row, selectedRow)) {
    setOpened(null);
  }
  const active = opened !== null && rowsEqual(opened.row, selectedRow) ? opened : null;

  const branchRef = active?.target.kind === "branch" ? active.target.refName : null;
  useEffect(() => {
    if (branchRef === null) return;
    let ignore = false;
    // No synchronous reset here: a result for another ref is ignored below, and a range already
    // shown for this ref stays up while it re-resolves after a refresh.
    client.getBranchCompareRange(repoPath, branchRef).then(
      (range) => {
        if (!ignore) setResolved({ refName: branchRef, token: retryToken, range, error: null });
      },
      (err: unknown) => {
        if (!ignore) setResolved({ refName: branchRef, token: retryToken, range: null, error: describeError(err) });
      },
    );
    return () => {
      ignore = true;
    };
  }, [client, repoPath, branchRef, refreshGeneration, retryToken]);

  const open = useCallback((target: CompareTarget) => setOpened({ target, row: selectedRowRef.current }), []);
  const compareCommits = useCallback((fromId: string, toId: string) => open({ kind: "commits", fromId, toId }), [open]);
  const compareBranch = useCallback((refName: string, label: string) => open({ kind: "branch", refName, label }), [open]);
  const compareWorkingTree = useCallback((commitId: string) => open({ kind: "workingTree", commitId }), [open]);
  const swap = useCallback(
    () =>
      setOpened((prev) =>
        prev?.target.kind === "commits"
          ? { ...prev, target: { kind: "commits", fromId: prev.target.toId, toId: prev.target.fromId } }
          : prev,
      ),
    [],
  );
  const close = useCallback(() => setOpened(null), []);
  const retry = useCallback(() => setRetryToken((token) => token + 1), []);

  let compare: CompareState | null = null;
  if (active !== null) {
    const { target } = active;
    if (target.kind === "commits") {
      compare = { target, range: { fromId: target.fromId, toId: target.toId }, error: null };
    } else if (target.kind === "workingTree") {
      compare = { target, range: { fromId: target.commitId, toId: null }, error: null };
    } else {
      const current = resolved?.refName === target.refName ? resolved : null;
      const error = current !== null && current.token === retryToken ? current.error : null;
      compare = { target, range: current?.range ?? null, error };
    }
  }

  return { compare, compareCommits, compareBranch, compareWorkingTree, swap, close, retry };
}
```

- [ ] **Step 5: Run tests and lint**

Run: `cd frontend && pnpm test -- --run src/state src/components/CommitGraph && pnpm lint`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/state frontend/src/components/CommitGraph.tsx
git commit -m "feat(frontend): useCompare hook for commit, branch and working-tree compares"
```

---

### Task 8: `CompareDiffPane`

**Files:**
- Create: `frontend/src/components/CompareDiffPane.tsx`, `CompareDiffPane.module.css`, `CompareDiffPane.test.tsx`

**Interfaces:**
- Consumes: `CompareState` (Task 7), `ReadOnlyFileSection`, `CollapseAllToggle` (Task 6), the Task 5 methods.
- Produces (used by Task 10):

```ts
export const COMPARE_EXPAND_LIMIT = 50;
export function CompareDiffPane(props: {
  repoPath: string;
  client: RepoClient;
  compare: CompareState;
  commits: GraphCommit[];
  refreshGeneration: number;
  onSwap: () => void;
  onClose: () => void;
  onRetry: () => void;
  onSelectRow: (row: SelectedRow) => void;
  onOpenFileHistory?: (path: string) => void;
}): JSX.Element;
```

- [ ] **Step 1: Write the failing component tests**

`CompareDiffPane.test.tsx`:

```tsx
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { ComponentProps } from "react";
import { describe, expect, it, vi } from "vitest";
import type { ChangedFile, GraphCommit, RepoClient } from "../ipc/RepoClient";
import type { CompareState } from "../state/useCompare";
import { CompareDiffPane } from "./CompareDiffPane";

const A = "a".repeat(40);
const B = "b".repeat(40);
const commit = (id: string, shortId: string, summary: string): GraphCommit => ({
  id,
  shortId,
  summary,
  authorName: "Rene",
  authorEmail: "rene@example.com",
  timestamp: 1,
  parentIds: [],
  branchRefs: [],
  remoteBranchRefs: [],
});
const commits = [commit(B, "bbbbbbb", "newer"), commit(A, "aaaaaaa", "older")];
const twoCommits: CompareState = { target: { kind: "commits", fromId: A, toId: B }, range: { fromId: A, toId: B }, error: null };

function client(overrides: Partial<RepoClient> = {}): RepoClient {
  return {
    getRangeFiles: vi.fn(async (): Promise<ChangedFile[]> => []),
    getRangeDiff: vi.fn(async () => []),
    getWorkingTreeCompareFiles: vi.fn(async (): Promise<ChangedFile[]> => []),
    getWorkingTreeCompareDiff: vi.fn(async () => []),
    getBlame: vi.fn(async () => []),
    ...overrides,
  } as unknown as RepoClient;
}

function renderPane(props: Partial<ComponentProps<typeof CompareDiffPane>> = {}) {
  const all: ComponentProps<typeof CompareDiffPane> = {
    repoPath: "/repo",
    client: client(),
    compare: twoCommits,
    commits,
    refreshGeneration: 0,
    onSwap: vi.fn(),
    onClose: vi.fn(),
    onRetry: vi.fn(),
    onSelectRow: vi.fn(),
    ...props,
  };
  return { ...render(<CompareDiffPane {...all} />), props: all };
}

describe("CompareDiffPane", () => {
  it("names both commits and lists the range's files, renames included", async () => {
    const getRangeDiff = vi.fn(async () => []);
    renderPane({
      client: client({
        getRangeFiles: vi.fn(async () => [
          { path: "a.txt", oldPath: null },
          { path: "new.txt", oldPath: "old.txt" },
        ]),
        getRangeDiff,
      }),
    });
    const header = screen.getByRole("region", { name: "Comparison" });
    expect(within(header).getByRole("heading")).toHaveTextContent("Comparing aaaaaaa..bbbbbbb");
    expect(header).toHaveTextContent("aaaaaaa older");
    expect(header).toHaveTextContent("bbbbbbb newer");
    expect(await screen.findByText("2 files changed")).toBeInTheDocument();
    expect(screen.getByText("Renamed from old.txt")).toBeInTheDocument();
    await waitFor(() =>
      expect(getRangeDiff).toHaveBeenCalledWith("/repo", A, B, "new.txt", "old.txt", { ignoreWhitespace: false, contextLines: 3 }),
    );
  });

  it("Swap and × call back; only two-commit compares can swap", () => {
    const { props, unmount } = renderPane();
    fireEvent.click(screen.getByRole("button", { name: "Swap sides" }));
    expect(props.onSwap).toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Close compare" }));
    expect(props.onClose).toHaveBeenCalled();
    unmount();

    renderPane({ compare: { target: { kind: "workingTree", commitId: A }, range: { fromId: A, toId: null }, error: null } });
    expect(screen.queryByRole("button", { name: "Swap sides" })).toBeNull();
  });

  it("says No differences. for an empty range", async () => {
    renderPane();
    expect(await screen.findByText("No differences.")).toBeInTheDocument();
  });

  it("starts every section collapsed above the expand limit", async () => {
    const files = Array.from({ length: 51 }, (_, i) => ({ path: `f${i}.txt`, oldPath: null }));
    const getRangeDiff = vi.fn(async () => []);
    renderPane({ client: client({ getRangeFiles: vi.fn(async () => files), getRangeDiff }) });
    expect(await screen.findByText("51 files changed")).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /^Expand f\d+\.txt$/ })).toHaveLength(51);
    expect(getRangeDiff).not.toHaveBeenCalled();
  });

  it("a working-tree compare reloads on refresh and blames at HEAD", async () => {
    const getWorkingTreeCompareFiles = vi.fn(async () => [{ path: "a.txt", oldPath: null }]);
    const getBlame = vi.fn(async () => []);
    const c = client({ getWorkingTreeCompareFiles, getBlame });
    const compare: CompareState = { target: { kind: "workingTree", commitId: A }, range: { fromId: A, toId: null }, error: null };
    const { rerender, props } = renderPane({ client: c, compare });
    expect(screen.getByRole("heading")).toHaveTextContent("Comparing aaaaaaa with the working tree");
    expect(screen.getByRole("region", { name: "Comparison" })).toHaveTextContent("Working tree");
    await screen.findByText("1 file changed");

    rerender(<CompareDiffPane {...props} client={c} compare={compare} refreshGeneration={1} />);
    await waitFor(() => expect(getWorkingTreeCompareFiles).toHaveBeenCalledTimes(2));

    fireEvent.click(screen.getByRole("button", { name: "Blame" }));
    await waitFor(() => expect(getBlame).toHaveBeenCalledWith("/repo", "HEAD", "a.txt"));
  });

  it("labels a branch compare and waits for its range", () => {
    const resolving: CompareState = { target: { kind: "branch", refName: "refs/heads/feature", label: "feature" }, range: null, error: null };
    const { unmount } = renderPane({ compare: resolving });
    expect(screen.getByRole("heading")).toHaveTextContent("Changes on feature since it diverged");
    expect(screen.getByText("Finding where feature diverged…")).toBeInTheDocument();
    unmount();

    renderPane({ compare: { ...resolving, range: { fromId: A, toId: B } } });
    const header = screen.getByRole("region", { name: "Comparison" });
    expect(header).toHaveTextContent("aaaaaaa (merge base)");
    expect(header).toHaveTextContent("bbbbbbb feature");
  });

  it("a resolve error offers Retry, and Dismiss closes the compare", () => {
    const { props } = renderPane({
      compare: {
        target: { kind: "branch", refName: "refs/heads/orphan", label: "orphan" },
        range: null,
        error: { message: "orphan and HEAD have no common ancestor." },
      },
    });
    expect(screen.getByText("orphan and HEAD have no common ancestor.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(props.onRetry).toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Dismiss error" }));
    expect(props.onClose).toHaveBeenCalled();
  });

  it("a file-list error retries the file list", async () => {
    const getRangeFiles = vi
      .fn<() => Promise<ChangedFile[]>>()
      .mockRejectedValueOnce("failed to compute diff: boom")
      .mockResolvedValueOnce([{ path: "a.txt", oldPath: null }]);
    renderPane({ client: client({ getRangeFiles }) });
    fireEvent.click(await screen.findByRole("button", { name: "Retry" }));
    expect(await screen.findByText("1 file changed")).toBeInTheDocument();
  });
});
```

`InlineError` renders `Retry` and an icon button labelled `Dismiss error`.

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/components/CompareDiffPane`
Expected: FAIL (module not found).

- [ ] **Step 3: Implement**

`CompareDiffPane.tsx`:

```tsx
import { useEffect, useRef, useState } from "react";
import type { ChangedFile, DiffOptions, GraphCommit, RepoClient } from "../ipc/RepoClient";
import { describeError, type DescribedError } from "../lib/errorMessages";
import type { SelectedRow } from "../state/useAppState";
import type { CompareState } from "../state/useCompare";
import { CollapseAllToggle } from "./CollapseToggle";
import styles from "./CompareDiffPane.module.css";
import diffStyles from "./DiffPane.module.css";
import { InlineError } from "./primitives/InlineError";
import { ReadOnlyFileSection } from "./ReadOnlyFileSection";

/** Above this many files every section starts collapsed: each open section fetches its own diff. */
export const COMPARE_EXPAND_LIMIT = 50;

export function CompareDiffPane({
  repoPath,
  client,
  compare,
  commits,
  refreshGeneration,
  onSwap,
  onClose,
  onRetry,
  onSelectRow,
  onOpenFileHistory,
}: {
  repoPath: string;
  client: RepoClient;
  compare: CompareState;
  commits: GraphCommit[];
  refreshGeneration: number;
  onSwap: () => void;
  onClose: () => void;
  onRetry: () => void;
  onSelectRow: (row: SelectedRow) => void;
  onOpenFileHistory?: (path: string) => void;
}) {
  const { target, range } = compare;
  const fromId = range?.fromId ?? null;
  const toId = range?.toId ?? null;
  // Commits never change; the working tree does.
  const reloadKey = target.kind === "workingTree" ? refreshGeneration : 0;
  const rangeKey = fromId === null ? null : `${fromId}..${toId ?? "worktree"}`;

  const [files, setFiles] = useState<ChangedFile[] | null>(null);
  const [error, setError] = useState<DescribedError | null>(null);
  const [collapsedPaths, setCollapsedPaths] = useState<Set<string>>(new Set());
  const collapseInitializedFor = useRef<string | null>(null);

  const loadFiles = () => {
    if (fromId === null) return;
    let ignore = false;
    const request = toId === null ? client.getWorkingTreeCompareFiles(repoPath, fromId) : client.getRangeFiles(repoPath, fromId, toId);
    request.then(
      (next) => {
        if (ignore) return;
        setFiles(next);
        setError(null);
        if (collapseInitializedFor.current !== rangeKey) {
          collapseInitializedFor.current = rangeKey;
          setCollapsedPaths(next.length > COMPARE_EXPAND_LIMIT ? new Set(next.map((f) => f.path)) : new Set());
        }
      },
      (err: unknown) => {
        if (!ignore) setError(describeError(err));
      },
    );
    return () => {
      ignore = true;
    };
  };

  useEffect(() => {
    return loadFiles();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [repoPath, client, fromId, toId, reloadKey]);

  const short = (id: string) => commits.find((c) => c.id === id)?.shortId ?? id.slice(0, 7);
  const described = (id: string) => {
    const found = commits.find((c) => c.id === id);
    return found === undefined ? id.slice(0, 7) : `${found.shortId} ${found.summary}`;
  };

  let heading: string;
  if (target.kind === "commits") heading = `Comparing ${short(target.fromId)}..${short(target.toId)}`;
  else if (target.kind === "branch") heading = `Changes on ${target.label} since it diverged`;
  else heading = `Comparing ${short(target.commitId)} with the working tree`;

  const toggleCollapse = (path: string) =>
    setCollapsedPaths((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  const allCollapsed = files !== null && files.length > 0 && files.every((f) => collapsedPaths.has(f.path));

  const fetchFor = (file: ChangedFile) => (opts: DiffOptions) =>
    toId === null
      ? client.getWorkingTreeCompareDiff(repoPath, fromId as string, file.path, file.oldPath, opts)
      : client.getRangeDiff(repoPath, fromId as string, toId, file.path, file.oldPath, opts);

  let body;
  if (compare.error !== null) {
    body = <InlineError message={compare.error.message} hint={compare.error.hint} onRetry={onRetry} onDismiss={onClose} />;
  } else if (fromId === null) {
    body = <p className={styles.status}>{`Finding where ${target.kind === "branch" ? target.label : "it"} diverged…`}</p>;
  } else if (error !== null) {
    body = <InlineError message={error.message} hint={error.hint} onRetry={loadFiles} onDismiss={onClose} />;
  } else if (files === null) {
    body = null;
  } else if (files.length === 0) {
    body = <p className={styles.status}>No differences.</p>;
  } else {
    body = (
      <>
        <div className={diffStyles.groupHeading}>
          <span className={styles.status}>{files.length === 1 ? "1 file changed" : `${files.length} files changed`}</span>
          <CollapseAllToggle
            allCollapsed={allCollapsed}
            onToggle={() => setCollapsedPaths(allCollapsed ? new Set() : new Set(files.map((f) => f.path)))}
          />
        </div>
        <ul className={diffStyles.fileList}>
          {files.map((file) => (
            <ReadOnlyFileSection
              key={`${file.oldPath ?? ""}→${file.path}`}
              repoPath={repoPath}
              client={client}
              path={file.path}
              oldPath={file.oldPath}
              diffKey={`${rangeKey}:${reloadKey}`}
              fetchDiff={fetchFor(file)}
              blameAt={toId ?? "HEAD"}
              collapsed={collapsedPaths.has(file.path)}
              onToggleCollapse={() => toggleCollapse(file.path)}
              onSelectRow={onSelectRow}
              onOpenFileHistory={onOpenFileHistory}
            />
          ))}
        </ul>
      </>
    );
  }

  return (
    <div>
      <section className={styles.header} aria-label="Comparison">
        <div className={styles.titleRow}>
          <h2 className={styles.title}>{heading}</h2>
          {target.kind === "commits" && (
            <button type="button" aria-label="Swap sides" onClick={onSwap}>
              Swap
            </button>
          )}
          <button type="button" aria-label="Close compare" onClick={onClose}>
            ×
          </button>
        </div>
        {fromId !== null && (
          <dl className={styles.ends}>
            <dt>From</dt>
            <dd>{target.kind === "branch" ? `${short(fromId)} (merge base)` : described(fromId)}</dd>
            <dt>To</dt>
            <dd>
              {toId === null ? "Working tree" : target.kind === "branch" ? `${short(toId)} ${target.label}` : described(toId)}
            </dd>
          </dl>
        )}
      </section>
      {body}
    </div>
  );
}
```

The status line `<span>` sits inside the group heading on purpose. `findByText("2 files changed")` matches it, and `body` renders only one status at a time.

`CompareDiffPane.module.css`, using the same tokens as `CommitHeader.module.css`:

```css
.header {
  padding: 8px 12px;
  border-bottom: 1px solid var(--color-border);
}

.titleRow {
  display: flex;
  align-items: center;
  gap: 8px;
}

.title {
  flex: 1;
  margin: 0;
  font-size: 13px;
  font-weight: 600;
}

.ends {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 2px 8px;
  margin: 6px 0 0;
}

.ends dt {
  color: var(--color-text-muted);
}

.ends dd {
  margin: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.status {
  color: var(--color-text-muted);
}
```

These are the tokens `CommitHeader.module.css` uses.

- [ ] **Step 4: Run tests and lint**

Run: `cd frontend && pnpm test -- --run src/components/CompareDiffPane && pnpm lint`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/CompareDiffPane.*
git commit -m "feat(frontend): CompareDiffPane for commit, branch and working-tree compares"
```

---

### Task 9: Entry points in `CommitGraph` and `BranchTree`

**Files:**
- Modify: `frontend/src/components/CommitGraph.tsx` (+ `CommitGraph.test.tsx`), `frontend/src/components/BranchTree.tsx` (+ `BranchTree.test.tsx`)

**Interfaces:**
- Produces (used by Task 10):
  - `CommitGraph` props: `onCompareCommits?: (fromId: string, toId: string) => void;` and `onCompareWithWorkingTree?: (commitId: string) => void;`
  - `BranchTree` prop: `onCompareBranch?: (refName: string, label: string) => void;`

- [ ] **Step 1: Write the failing `CommitGraph` tests**

In the describe that has `renderChain` (chain `D → C → B → A`, newest first):

```tsx
  it("offers Compare these commits only with exactly two selected", () => {
    const onCompareCommits = vi.fn();
    renderChain({ onCompareCommits });

    fireEvent.click(row("D"));
    fireEvent.click(row("B"), { ctrlKey: true });
    expect(screen.getByRole("button", { name: "Compare these commits" })).toBeEnabled();

    fireEvent.click(row("C"), { ctrlKey: true });
    expect(screen.queryByRole("button", { name: "Compare these commits" })).toBeNull();
  });

  it("orders the two commits older..newer whatever the click order", () => {
    const onCompareCommits = vi.fn();
    renderChain({ onCompareCommits });

    fireEvent.click(row("B"));
    fireEvent.click(row("D"), { ctrlKey: true });
    fireEvent.click(screen.getByRole("button", { name: "Compare these commits" }));
    expect(onCompareCommits).toHaveBeenLastCalledWith("B", "D");

    fireEvent.contextMenu(row("D"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Compare these commits" }));
    expect(onCompareCommits).toHaveBeenLastCalledWith("B", "D");
  });

  it("keeps Compare these commits enabled while an operation is pending", () => {
    renderChain({ pending: true, onCompareCommits: vi.fn() });
    fireEvent.click(row("C"));
    fireEvent.click(row("B"), { shiftKey: true });
    expect(screen.getByRole("button", { name: "Compare these commits" })).toBeEnabled();
  });

  it("offers Diff against working tree on a single commit", () => {
    const onCompareWithWorkingTree = vi.fn();
    renderChain({ pending: true, onCompareWithWorkingTree });
    fireEvent.contextMenu(row("C"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Diff against working tree" }));
    expect(onCompareWithWorkingTree).toHaveBeenCalledWith("C");
  });
```

- [ ] **Step 2: Write the failing `BranchTree` tests**

In `describe("BranchTree — local branches", …)`:

```tsx
  it("Compare with current branch is offered on other local branches only", () => {
    const onCompareBranch = vi.fn();
    renderTree({ onCompareBranch });
    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Compare with current branch" }));
    expect(onCompareBranch).toHaveBeenCalledWith("refs/heads/feat/foo", "feat/foo");

    fireEvent.contextMenu(screen.getByRole("button", { name: "main" }));
    expect(screen.queryByRole("menuitem", { name: "Compare with current branch" })).toBeNull();
  });

  it("says Compare with HEAD when no branch is checked out", () => {
    renderTree({
      branches: [
        { name: "main", isCurrent: false },
        { name: "feat/foo", isCurrent: false },
      ],
      onCompareBranch: vi.fn(),
    });
    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));
    expect(screen.getByRole("menuitem", { name: "Compare with HEAD" })).toBeInTheDocument();
  });
```

In the remote describe (the one that defines `oneRemote`):

```tsx
  it("Compare with current branch on a remote-tracking branch passes its full ref", async () => {
    const onCompareBranch = vi.fn();
    renderTree({ remotes: oneRemote, onListRemoteBranches: vi.fn().mockResolvedValue(["main"]), onCompareBranch });
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    fireEvent.contextMenu(await screen.findByText("main"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Compare with current branch" }));
    expect(onCompareBranch).toHaveBeenCalledWith("refs/remotes/origin/main", "origin/main");
  });
```

If the local "main" row and the remote "main" row are both on screen, scope `findByText("main")` to the remote section, the same way the neighbouring "Set as upstream for current branch" test does.

- [ ] **Step 3: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/components/CommitGraph src/components/BranchTree`
Expected: FAIL (items missing).

- [ ] **Step 4: Implement `CommitGraph`**

- Props (after `onRequestSearch`), with a comment: read-only actions, so never disabled by `pending`:

```tsx
  /** Exactly two commits selected: compare `fromId` (further down the graph) with `toId`. */
  onCompareCommits?: (fromId: string, toId: string) => void;
  /** Single-commit menu: compare that commit with the working tree. */
  onCompareWithWorkingTree?: (commitId: string) => void;
```

- `singleCommitItems`: before `return items;`:

```tsx
    if (onCompareWithWorkingTree !== undefined) {
      items.push({ label: "Diff against working tree", onSelect: () => onCompareWithWorkingTree(commitId) });
    }
```

- `multiSelectItems`: rename the existing IIFE to `rebaseItems`, then:

```tsx
  // Read-only, so neither `pending` nor a blocked rebase plan disables it.
  const compareItem: ContextMenuItem | null = (() => {
    if (onCompareCommits === undefined || effectiveSelection.ids.size !== 2) return null;
    const [first, second] = [...effectiveSelection.ids];
    // Graph order is newest first, so the higher index is the older commit.
    const firstIndex = commits.findIndex((c) => c.id === first);
    const secondIndex = commits.findIndex((c) => c.id === second);
    const [fromId, toId] = firstIndex > secondIndex ? [first, second] : [second, first];
    return { label: "Compare these commits", onSelect: () => onCompareCommits(fromId, toId) };
  })();
  const multiSelectItems: ContextMenuItem[] = compareItem === null ? rebaseItems : [...rebaseItems, compareItem];
```

- [ ] **Step 5: Implement `BranchTree`**

- Prop: `onCompareBranch?: (refName: string, label: string) => void;` (optional; add it to the destructuring).
- Next to `currentBranchName` (declared after the menu builders, which is fine: they only run from the JSX at the end of the component): `const compareLabel = currentBranchName !== undefined ? "Compare with current branch" : "Compare with HEAD";`.
- `branchContextItems`: inside `if (!branch.isCurrent) { … }`, after the "Merge into current branch" push (and after anything 4b added there):

```tsx
      if (onCompareBranch !== undefined) {
        items.push({ label: compareLabel, onSelect: () => onCompareBranch(`refs/heads/${branch.name}`, branch.name) });
      }
```

- `remoteBranchItems`: return the existing array with, when `onCompareBranch !== undefined`, a final item `{ label: compareLabel, onSelect: () => onCompareBranch(`refs/remotes/${remoteName}/${branchName}`, `${remoteName}/${branchName}`) }`.

- [ ] **Step 6: Run tests and lint**

Run: `cd frontend && pnpm test -- --run src/components && pnpm lint`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/components/CommitGraph.* frontend/src/components/BranchTree.*
git commit -m "feat(frontend): compare entry points in the commit graph and branch menus"
```

---

### Task 10: Wire compare into the app

**Files:**
- Modify: `frontend/src/App.tsx`, `frontend/src/App.test.tsx`

**Interfaces:**
- Consumes: `useCompare` (Task 7), `CompareDiffPane` (Task 8), the Task 9 props.

- [ ] **Step 1: Write the failing App test**

After "searches from the graph with Ctrl+F…":

```tsx
  it("compares two selected commits in place of the diff pane, and × restores the commit view", async () => {
    const base = { authorName: "Rene", authorEmail: "rene@example.com", timestamp: 1_700_000_000, branchRefs: [], remoteBranchRefs: [] };
    const newer = { ...base, id: "n".repeat(40), shortId: "nnnnnnn", summary: "newer commit", parentIds: ["o".repeat(40)] };
    const older = { ...base, id: "o".repeat(40), shortId: "ooooooo", summary: "older commit", parentIds: [] };
    const getRangeFiles = vi.fn(async () => [{ path: "a.txt", oldPath: null }]);
    const client = fakeClient({
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [newer, older],
      listBranches: async () => [],
      listStashes: async () => [],
      getGraphBranchSelection: async () => null,
      getCommitFiles: async () => [],
      getCommitMessage: async () => "older commit",
      getCommitSignature: async () => ({ status: "unsigned", format: null, signer: null, key: null, detail: null }),
      getRangeFiles,
      getRangeDiff: async () => [],
    });

    render(<App client={client} />);
    const graph = await screen.findByRole("listbox", { name: "Commit history" });
    fireEvent.click(within(graph).getByText(/newer commit/));
    fireEvent.click(within(graph).getByText(/older commit/), { ctrlKey: true });
    fireEvent.click(screen.getByRole("button", { name: "Compare these commits" }));

    const comparison = await screen.findByRole("region", { name: "Comparison" });
    expect(within(comparison).getByRole("heading")).toHaveTextContent("Comparing ooooooo..nnnnnnn");
    await waitFor(() => expect(getRangeFiles).toHaveBeenCalledWith("/repos/a", "o".repeat(40), "n".repeat(40)));

    fireEvent.click(screen.getByRole("button", { name: "Close compare" }));
    expect(screen.queryByRole("region", { name: "Comparison" })).toBeNull();
  });
```

Add whatever other client methods the first render needs, using the search test's list as the baseline. If it throws "not used in this test", add that method with an inert value.

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/App.test.tsx`
Expected: FAIL (no "Compare these commits" button, because `App` does not pass `onCompareCommits`).

- [ ] **Step 3: Implement**

In `RepoWorkspace` (next to `useCommitSearch`):

```tsx
  const compare = useCompare(client, repoPath, appState.state.refreshGeneration, appState.state.selectedRow);
```

- `<CommitGraph … onCompareCommits={compare.compareCommits} onCompareWithWorkingTree={compare.compareWorkingTree} />`
- `<BranchTree … onCompareBranch={compare.compareBranch} />`
- `right=`:

```tsx
            right={
              compare.compare !== null ? (
                <CompareDiffPane
                  repoPath={repoPath}
                  client={client}
                  compare={compare.compare}
                  commits={diffPaneCommits}
                  refreshGeneration={appState.state.refreshGeneration}
                  onSwap={compare.swap}
                  onClose={compare.close}
                  onRetry={compare.retry}
                  onSelectRow={appState.selectRow}
                  onOpenFileHistory={fileHistory.open}
                />
              ) : (
                <DiffPane …unchanged… />
              )
            }
```

If Task 0 found a 4c stash view that also replaces the diff pane: make each opener close the other. Call `compare.close()` where the stash view opens, and close the stash view inside a small wrapper around `compare.compareCommits`/`compareBranch`/`compareWorkingTree` that you pass to the entry points. Add one App test: opening a compare while a stash is shown replaces it.

- [ ] **Step 4: Run tests and lint**

Run: `cd frontend && pnpm test -- --run && pnpm lint && pnpm build`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/App.tsx frontend/src/App.test.tsx
git commit -m "feat(frontend): open compares from the graph and branch menus"
```

---

### Task 11: Tauri E2E, user guide and changelog

**Files:**
- Create: `e2e/specs/compare.spec.ts`
- Modify: `docs/USER_GUIDE.md` (new section after "## Searching commits"), `CHANGELOG.md` (under `## [Unreleased]`, after the last batch-4 subsection)

- [ ] **Step 1: Write the E2E spec**

The spec does not depend on other specs' state. It makes its own commits on a unique file, refreshes first, and reads the short ids from git:

```ts
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const FILE = "compare-e2e.txt";

function git(...args: string[]): string {
  return execFileSync("git", args, { cwd: E2E_REPO_PATH, encoding: "utf8" }).trim();
}

function commit(contents: string, message: string): string {
  fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), contents);
  git("add", FILE);
  git("commit", "-m", message);
  return git("rev-parse", "HEAD").slice(0, 7);
}

describe("Browsitory compare", () => {
  let base = "";
  let tip = "";

  before(() => {
    base = commit("one\n", "e2e: compare base");
    commit("one\ntwo\n", "e2e: compare middle");
    tip = commit("one\ntwo\nthree\n", "e2e: compare tip");
  });

  it("compares two commits, swaps them, and closes back to the commit view", async () => {
    // Commits made with plain git after the app loaded; reload so the graph has them.
    await browser.refresh();

    const baseRow = await $("li*=e2e: compare base");
    await baseRow.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), baseRow);
    // Synthetic ctrl-click, as in rebase.spec.ts: a real held modifier sticks in WebKitGTK.
    const tipRow = await $("li*=e2e: compare tip");
    await browser.execute((el) => {
      el.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, ctrlKey: true }));
    }, tipRow);

    const compareButton = await $("button=Compare these commits");
    await compareButton.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), compareButton);

    const heading = await $(`section[aria-label="Comparison"] h2`);
    await heading.waitForExist({ timeout: 10000 });
    await expect(heading).toHaveText(`Comparing ${base}..${tip}`);
    await expect($(`section[aria-label="Comparison"]`)).toHaveText(expect.stringContaining("e2e: compare tip"));
    await (await $(`span=${FILE}`)).waitForExist({ timeout: 10000 });

    await browser.execute((el) => (el as HTMLElement).click(), await $(`button[aria-label="Swap sides"]`));
    await expect(heading).toHaveText(`Comparing ${tip}..${base}`);

    await browser.execute((el) => (el as HTMLElement).click(), await $(`button[aria-label="Close compare"]`));
    await browser.waitUntil(async () => !(await $(`section[aria-label="Comparison"]`).isExisting()), { timeout: 10000 });
  });
});
```

`span=${FILE}` relies on the file header rendering the path in a `<span>` (`styles.path` in `ReadOnlyFileSection`). If the selector misses, use `//li[contains(., '${FILE}')]` instead.

- [ ] **Step 2: Run the E2E**

Run (from the repo root, per CLAUDE.md "E2E: Tauri app"):

```bash
cd frontend && VITE_E2E_REPO_PATH=/tmp/browsitory-e2e-repo pnpm build && cd ..
cargo build --workspace --features tauri-app/custom-protocol,tauri-app/forge-fixture-override
cd e2e && pnpm install && pnpm test -- --spec ./specs/compare.spec.ts && cd ..
```

Expected: PASS. If there is no display, use `xvfb-run`.

- [ ] **Step 3: USER_GUIDE**

After "## Searching commits":

```markdown
## Comparing commits and branches

Compares are read-only and replace the diff pane until you click **×** or select another row.

- **Two commits:** select one commit, then Ctrl-click (Cmd-click on macOS) or Shift-click a
  second, and choose **Compare these commits** in the bar under the graph or in the right-click
  menu. The older commit (lower in the graph) is on the left: the header reads
  `Comparing older..newer`. **Swap** reverses it. The commits do not need to be related; this
  is `git diff A B`.
- **A branch since it diverged:** right-click a local branch other than the current one, or a
  remote-tracking branch, and choose **Compare with current branch** (**Compare with HEAD**
  when no branch is checked out). It shows what the branch changed since its merge base with
  HEAD (`git diff HEAD...branch`), so commits only on your branch do not appear.
- **A commit against the working tree:** right-click a commit and choose **Diff against working
  tree**. Staged, unstaged and untracked changes are all included, and the view follows edits on
  disk.

Renamed files are listed once with "Renamed from …", and their diff shows only the edits. Each
file has **Blame** (at the newer side, or at HEAD for the working tree) and **History**. With
more than 50 files, every file starts collapsed; use **Expand all**.
```

- [ ] **Step 4: CHANGELOG**

```markdown
### Compare

- **Compare these commits** (two selected commits, from the selection bar or the right-click
  menu) shows every file that differs between them in the diff pane, oldest first, with
  **Swap**. Renames are followed.
- **Compare with current branch** on a branch (local or remote-tracking) shows what it changed
  since it diverged; **Diff against working tree** on a commit compares it with your files on
  disk, untracked files included.
- Compare views are read-only, with Blame and History on each file.
```

- [ ] **Step 5: Full verification and commit**

Run:
```bash
cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd frontend && pnpm lint && pnpm test -- --run && pnpm build && cd ..
cd extension && pnpm test -- --run && pnpm run compile && cd ..
```
Expected: all PASS.

```bash
git add e2e/specs/compare.spec.ts docs/USER_GUIDE.md CHANGELOG.md
git commit -m "test(e2e): compare two commits; docs: compare in user guide and changelog"
```

---

## Self-review notes

- Spec coverage: backend (Tasks 1–2), worker (3), Tauri + sidecar (4), clients (5), read-only section and file headers with Blame and History (6), state (7), pane, header, swap, counts, collapse limit and working-tree reload (8), toolbar, menu, branch and working-tree entry points (9), wiring (10), E2E and docs (11). No undo, palette or `LICENSE_COMPLIANCE` work, by design.
- Names used across tasks: `ChangedFile`/`CompareRange` (Rust and TS), `range_diff_files`/`range_diff`/`compare_working_tree_files`/`compare_working_tree`/`branch_compare_range`, `get_range_files`/`get_range_diff`/`get_working_tree_compare_files`/`get_working_tree_compare_diff`/`get_branch_compare_range`, `getRangeFiles`/`getRangeDiff`/`getWorkingTreeCompareFiles`/`getWorkingTreeCompareDiff`/`getBranchCompareRange`, `useCompare` → `compare.compare`, `CompareDiffPane`, `ReadOnlyFileSection`, `COMPARE_EXPAND_LIMIT`.

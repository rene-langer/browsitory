# Task 1.A.02: `git-core::diff`

## Goal

Add `git_core::diff` with three functions: `working_diff` (a path's diff between HEAD/index or
index/workdir), `commit_diff` (a path's diff between a commit and its first parent), and
`commit_files` (the list of paths changed by a commit — used to build the commit's file list in
the UI before diffing any one of them). Line-level diff only this phase — added/removed/context
lines, no word-level highlighting within a line.

## Depends on

None — new module, only touches `crates/git-core`.

## Interfaces produced

`crates/git-core/src/diff.rs`:
```rust
use git2::Repository;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DiffError {
    #[error("failed to compute diff: {0}")]
    Diff(#[from] git2::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineOrigin {
    Add,
    Remove,
    Context,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub origin: DiffLineOrigin,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffHunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<DiffLine>,
}

/// Diff for one path in the working tree. `staged: true` diffs HEAD's tree against the index
/// (what `git diff --cached -- path` shows); `staged: false` diffs the index against the
/// working directory (what `git diff -- path` shows).
pub fn working_diff(repo: &Repository, path: &str, staged: bool) -> Result<Vec<DiffHunk>, DiffError> {
    // ...
}

/// Diff for one path between `commit_id` and its first parent (or an empty tree, for a
/// commit with no parent). Merge commits are diffed against their first parent only — no
/// combined/conflict view; fine, since Phase 1 has no merge UI.
pub fn commit_diff(repo: &Repository, commit_id: &str, path: &str) -> Result<Vec<DiffHunk>, DiffError> {
    // ...
}

/// Paths changed by `commit_id` (vs. its first parent, same rule as `commit_diff`) — used to
/// build a commit's file list before diffing any individual file.
pub fn commit_files(repo: &Repository, commit_id: &str) -> Result<Vec<String>, DiffError> {
    // ...
}
```

`crates/git-core/src/lib.rs` gains `pub mod diff;`.

## Implementation notes

**Getting hunks/lines out of a `git2::Diff`** — this is the one genuinely non-obvious part.
`Diff::foreach` takes up to four callbacks (file/binary/hunk/line) that all need to be alive
simultaneously, so a plain `&mut Vec<DiffHunk>` captured by two separate closures won't borrow-
check. Use a `RefCell` captured by shared reference in both closures instead:

```rust
use std::cell::RefCell;

fn hunks_from_diff(diff: &git2::Diff) -> Result<Vec<DiffHunk>, DiffError> {
    let hunks: RefCell<Vec<DiffHunk>> = RefCell::new(Vec::new());

    diff.foreach(
        &mut |_delta, _progress| true,
        None,
        Some(&mut |_delta, hunk| {
            hunks.borrow_mut().push(DiffHunk {
                old_start: hunk.old_start(),
                old_lines: hunk.old_lines(),
                new_start: hunk.new_start(),
                new_lines: hunk.new_lines(),
                lines: Vec::new(),
            });
            true
        }),
        Some(&mut |_delta, _hunk, line| {
            let origin = match line.origin() {
                '+' => DiffLineOrigin::Add,
                '-' => DiffLineOrigin::Remove,
                _ => DiffLineOrigin::Context,
            };
            let content = String::from_utf8_lossy(line.content()).into_owned();
            if let Some(last) = hunks.borrow_mut().last_mut() {
                last.lines.push(DiffLine { origin, content });
            }
            true
        }),
    )?;

    Ok(hunks.into_inner())
}
```
Both `git2::DiffLine::origin()` values other than `'+'`/`'-'` (context `' '`, and a few rare
markers like "no newline at end of file") fold into `Context` — fine for line-level diff, no
special handling needed this phase.

**`working_diff`:**
```rust
pub fn working_diff(repo: &Repository, path: &str, staged: bool) -> Result<Vec<DiffHunk>, DiffError> {
    let mut opts = git2::DiffOptions::new();
    opts.pathspec(path);

    let diff = if staged {
        let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
        repo.diff_tree_to_index(head_tree.as_ref(), None, Some(&mut opts))?
    } else {
        repo.diff_index_to_workdir(None, Some(&mut opts))?
    };

    hunks_from_diff(&diff)
}
```
`head_tree` is `None` on a repo with no commits yet (unborn HEAD) — `diff_tree_to_index(None, ...)`
correctly treats that as an empty tree, so a staged new file's diff shows every line as an
addition, matching real `git diff --cached` behavior.

**`commit_diff` and `commit_files`** share the same tree-pair lookup:
```rust
fn commit_and_parent_trees<'repo>(
    repo: &'repo Repository,
    commit_id: &str,
) -> Result<(Option<git2::Tree<'repo>>, git2::Tree<'repo>), DiffError> {
    let oid = git2::Oid::from_str(commit_id)?;
    let commit = repo.find_commit(oid)?;
    let new_tree = commit.tree()?;
    let old_tree = commit.parent(0).ok().and_then(|p| p.tree().ok());
    Ok((old_tree, new_tree))
}

pub fn commit_diff(repo: &Repository, commit_id: &str, path: &str) -> Result<Vec<DiffHunk>, DiffError> {
    let (old_tree, new_tree) = commit_and_parent_trees(repo, commit_id)?;
    let mut opts = git2::DiffOptions::new();
    opts.pathspec(path);
    let diff = repo.diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), Some(&mut opts))?;
    hunks_from_diff(&diff)
}

pub fn commit_files(repo: &Repository, commit_id: &str) -> Result<Vec<String>, DiffError> {
    let (old_tree, new_tree) = commit_and_parent_trees(repo, commit_id)?;
    let diff = repo.diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), None)?;

    let mut paths = Vec::new();
    for delta in diff.deltas() {
        let path = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .map(|p| p.to_string_lossy().into_owned());
        if let Some(path) = path {
            paths.push(path);
        }
    }
    Ok(paths)
}
```
`commit_and_parent_trees` is a private helper — do not export it, both public functions call it.

## TDD requirement

`crates/git-core/tests/diff.rs` (new file, `mod common;` + existing helpers):

- `working_diff_unstaged_shows_added_and_context_lines`: `commit_all` a file with 3 lines, then
  `write_file` it again changing the middle line. Call
  `working_diff(&repo, "tracked.txt", false)`, assert exactly 1 hunk, and that its `lines`
  contain at least one `DiffLineOrigin::Remove` (old middle line) and one
  `DiffLineOrigin::Add` (new middle line).
- `working_diff_staged_shows_the_staged_content`: `write_file` a new file, stage it
  (`index.add_path` + `index.write()`, same pattern as `tests/status.rs`), call
  `working_diff(&repo, "new.txt", true)`, assert every line's `origin` is
  `DiffLineOrigin::Add` and the concatenated `content` matches what was written.
  Then call `working_diff(&repo, "new.txt", false)` (unstaged) and assert it returns an empty
  `Vec` (nothing left in the workdir-vs-index diff once the only change is staged).
- `commit_diff_shows_the_change_introduced_by_that_commit`: two commits via `commit_all`
  (second one modifies a line from the first). Get the second commit's id via
  `repo.head().unwrap().peel_to_commit().unwrap().id().to_string()`, call
  `commit_diff(&repo, &second_id, "tracked.txt")`, assert it shows the same add/remove shape
  as the unstaged-diff test above.
- `commit_diff_on_the_first_commit_shows_every_line_as_added`: one commit via `commit_all`
  adding a 2-line file. Diff that commit (no parent) against `"tracked.txt"`, assert every line
  is `DiffLineOrigin::Add`.
- `commit_files_lists_every_changed_path`: one commit via `commit_all` that adds two files
  (`write_file` both before the single `commit_all` call). Assert
  `commit_files(&repo, &commit_id)` returns both paths (order-independent —
  `assert_eq!(result.iter().collect::<std::collections::HashSet<_>>(), expected_set)` or sort
  both sides before comparing).

Write these five tests first, run `cargo test -p git-core --test diff`, confirm they fail to
compile, then implement `diff.rs` and re-run until green.

## Acceptance criteria

- [ ] `cargo test -p git-core --test diff` passes (all 5 tests).
- [ ] `cargo test --workspace` still passes.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean.
- [ ] `cargo fmt --all -- --check` clean.
- [ ] Commit: `git add crates/git-core/src/lib.rs crates/git-core/src/diff.rs crates/git-core/tests/diff.rs && git commit -m "feat(git-core): add working_diff, commit_diff, commit_files"`.

## Out of scope

Word-level diff highlighting within a line. Binary file diffs (a binary path's `working_diff`/
`commit_diff` call will error or return an empty hunk list via libgit2's binary detection —
acceptable for this phase, not specially handled). Combined/conflict diffs for merge commits.

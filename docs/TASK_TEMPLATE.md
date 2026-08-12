# Task template

Every implementation task in Browsitory (Phase 1 onward) follows this shape, one file per
task, named `docs/tasks/phase-<N>/<workstream>-<id>-<slug>.md` (e.g.
`docs/tasks/phase-1/a-01-diff-viewer.md`). Tasks within the same phase that touch disjoint
files are grouped into parallel workstreams (A, B, C, ...) per
`superpowers:subagent-driven-development`; tasks within a workstream are sequential.

```markdown
# Task <phase>.<workstream>.<id>: <title>

## Goal
One paragraph: what this task adds and why.

## Depends on
List of task IDs that must land first, or "none".

## TDD requirement
Which test file(s) get written first, and what they must assert before any implementation
code is written. Real assertions, not "add appropriate tests."

## Acceptance criteria
Checklist of observable outcomes (tests passing, a command working end-to-end, etc).

## Out of scope
What this task deliberately does not do — keeps tasks isolated and reviewable independently.
```

## Example

```markdown
# Task 1.A.02: Diff viewer for a single file

## Goal
Add `git_core::diff::file_diff(repo, path) -> Result<Vec<DiffHunk>, DiffError>` and a
`DiffView` frontend component that renders it, so a user can see line-level changes for a
selected file in the status list.

## Depends on
Task 1.A.01 (git-core::status, already landed).

## TDD requirement
`crates/git-core/tests/diff.rs`: a test that modifies a tracked file and asserts
`file_diff` returns one hunk containing both the removed and added line. A test that diffs an
untracked file against an empty tree and asserts every line is an addition.
`frontend/src/components/DiffView.test.tsx`: a test that renders a `DiffView` given a fixed
list of `DiffHunk`s (via a mocked `RepoClient`) and asserts added/removed lines get distinct
CSS classes.

## Acceptance criteria
- [ ] `cargo test -p git-core --test diff` passes.
- [ ] `pnpm test -- --run` passes for `DiffView.test.tsx`.
- [ ] Selecting a file in `StatusView` shows its diff in `cargo tauri dev`.

## Out of scope
Word-level diff highlighting (later task). Binary file diffs (later task).
```

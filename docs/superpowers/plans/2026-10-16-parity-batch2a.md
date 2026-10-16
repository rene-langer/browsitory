# Feature Parity Batch 2a Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Line-level stage/unstage/discard, ignore-whitespace and context-lines diff options, syntax highlighting in diffs, and file-level discard / ignore, end to end (git-core → repo-service → Tauri/sidecar → `RepoClient` → UI).

**Architecture:** Line staging synthesizes a one-hunk patch in Rust from the recomputed hunk and the selected line indices, parses it with `git2::Diff::from_buffer`, and applies it with `repo.apply` (same apply path as hunk staging). Diff options live in a new `git_core::diff::DiffOptions`, shared by display and staging through one options builder so hunk identity can't drift. Global diff settings persist in `config.toml` and reach the UI through a React context. Highlighting is frontend-only: Shiki core + JavaScript regex engine + a CSS-variables theme.

**Tech Stack:** Rust (`git2` 0.21, `thiserror`, `tempfile`, `toml`/`serde`), Tauri 2, React 19 + TypeScript, Vitest + Testing Library, Shiki 4.5.0, WebdriverIO (Tauri E2E), `@vscode/test-electron`.

**Spec:** `docs/superpowers/specs/2026-10-16-parity-batch2a-design.md`

## Global Constraints

- Branch: `feat/parity-batch2a` (already created; spec committed). Commit after every task. Commit messages end with:
  ```
  Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_01TzyRBFjAeEVmDDiMCaBL11
  ```
- `git-core`/`repo-service` tests use real temp-dir repos (`tests/common/mod.rs`: `init_repo`, `commit_all`, `write_file`), never a mocked `Repository`.
- Frontend tests mock `RepoClient` only — never `@tauri-apps/api` or `postMessage`. Transport-isolation ESLint rule stays green (`pnpm lint`).
- Only new dependency: `shiki` **exactly** `4.5.0` (MIT), frontend only. Row in `docs/LICENSE_COMPLIANCE.md` in the same commit. No new Rust dependencies.
- Backend error text is user-facing. Exact strings:
  - `EmptySelection`: "Select at least one added or removed line."
  - `ContextLineSelected`: "Only added or removed lines can be staged."
  - `InvalidPath`: "That path is outside the repository."
  - `InvalidPattern`: "Ignore pattern can't be empty or span several lines."
  - `SelectionNotApplicable`: "Couldn't apply just those lines. Try the whole hunk instead." (plan addition — see Review Focus 3)
- UI copy (verbatim):
  - Hint: "Turn off Ignore whitespace to stage partially."
  - Discard tracked: "Discard changes to ⟨path⟩? This can't be undone."
  - Discard untracked: "Delete untracked file ⟨path⟩? This can't be undone."
  - Discard all: "Discard changes to ⟨n⟩ files (⟨m⟩ untracked will be deleted)? This can't be undone." — omit the parenthetical when m = 0; "1 file" singular.
  - Toasts: "Discarded changes to ⟨path⟩", "Discarded all unstaged changes", "Added ⟨pattern⟩ to .gitignore", "Staged ⟨n⟩ lines" / "Unstaged ⟨n⟩ lines" / "Discarded ⟨n⟩ lines" ("1 line" singular).
- Context lines clamp to `0..=20`, default `3`. Highlight cap: 2,000 diff lines per file.
- Labels sentence case (FB-007).
- `CHANGELOG.md` entry required (pre-push hook) — Task 15.
- Commands: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`; `cd frontend && pnpm exec tsc --noEmit && pnpm lint && pnpm test -- --run`.

## Review Focus

1. **Staging lines while a whole-file context-lines value differs from the server default** — the hunk the user sees (e.g. context 0) must be the hunk the backend finds; staging must not fail with `HunkNotFound`. Test in Task 2 (`stage_lines_with_zero_context_matches_the_displayed_hunk`).
2. **Discarding a tracked file that also has staged changes** — only the working-tree changes go; the staged version survives in the index *and* becomes the working-tree content. Test in Task 3.
3. **A selection that would leave a "no newline at end of file" line in the middle of the patch** (old `a` without EOL, new `a\nb\n`, select only `+b`) — must fail with `SelectionNotApplicable` and leave index and workdir byte-for-byte unchanged, never corrupt the file. Test in Task 2.
4. **Discarding a deleted tracked file** — the file comes back with its index content. Test in Task 3.
5. **Ignore-whitespace on while viewing the staged side** — partial actions are disabled there too (both working-tree sides), but commit diffs stay fully readable; file-level stage/unstage buttons still work. Test in Task 10.

---

## File Map

| File | Responsibility |
|---|---|
| `crates/git-core/src/diff.rs` (modify) | `DiffOptions`, `working_diff_options`, options on `working_diff`/`commit_diff` |
| `crates/git-core/src/line_patch.rs` (create) | pure single-hunk patch builder for a line selection |
| `crates/git-core/src/stage.rs` (modify) | `context_lines` on hunk fns; `stage_lines`/`unstage_lines`/`discard_lines`; `discard_file`/`discard_all_unstaged`/`ignore_path`; new error variants |
| `crates/git-core/src/lib.rs` (modify) | `mod line_patch;` |
| `crates/git-core/tests/{diff.rs,stage_commit.rs}` (modify), `tests/stage_lines.rs`, `tests/discard_ignore.rs` (create) | real-repo tests |
| `crates/config/src/lib.rs` (modify), `crates/config/tests/diff_settings.rs` (create) | `DiffSettings` persistence |
| `crates/repo-service/src/worker/{mod.rs,status.rs}` (modify) | worker commands + handle methods |
| `crates/tauri-app/src/commands/{status.rs,mod.rs}`, `main.rs` (modify) | Tauri commands + DTOs |
| `crates/vscode-sidecar/src/dispatch.rs`, `tests/protocol_roundtrip.rs` (modify) | JSON-RPC methods |
| `frontend/src/ipc/{RepoClient.ts,tauriRepoClient.ts,vscodeRepoClient.ts}` (+ tests, + every full `RepoClient` literal in tests) | client surface |
| `frontend/src/state/useDiffSettings.tsx` (create) | settings hook + `DiffSettingsProvider` / `useDiffSettingsContext` |
| `frontend/src/state/{useStagingActions.ts,useAppState.ts}` (modify) | new staging actions, `contextLines` on hunk actions |
| `frontend/src/lib/lineSelection.ts` (create) | pure selection reducer |
| `frontend/src/lib/highlight.ts` (create), `frontend/src/lib/languageForPath.ts` (create) | Shiki wrapper, extension→language map |
| `frontend/src/components/DiffView.tsx` (+ `.module.css`) (modify) | selection, line actions, options toolbar, highlighting |
| `frontend/src/components/DiffLineContent.tsx` (create) | renders one line's content: tokens + word-diff marks |
| `frontend/src/components/FileActionsMenu.tsx` (create) | file "⋯" menu items + confirm dialog |
| `frontend/src/components/DiffPane.tsx`, `App.tsx`, `lib/commands.ts`, `components/ShortcutSheet.tsx` (modify) | wiring, palette, shortcut sheet |
| `frontend/src/styles` or global theme CSS (modify) | `--shiki-token-*` variables, light + dark |
| `e2e/specs/line-staging.spec.ts` (create), `extension/e2e/src/specs/line-staging.spec.ts` (create) | E2E |
| `docs/USER_GUIDE.md`, `CHANGELOG.md`, `docs/LICENSE_COMPLIANCE.md` (modify) | docs |

---

### Task 1: `DiffOptions` and context lines in git-core

**Files:**
- Modify: `crates/git-core/src/diff.rs`, `crates/git-core/src/stage.rs`
- Modify (call sites): `crates/git-core/tests/diff.rs`, `crates/git-core/tests/stage_commit.rs`, `crates/repo-service/src/worker/status.rs` (pass `&DiffOptions::default()` / `3` for now so the workspace compiles — Task 5 replaces these)

**Interfaces:**
- Produces:
  ```rust
  // git_core::diff
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub struct DiffOptions { pub ignore_whitespace: bool, pub context_lines: u32 }
  impl Default for DiffOptions { /* { ignore_whitespace: false, context_lines: 3 } */ }
  pub const MAX_CONTEXT_LINES: u32 = 20;
  pub fn working_diff(repo: &Repository, path: &str, staged: bool, opts: &DiffOptions) -> Result<Vec<DiffHunk>, DiffError>
  pub fn commit_diff(repo: &Repository, commit_id: &str, path: &str, opts: &DiffOptions) -> Result<Vec<DiffHunk>, DiffError>
  pub(crate) fn working_diff_options(path: &str, staged: bool, opts: &DiffOptions) -> git2::DiffOptions
  pub(crate) fn hunks_from_diff(diff: &git2::Diff) -> Result<Vec<DiffHunk>, DiffError>   // visibility widened only
  // git_core::stage
  pub fn stage_hunk(repo, path, old_start: u32, new_start: u32, context_lines: u32) -> Result<(), StageError>
  pub fn unstage_hunk(repo, path, old_start, new_start, context_lines) -> Result<(), StageError>
  pub fn discard_hunk(repo, path, old_start, new_start, context_lines) -> Result<(), StageError>
  ```

- [ ] **Step 1: Write the failing tests** — append to `crates/git-core/tests/diff.rs`:

```rust
use git_core::diff::DiffOptions;

fn twenty_lines() -> String {
    (1..=20).map(|n| format!("line {n}\n")).collect()
}

#[test]
fn context_lines_option_merges_or_splits_hunks() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", &twenty_lines());
    common::commit_all(&repo, "base");
    let changed = twenty_lines()
        .replace("line 2\n", "line 2 changed\n")
        .replace("line 10\n", "line 10 changed\n");
    common::write_file(dir.path(), "f.txt", &changed);

    let three = DiffOptions { ignore_whitespace: false, context_lines: 3 };
    let five = DiffOptions { ignore_whitespace: false, context_lines: 5 };
    assert_eq!(git_core::diff::working_diff(&repo, "f.txt", false, &three).unwrap().len(), 2);
    assert_eq!(git_core::diff::working_diff(&repo, "f.txt", false, &five).unwrap().len(), 1);
}

#[test]
fn context_lines_option_is_clamped_to_twenty() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", &(1..=60).map(|n| format!("l{n}\n")).collect::<String>());
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "f.txt", &(1..=60).map(|n| if n == 30 { "x\n".to_string() } else { format!("l{n}\n") }).collect::<String>());

    let huge = DiffOptions { ignore_whitespace: false, context_lines: 1000 };
    let hunk = &git_core::diff::working_diff(&repo, "f.txt", false, &huge).unwrap()[0];
    // 20 context lines on each side + the removed line.
    assert_eq!(hunk.old_lines, 41);
}

#[test]
fn ignore_whitespace_hides_whitespace_only_changes() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", "a b\nc\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "f.txt", "a   b\nc\n");

    let raw = DiffOptions::default();
    let ignoring = DiffOptions { ignore_whitespace: true, context_lines: 3 };
    assert_eq!(git_core::diff::working_diff(&repo, "f.txt", false, &raw).unwrap().len(), 1);
    assert!(git_core::diff::working_diff(&repo, "f.txt", false, &ignoring).unwrap().is_empty());
}

#[test]
fn commit_diff_honours_ignore_whitespace() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", "a b\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "f.txt", "a  b\n");
    common::commit_all(&repo, "spaces");
    let head = repo.head().unwrap().target().unwrap().to_string();

    let ignoring = DiffOptions { ignore_whitespace: true, context_lines: 3 };
    assert!(git_core::diff::commit_diff(&repo, &head, "f.txt", &ignoring).unwrap().is_empty());
}
```

Append to `crates/git-core/tests/stage_commit.rs`:

```rust
#[test]
fn stage_hunk_with_wider_context_stages_the_merged_hunk() {
    let (dir, repo) = common::init_repo();
    let base: String = (1..=20).map(|n| format!("line {n}\n")).collect();
    common::write_file(dir.path(), "f.txt", &base);
    common::commit_all(&repo, "base");
    let changed = base.replace("line 2\n", "line 2 changed\n").replace("line 10\n", "line 10 changed\n");
    common::write_file(dir.path(), "f.txt", &changed);

    let five = git_core::diff::DiffOptions { ignore_whitespace: false, context_lines: 5 };
    let hunks = git_core::diff::working_diff(&repo, "f.txt", false, &five).unwrap();
    assert_eq!(hunks.len(), 1);
    git_core::stage::stage_hunk(&repo, "f.txt", hunks[0].old_start, hunks[0].new_start, 5).unwrap();

    let remaining = git_core::diff::working_diff(&repo, "f.txt", false, &five).unwrap();
    assert!(remaining.is_empty(), "both edits staged in one go");
}
```

Update every existing call in both test files: `working_diff(&repo, p, s)` → `working_diff(&repo, p, s, &DiffOptions::default())`, `commit_diff(&repo, c, p)` → `commit_diff(&repo, c, p, &DiffOptions::default())`, `stage_hunk(&repo, p, o, n)` → `stage_hunk(&repo, p, o, n, 3)` (same for `unstage_hunk`, `discard_hunk`). Add `use git_core::diff::DiffOptions;` where needed.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p git-core --test diff --test stage_commit`
Expected: compile errors (`DiffOptions` not found, wrong arity).

- [ ] **Step 3: Implement** — in `diff.rs` add after the `DiffHunk` struct:

```rust
pub const MAX_CONTEXT_LINES: u32 = 20;

/// Display options shared by every diff the UI shows. Staging builds its diff through
/// `working_diff_options` with the same `context_lines`, so the `(old_start, new_start)` hunk
/// identity the UI captured always matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiffOptions {
    pub ignore_whitespace: bool,
    pub context_lines: u32,
}

impl Default for DiffOptions {
    fn default() -> Self {
        Self { ignore_whitespace: false, context_lines: 3 }
    }
}

fn apply_display_options(git_opts: &mut git2::DiffOptions, opts: &DiffOptions) {
    git_opts
        .ignore_whitespace(opts.ignore_whitespace)
        .context_lines(opts.context_lines.min(MAX_CONTEXT_LINES));
}

/// Single source of the git2 options for working-tree diffs (display *and* staging).
pub(crate) fn working_diff_options(path: &str, staged: bool, opts: &DiffOptions) -> git2::DiffOptions {
    let mut git_opts = git2::DiffOptions::new();
    // `pathspec` is a glob by default; callers pass literal paths from `status()`.
    git_opts.pathspec(path).disable_pathspec_match(true);
    if !staged {
        // Untracked files show as whole-file additions — see `working_diff`.
        git_opts
            .include_untracked(true)
            .recurse_untracked_dirs(true)
            .show_untracked_content(true);
    }
    apply_display_options(&mut git_opts, opts);
    git_opts
}
```

Rewrite `working_diff` to take `opts: &DiffOptions` and build with `working_diff_options(path, staged, opts)` (keep the existing explanatory comments, moving the untracked one into the helper). `commit_diff` takes `opts` and calls `apply_display_options(&mut git_opts, opts)` after the pathspec lines. Make `hunks_from_diff` `pub(crate)`.

In `stage.rs`, each of `stage_hunk`, `unstage_hunk`, `discard_hunk` gains `context_lines: u32` and builds its options via the helper:

```rust
use crate::diff::{working_diff_options, DiffOptions};

fn staging_opts(context_lines: u32) -> DiffOptions {
    // Partial staging is disabled in the UI while ignore-whitespace is on, so staging always
    // diffs with whitespace significant.
    DiffOptions { ignore_whitespace: false, context_lines }
}
```

- `stage_hunk`: `let mut opts = working_diff_options(path, false, &staging_opts(context_lines));` then `repo.diff_index_to_workdir(None, Some(&mut opts))`.
- `discard_hunk`: same, plus `opts.reverse(true);` before diffing.
- `unstage_hunk`: `let mut opts = working_diff_options(path, true, &staging_opts(context_lines)); opts.reverse(true);` then `diff_tree_to_index`.

Keep the existing hunk-callback matching and comments. In `repo-service/src/worker/status.rs`, temporarily pass `&git_core::diff::DiffOptions::default()` and `3` so the workspace builds.

- [ ] **Step 4: Run tests**

Run: `cargo test -p git-core && cargo build --workspace`
Expected: PASS / builds.

- [ ] **Step 5: Commit**

```bash
git add crates/git-core crates/repo-service
git commit -m "feat(git-core): diff options (ignore whitespace, context lines) shared by display and staging"
```

---

### Task 2: Line staging in git-core

**Files:**
- Create: `crates/git-core/src/line_patch.rs`, `crates/git-core/tests/stage_lines.rs`
- Modify: `crates/git-core/src/lib.rs` (`mod line_patch;` — private), `crates/git-core/src/stage.rs`

**Interfaces:**
- Consumes: Task 1 `working_diff_options`, `DiffOptions`.
- Produces:
  ```rust
  // git_core::stage
  pub fn stage_lines(repo: &git2::Repository, path: &str, old_start: u32, new_start: u32, context_lines: u32, line_indices: &[u32]) -> Result<(), StageError>
  pub fn unstage_lines(/* same */) -> Result<(), StageError>
  pub fn discard_lines(/* same */) -> Result<(), StageError>
  // StageError gains:
  #[error("Select at least one added or removed line.")] EmptySelection,
  #[error("Only added or removed lines can be staged.")] ContextLineSelected,
  #[error("Couldn't apply just those lines. Try the whole hunk instead.")] SelectionNotApplicable,
  ```
- Line index contract: indices into the hunk's lines in the exact order `diff::hunks_from_diff` emits them (EOF-marker lines included, as `Context`) for the *forward* diff the user sees: index→workdir for stage/discard, HEAD→index for unstage.

- [ ] **Step 1: Write the failing tests** — `crates/git-core/tests/stage_lines.rs`:

```rust
mod common;

use git_core::diff::{working_diff, DiffOptions};
use git_core::stage::{discard_lines, stage_file, stage_lines, unstage_lines, StageError};

fn index_content(repo: &git2::Repository, path: &str) -> Option<String> {
    let mut index = repo.index().unwrap();
    index.read(true).unwrap();
    let entry = index.get_path(std::path::Path::new(path), 0)?;
    Some(String::from_utf8(repo.find_blob(entry.id).unwrap().content().to_vec()).unwrap())
}

fn workdir_content(dir: &std::path::Path, path: &str) -> String {
    std::fs::read_to_string(dir.join(path)).unwrap()
}

/// base "a\nb\nc\n" → workdir "a\nB\nc\nd\n". Hunk lines (context 3):
/// 0 " a", 1 "-b", 2 "+B", 3 " c", 4 "+d".
fn setup_abc() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", "a\nb\nc\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "f.txt", "a\nB\nc\nd\n");
    (dir, repo)
}

fn only_hunk(repo: &git2::Repository, staged: bool) -> (u32, u32) {
    let hunks = working_diff(repo, "f.txt", staged, &DiffOptions::default()).unwrap();
    assert_eq!(hunks.len(), 1);
    (hunks[0].old_start, hunks[0].new_start)
}

#[test]
fn stages_only_the_selected_added_line() {
    let (dir, repo) = setup_abc();
    let (o, n) = only_hunk(&repo, false);
    stage_lines(&repo, "f.txt", o, n, 3, &[4]).unwrap();
    assert_eq!(index_content(&repo, "f.txt").unwrap(), "a\nb\nc\nd\n");
    assert_eq!(workdir_content(dir.path(), "f.txt"), "a\nB\nc\nd\n");
}

#[test]
fn stages_only_the_selected_removed_line() {
    let (_dir, repo) = setup_abc();
    let (o, n) = only_hunk(&repo, false);
    stage_lines(&repo, "f.txt", o, n, 3, &[1]).unwrap();
    assert_eq!(index_content(&repo, "f.txt").unwrap(), "a\nc\n");
}

#[test]
fn stages_a_mixed_selection() {
    let (_dir, repo) = setup_abc();
    let (o, n) = only_hunk(&repo, false);
    stage_lines(&repo, "f.txt", o, n, 3, &[1, 2]).unwrap();
    assert_eq!(index_content(&repo, "f.txt").unwrap(), "a\nB\nc\n");
}

#[test]
fn stages_selected_lines_of_an_untracked_file() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "seed.txt", "seed\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "new.txt", "x\ny\nz\n");
    let hunks = working_diff(&repo, "new.txt", false, &DiffOptions::default()).unwrap();
    stage_lines(&repo, "new.txt", hunks[0].old_start, hunks[0].new_start, 3, &[1]).unwrap();
    assert_eq!(index_content(&repo, "new.txt").unwrap(), "y\n");
}

#[test]
fn stages_an_added_last_line_without_trailing_newline() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", "a\nb\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "f.txt", "a\nb\nc");
    let hunks = working_diff(&repo, "f.txt", false, &DiffOptions::default()).unwrap();
    let add = hunks[0].lines.iter().position(|l| l.content.starts_with('c')).unwrap() as u32;
    stage_lines(&repo, "f.txt", hunks[0].old_start, hunks[0].new_start, 3, &[add]).unwrap();
    assert_eq!(index_content(&repo, "f.txt").unwrap(), "a\nb\nc");
}

#[test]
fn staging_every_changed_line_of_a_no_eol_file_matches_the_workdir() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", "a\nb");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "f.txt", "a\nb\nc");
    let hunks = working_diff(&repo, "f.txt", false, &DiffOptions::default()).unwrap();
    let changed: Vec<u32> = hunks[0]
        .lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.origin != git_core::diff::DiffLineOrigin::Context)
        .map(|(i, _)| i as u32)
        .collect();
    stage_lines(&repo, "f.txt", hunks[0].old_start, hunks[0].new_start, 3, &changed).unwrap();
    assert_eq!(index_content(&repo, "f.txt").unwrap(), "a\nb\nc");
}

#[test]
fn a_selection_that_strands_a_no_eol_line_fails_cleanly() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", "a");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "f.txt", "a\nb\n");
    let hunks = working_diff(&repo, "f.txt", false, &DiffOptions::default()).unwrap();
    let add_b = hunks[0].lines.iter().position(|l| l.content == "b\n").unwrap() as u32;

    let result = stage_lines(&repo, "f.txt", hunks[0].old_start, hunks[0].new_start, 3, &[add_b]);

    assert!(matches!(result, Err(StageError::SelectionNotApplicable)), "{result:?}");
    assert_eq!(index_content(&repo, "f.txt").unwrap(), "a");
    assert_eq!(workdir_content(dir.path(), "f.txt"), "a\nb\n");
}

#[test]
fn stage_lines_with_zero_context_matches_the_displayed_hunk() {
    let (_dir, repo) = setup_abc();
    let zero = DiffOptions { ignore_whitespace: false, context_lines: 0 };
    let hunks = working_diff(&repo, "f.txt", false, &zero).unwrap();
    // With zero context: hunk 0 = "-b","+B"; hunk 1 = "+d".
    assert_eq!(hunks.len(), 2);
    stage_lines(&repo, "f.txt", hunks[1].old_start, hunks[1].new_start, 0, &[0]).unwrap();
    assert_eq!(index_content(&repo, "f.txt").unwrap(), "a\nb\nc\nd\n");
}

#[test]
fn unstages_only_the_selected_line() {
    let (_dir, repo) = setup_abc();
    stage_file(&repo, "f.txt").unwrap();
    let (o, n) = only_hunk(&repo, true);
    // HEAD→index hunk has the same layout: 0 " a", 1 "-b", 2 "+B", 3 " c", 4 "+d".
    unstage_lines(&repo, "f.txt", o, n, 3, &[4]).unwrap();
    assert_eq!(index_content(&repo, "f.txt").unwrap(), "a\nB\nc\n");
}

#[test]
fn unstaging_a_removed_line_restores_it_in_the_index() {
    let (_dir, repo) = setup_abc();
    stage_file(&repo, "f.txt").unwrap();
    let (o, n) = only_hunk(&repo, true);
    unstage_lines(&repo, "f.txt", o, n, 3, &[1]).unwrap();
    assert_eq!(index_content(&repo, "f.txt").unwrap(), "a\nb\nB\nc\nd\n");
}

#[test]
fn discards_only_the_selected_lines() {
    let (dir, repo) = setup_abc();
    let (o, n) = only_hunk(&repo, false);
    discard_lines(&repo, "f.txt", o, n, 3, &[1, 2]).unwrap();
    assert_eq!(workdir_content(dir.path(), "f.txt").replace("\r\n", "\n"), "a\nb\nc\nd\n");
    assert_eq!(index_content(&repo, "f.txt").unwrap(), "a\nb\nc\n");
}

#[test]
fn stale_hunk_is_reported() {
    let (_dir, repo) = setup_abc();
    assert!(matches!(stage_lines(&repo, "f.txt", 99, 99, 3, &[1]), Err(StageError::HunkNotFound)));
}

#[test]
fn out_of_range_index_is_reported_as_stale() {
    let (_dir, repo) = setup_abc();
    let (o, n) = only_hunk(&repo, false);
    assert!(matches!(stage_lines(&repo, "f.txt", o, n, 3, &[42]), Err(StageError::HunkNotFound)));
}

#[test]
fn context_and_empty_selections_are_rejected() {
    let (_dir, repo) = setup_abc();
    let (o, n) = only_hunk(&repo, false);
    assert!(matches!(stage_lines(&repo, "f.txt", o, n, 3, &[0]), Err(StageError::ContextLineSelected)));
    assert!(matches!(stage_lines(&repo, "f.txt", o, n, 3, &[]), Err(StageError::EmptySelection)));
    assert_eq!(index_content(&repo, "f.txt").unwrap(), "a\nb\nc\n");
}
```

Also add pure unit tests at the bottom of `line_patch.rs` (Step 3 shows them).

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p git-core --test stage_lines`
Expected: compile error (`stage_lines` not found).

- [ ] **Step 3: Implement**

`crates/git-core/src/line_patch.rs`:

```rust
//! Builds a single-hunk unified patch that applies only some of a hunk's added/removed lines.
//! Pure: no repository access, so the selection rules are unit-testable on their own.

use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    /// Apply the selected lines as they appear (stage).
    Forward,
    /// Undo the selected lines (unstage, discard).
    Reverse,
}

#[derive(Debug, Clone)]
pub(crate) struct PatchLine {
    /// libgit2 origin: '+', '-', ' ', or an EOF marker ('=', '>', '<').
    pub origin: char,
    pub content: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LinePatchError {
    OutOfRange,
    ContextSelected,
    Empty,
    /// The selection would put a line without a trailing newline before another line on the
    /// same side — not expressible as a valid patch.
    NotApplicable,
}

pub(crate) struct HunkRef {
    pub old_start: u32,
    pub new_start: u32,
}

/// File-header facts the patch needs, taken from the forward diff's delta.
pub(crate) struct FileRef<'a> {
    pub path: &'a str,
    /// Forward old side absent (untracked / added file).
    pub old_absent: bool,
    /// Forward new side absent (deleted file).
    pub new_absent: bool,
    /// Octal mode of the side that exists, e.g. 0o100644.
    pub mode: u32,
}

pub(crate) fn build(
    file: &FileRef,
    hunk: &HunkRef,
    lines: &[PatchLine],
    selected: &[u32],
    direction: Direction,
) -> Result<Vec<u8>, LinePatchError> {
    let selected: BTreeSet<usize> = selected.iter().map(|&i| i as usize).collect();
    if selected.is_empty() {
        return Err(LinePatchError::Empty);
    }
    for &i in &selected {
        let line = lines.get(i).ok_or(LinePatchError::OutOfRange)?;
        if line.origin != '+' && line.origin != '-' {
            return Err(LinePatchError::ContextSelected);
        }
    }

    let mut body: Vec<(char, &[u8])> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let picked = selected.contains(&i);
        let emitted = match (line.origin, direction) {
            (' ', _) => Some(' '),
            ('+', Direction::Forward) => picked.then_some('+'),
            ('-', Direction::Forward) => Some(if picked { '-' } else { ' ' }),
            ('+', Direction::Reverse) => Some(if picked { '-' } else { ' ' }),
            ('-', Direction::Reverse) => picked.then_some('+'),
            // EOF markers are re-derived from each emitted line's own trailing newline.
            _ => None,
        };
        if let Some(origin) = emitted {
            body.push((origin, &line.content));
        }
    }

    // A line lacking its trailing newline must be the last line of each side it belongs to
    // (old side = ' ' and '-', new side = ' ' and '+').
    for (i, (origin, content)) in body.iter().enumerate() {
        if content.ends_with(b"\n") {
            continue;
        }
        let later_on_same_side = body[i + 1..].iter().any(|(o, _)| match origin {
            ' ' => true,
            '-' => *o != '+',
            _ => *o != '-',
        });
        if later_on_same_side {
            return Err(LinePatchError::NotApplicable);
        }
    }

    let old_count = body.iter().filter(|(o, _)| *o != '+').count() as u32;
    let new_count = body.iter().filter(|(o, _)| *o != '-').count() as u32;
    // The preimage side is an existing side of the forward diff, so its start is exact. libgit2
    // locates hunks by the preimage only; the postimage start just has to be well-formed.
    let old_start = match direction {
        Direction::Forward => hunk.old_start,
        Direction::Reverse => hunk.new_start,
    };
    let new_start = if new_count == 0 { old_start.saturating_sub(1) } else { old_start.max(1) };

    // The patch's preimage is the forward old side (Forward) or the forward new side (Reverse).
    // When that side doesn't exist yet the patch creates the file. The postimage always exists:
    // a partial selection never deletes a file (selecting every line leaves it empty).
    let preimage_absent = match direction {
        Direction::Forward => file.old_absent,
        Direction::Reverse => file.new_absent,
    };

    let mut out = Vec::new();
    let p = file.path;
    out.extend_from_slice(format!("diff --git a/{p} b/{p}\n").as_bytes());
    if preimage_absent {
        out.extend_from_slice(format!("new file mode {:o}\n--- /dev/null\n", file.mode).as_bytes());
    } else {
        out.extend_from_slice(format!("--- a/{p}\n").as_bytes());
    }
    out.extend_from_slice(format!("+++ b/{p}\n").as_bytes());
    out.extend_from_slice(
        format!("@@ -{old_start},{old_count} +{new_start},{new_count} @@\n").as_bytes(),
    );
    for (origin, content) in body {
        out.push(origin as u8);
        out.extend_from_slice(content);
        if !content.ends_with(b"\n") {
            out.extend_from_slice(b"\n\\ No newline at end of file\n");
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l(origin: char, s: &str) -> PatchLine {
        PatchLine { origin, content: s.as_bytes().to_vec() }
    }

    fn file() -> FileRef<'static> {
        FileRef { path: "f.txt", old_absent: false, new_absent: false, mode: 0o100644 }
    }

    fn lines() -> Vec<PatchLine> {
        vec![l(' ', "a\n"), l('-', "b\n"), l('+', "B\n"), l(' ', "c\n"), l('+', "d\n")]
    }

    #[test]
    fn forward_keeps_selected_adds_and_turns_unselected_removes_into_context() {
        let patch = build(&file(), &HunkRef { old_start: 1, new_start: 1 }, &lines(), &[4], Direction::Forward).unwrap();
        let text = String::from_utf8(patch).unwrap();
        assert!(text.ends_with("@@ -1,3 +1,4 @@\n a\n b\n c\n+d\n"), "{text}");
    }

    #[test]
    fn reverse_turns_selected_adds_into_removes_and_drops_unselected_removes() {
        let patch = build(&file(), &HunkRef { old_start: 1, new_start: 1 }, &lines(), &[2], Direction::Reverse).unwrap();
        let text = String::from_utf8(patch).unwrap();
        assert!(text.ends_with("@@ -1,4 +1,3 @@\n a\n-B\n c\n d\n"), "{text}");
    }

    #[test]
    fn missing_trailing_newline_gets_a_marker() {
        let lines = vec![l(' ', "a\n"), l('+', "b")];
        let patch = build(&file(), &HunkRef { old_start: 1, new_start: 1 }, &lines, &[1], Direction::Forward).unwrap();
        assert!(String::from_utf8(patch).unwrap().ends_with("+b\n\\ No newline at end of file\n"));
    }

    #[test]
    fn new_file_header_for_an_absent_old_side() {
        let f = FileRef { old_absent: true, ..file() };
        let lines = vec![l('+', "x\n"), l('+', "y\n")];
        let patch = build(&f, &HunkRef { old_start: 0, new_start: 1 }, &lines, &[1], Direction::Forward).unwrap();
        let text = String::from_utf8(patch).unwrap();
        assert!(text.contains("new file mode 100644\n--- /dev/null\n+++ b/f.txt\n@@ -0,0 +1,1 @@\n+y\n"), "{text}");
    }

    #[test]
    fn a_no_eol_line_followed_by_a_same_side_line_is_not_applicable() {
        // old "a" (no EOL) → new "a\nb\n"; select only "+b\n".
        let lines = vec![l('-', "a"), l('<', "\n\\ No newline at end of file\n"), l('+', "a\n"), l('+', "b\n")];
        let h = HunkRef { old_start: 1, new_start: 1 };
        assert_eq!(build(&file(), &h, &lines, &[3], Direction::Forward), Err(LinePatchError::NotApplicable));
        // Selecting all changed lines is fine: "-a" + marker is the old side's last line.
        assert!(build(&file(), &h, &lines, &[0, 2, 3], Direction::Forward).is_ok());
    }

    #[test]
    fn validation_errors() {
        let h = HunkRef { old_start: 1, new_start: 1 };
        assert_eq!(build(&file(), &h, &lines(), &[], Direction::Forward), Err(LinePatchError::Empty));
        assert_eq!(build(&file(), &h, &lines(), &[0], Direction::Forward), Err(LinePatchError::ContextSelected));
        assert_eq!(build(&file(), &h, &lines(), &[9], Direction::Forward), Err(LinePatchError::OutOfRange));
    }
}
```

`stage.rs` additions:

```rust
use crate::line_patch::{self, Direction, FileRef, HunkRef, LinePatchError, PatchLine};

// StageError — add variants:
    #[error("Select at least one added or removed line.")]
    EmptySelection,
    #[error("Only added or removed lines can be staged.")]
    ContextLineSelected,
    #[error("Couldn't apply just those lines. Try the whole hunk instead.")]
    SelectionNotApplicable,

impl From<LinePatchError> for StageError {
    fn from(error: LinePatchError) -> Self {
        match error {
            LinePatchError::OutOfRange => StageError::HunkNotFound,
            LinePatchError::ContextSelected => StageError::ContextLineSelected,
            LinePatchError::Empty => StageError::EmptySelection,
            LinePatchError::NotApplicable => StageError::SelectionNotApplicable,
        }
    }
}

struct FoundHunk {
    old_absent: bool,
    new_absent: bool,
    mode: u32,
    lines: Vec<PatchLine>,
}

/// Re-walks `diff` the same way `diff::hunks_from_diff` does and returns the lines of the hunk
/// at `(old_start, new_start)` with their raw libgit2 origins, so indices line up with the UI.
fn find_hunk(diff: &git2::Diff, old_start: u32, new_start: u32) -> Result<FoundHunk, StageError> {
    use std::cell::RefCell;
    let found: RefCell<Option<FoundHunk>> = RefCell::new(None);
    let in_target = std::cell::Cell::new(false);
    diff.foreach(
        &mut |_, _| true,
        None,
        Some(&mut |delta, hunk| {
            let is_target = hunk.old_start() == old_start && hunk.new_start() == new_start;
            in_target.set(is_target);
            if is_target {
                let old_absent = matches!(delta.status(), git2::Delta::Untracked | git2::Delta::Added);
                let new_absent = matches!(delta.status(), git2::Delta::Deleted);
                let mode_side = if new_absent { delta.old_file() } else { delta.new_file() };
                *found.borrow_mut() = Some(FoundHunk {
                    old_absent,
                    new_absent,
                    mode: u32::from(mode_side.mode()),
                    lines: Vec::new(),
                });
            }
            true
        }),
        Some(&mut |_, _, line| {
            if in_target.get() {
                if let Some(h) = found.borrow_mut().as_mut() {
                    h.lines.push(PatchLine { origin: line.origin(), content: line.content().to_vec() });
                }
            }
            true
        }),
    )?;
    found.into_inner().ok_or(StageError::HunkNotFound)
}

fn apply_lines(
    repo: &git2::Repository,
    path: &str,
    old_start: u32,
    new_start: u32,
    context_lines: u32,
    line_indices: &[u32],
    staged_side: bool,
    direction: Direction,
    location: git2::ApplyLocation,
) -> Result<(), StageError> {
    let mut opts = working_diff_options(path, staged_side, &staging_opts(context_lines));
    let diff = if staged_side {
        let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
        repo.diff_tree_to_index(head_tree.as_ref(), None, Some(&mut opts))?
    } else {
        repo.diff_index_to_workdir(None, Some(&mut opts))?
    };
    let hunk = find_hunk(&diff, old_start, new_start)?;
    let file = FileRef { path, old_absent: hunk.old_absent, new_absent: hunk.new_absent, mode: hunk.mode };
    let patch = line_patch::build(&file, &HunkRef { old_start, new_start }, &hunk.lines, line_indices, direction)?;
    let parsed = git2::Diff::from_buffer(&patch).map_err(|_| StageError::SelectionNotApplicable)?;
    repo.apply(&parsed, location, None).map_err(|_| StageError::SelectionNotApplicable)
}

pub fn stage_lines(repo: &git2::Repository, path: &str, old_start: u32, new_start: u32, context_lines: u32, line_indices: &[u32]) -> Result<(), StageError> {
    apply_lines(repo, path, old_start, new_start, context_lines, line_indices, false, Direction::Forward, git2::ApplyLocation::Index)
}

pub fn unstage_lines(repo: &git2::Repository, path: &str, old_start: u32, new_start: u32, context_lines: u32, line_indices: &[u32]) -> Result<(), StageError> {
    apply_lines(repo, path, old_start, new_start, context_lines, line_indices, true, Direction::Reverse, git2::ApplyLocation::Index)
}

pub fn discard_lines(repo: &git2::Repository, path: &str, old_start: u32, new_start: u32, context_lines: u32, line_indices: &[u32]) -> Result<(), StageError> {
    apply_lines(repo, path, old_start, new_start, context_lines, line_indices, false, Direction::Reverse, git2::ApplyLocation::WorkDir)
}
```

`apply_lines` has 9 parameters — add `#[allow(clippy::too_many_arguments)]` on it. `git2::FileMode` → `u32`: use `u32::from(mode)` if `From` is implemented in git2 0.21, else `mode as u32` via `i32::from(mode) as u32`; check with `cargo doc -p git2 --open` or the compiler.

- [ ] **Step 4: Run tests**

Run: `cargo test -p git-core --test stage_lines && cargo test -p git-core line_patch && cargo clippy -p git-core --all-targets -- -D warnings`
Expected: PASS. If an integration test fails because libgit2 emits EOF markers differently than the `line_patch` unit tests assume (e.g. the marker attached to a different line), print the raw `(origin, content)` pairs from `find_hunk` for that fixture and adjust only the unit-test fixtures — the rule "a line without trailing `\n` must be last on its side" stays.

- [ ] **Step 5: Commit**

```bash
git add crates/git-core
git commit -m "feat(git-core): stage, unstage and discard selected lines of a hunk"
```

---

### Task 3: Discard file, discard all unstaged, ignore path

**Files:**
- Modify: `crates/git-core/src/stage.rs`
- Create: `crates/git-core/tests/discard_ignore.rs`

**Interfaces:**
- Produces:
  ```rust
  pub fn discard_file(repo: &git2::Repository, path: &str) -> Result<(), StageError>
  pub fn discard_all_unstaged(repo: &git2::Repository) -> Result<(), StageError>
  pub fn ignore_path(repo: &git2::Repository, pattern: &str) -> Result<(), StageError>
  // StageError gains:
  #[error("That path is outside the repository.")] InvalidPath,
  #[error("Ignore pattern can't be empty or span several lines.")] InvalidPattern,
  #[error("failed to update the working tree: {0}")] Io(#[from] std::io::Error),
  ```
- Note: `status()` uses `recurse_untracked_dirs(true)`, so it never reports a directory; `discard_file` handles files only (spec's "untracked directory" case cannot occur).

- [ ] **Step 1: Write the failing tests** — `crates/git-core/tests/discard_ignore.rs`:

```rust
mod common;

use git_core::stage::{discard_all_unstaged, discard_file, ignore_path, stage_file, StageError};

fn read(dir: &std::path::Path, p: &str) -> String {
    std::fs::read_to_string(dir.join(p)).unwrap().replace("\r\n", "\n")
}

#[test]
fn discard_file_restores_a_modified_tracked_file() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", "v1\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "f.txt", "v2\n");
    discard_file(&repo, "f.txt").unwrap();
    assert_eq!(read(dir.path(), "f.txt"), "v1\n");
}

#[test]
fn discard_file_keeps_staged_changes() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", "v1\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "f.txt", "v2\n");
    stage_file(&repo, "f.txt").unwrap();
    common::write_file(dir.path(), "f.txt", "v3\n");

    discard_file(&repo, "f.txt").unwrap();

    assert_eq!(read(dir.path(), "f.txt"), "v2\n");
    let statuses = git_core::status::status(&repo).unwrap();
    assert!(statuses.iter().any(|e| e.path == "f.txt" && e.staged));
    assert!(!statuses.iter().any(|e| e.path == "f.txt" && !e.staged));
}

#[test]
fn discard_file_restores_a_deleted_tracked_file() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", "v1\n");
    common::commit_all(&repo, "base");
    std::fs::remove_file(dir.path().join("f.txt")).unwrap();
    discard_file(&repo, "f.txt").unwrap();
    assert_eq!(read(dir.path(), "f.txt"), "v1\n");
}

#[test]
fn discard_file_deletes_an_untracked_file() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "seed.txt", "s\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "nested/new.txt", "n\n");
    discard_file(&repo, "nested/new.txt").unwrap();
    assert!(!dir.path().join("nested/new.txt").exists());
}

#[test]
fn discard_file_rejects_paths_outside_the_repo() {
    let (_dir, repo) = common::init_repo();
    assert!(matches!(discard_file(&repo, "../escape.txt"), Err(StageError::InvalidPath)));
    assert!(matches!(discard_file(&repo, "/etc/passwd"), Err(StageError::InvalidPath)));
}

#[test]
fn discard_all_unstaged_resets_tracked_deletes_untracked_and_keeps_the_index() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a1\n");
    common::write_file(dir.path(), "b.txt", "b1\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "a.txt", "a2\n");
    common::write_file(dir.path(), "b.txt", "b2\n");
    stage_file(&repo, "b.txt").unwrap();
    common::write_file(dir.path(), "u.txt", "u\n");

    discard_all_unstaged(&repo).unwrap();

    assert_eq!(read(dir.path(), "a.txt"), "a1\n");
    assert_eq!(read(dir.path(), "b.txt"), "b2\n");
    assert!(!dir.path().join("u.txt").exists());
    let statuses = git_core::status::status(&repo).unwrap();
    assert!(statuses.iter().all(|e| e.staged), "{statuses:?}");
}

#[test]
fn ignore_path_creates_gitignore() {
    let (dir, repo) = common::init_repo();
    ignore_path(&repo, "/build.log").unwrap();
    assert_eq!(read(dir.path(), ".gitignore"), "/build.log\n");
}

#[test]
fn ignore_path_appends_and_fixes_a_missing_trailing_newline() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), ".gitignore", "target");
    ignore_path(&repo, "*.log").unwrap();
    assert_eq!(read(dir.path(), ".gitignore"), "target\n*.log\n");
}

#[test]
fn ignore_path_skips_duplicates() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), ".gitignore", "*.log\n");
    ignore_path(&repo, "*.log").unwrap();
    assert_eq!(read(dir.path(), ".gitignore"), "*.log\n");
}

#[test]
fn ignore_path_rejects_empty_and_multiline_patterns() {
    let (_dir, repo) = common::init_repo();
    assert!(matches!(ignore_path(&repo, "  "), Err(StageError::InvalidPattern)));
    assert!(matches!(ignore_path(&repo, "a\nb"), Err(StageError::InvalidPattern)));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p git-core --test discard_ignore`
Expected: compile error.

- [ ] **Step 3: Implement** — in `stage.rs`:

```rust
use std::path::Component;

fn checked_relative(path: &str) -> Result<&Path, StageError> {
    let p = Path::new(path);
    if p.is_absolute() || p.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(StageError::InvalidPath);
    }
    Ok(p)
}

fn is_tracked_in_index(repo: &git2::Repository, path: &Path) -> Result<bool, StageError> {
    Ok(repo.index()?.get_path(path, 0).is_some())
}

pub fn discard_file(repo: &git2::Repository, path: &str) -> Result<(), StageError> {
    let rel = checked_relative(path)?;
    let workdir = repo.workdir().ok_or(StageError::NoWorkdir)?;
    if is_tracked_in_index(repo, rel)? {
        let mut checkout = git2::build::CheckoutBuilder::new();
        checkout.force().path(rel).disable_pathspec_match(true);
        repo.checkout_index(None, Some(&mut checkout))?;
    } else {
        let full = workdir.join(rel);
        if full.exists() {
            std::fs::remove_file(full)?;
        }
    }
    Ok(())
}

pub fn discard_all_unstaged(repo: &git2::Repository) -> Result<(), StageError> {
    let entries = crate::status::status(repo).map_err(|e| StageError::Index(git2::Error::from_str(&e.to_string())))?;
    for entry in entries.iter().filter(|e| !e.staged && e.kind != crate::status::StatusKind::Conflicted) {
        discard_file(repo, &entry.path)?;
    }
    Ok(())
}

pub fn ignore_path(repo: &git2::Repository, pattern: &str) -> Result<(), StageError> {
    let pattern = pattern.trim();
    if pattern.is_empty() || pattern.contains('\n') || pattern.contains('\r') {
        return Err(StageError::InvalidPattern);
    }
    let file = repo.workdir().ok_or(StageError::NoWorkdir)?.join(".gitignore");
    let existing = std::fs::read_to_string(&file).unwrap_or_default();
    if existing.lines().any(|line| line.trim_end() == pattern) {
        return Ok(());
    }
    let mut next = existing;
    if !next.is_empty() && !next.ends_with('\n') {
        next.push('\n');
    }
    next.push_str(pattern);
    next.push('\n');
    std::fs::write(&file, next)?;
    Ok(())
}
```

Add the `InvalidPath`, `InvalidPattern`, `Io` variants. Check `crate::status` error type name (`StatusError`?) — if `StageError` can't wrap it cleanly, add `#[error("failed to read status: {0}")] Status(String)` and map with `.map_err(|e| StageError::Status(e.to_string()))` instead of the `from_str` hack. `StatusKind::Conflicted` — confirm the variant name in `status.rs`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p git-core --test discard_ignore && cargo clippy -p git-core --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/git-core
git commit -m "feat(git-core): discard file, discard all unstaged, add to .gitignore"
```

---

### Task 4: Diff settings in `config`

**Files:**
- Modify: `crates/config/src/lib.rs`
- Create: `crates/config/tests/diff_settings.rs`

**Interfaces:**
- Produces:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
  pub struct DiffSettings { pub ignore_whitespace: bool, pub context_lines: u32, pub syntax_highlight: bool }
  impl Default for DiffSettings { /* false, 3, true */ }
  pub fn get_diff_settings() -> Result<DiffSettings, ConfigError>
  pub fn set_diff_settings(settings: &DiffSettings) -> Result<(), ConfigError>
  pub fn get_diff_settings_at(config_file: &Path) -> Result<DiffSettings, ConfigError>
  pub fn set_diff_settings_at(config_file: &Path, settings: &DiffSettings) -> Result<(), ConfigError>
  ```

- [ ] **Step 1: Write the failing tests** — `crates/config/tests/diff_settings.rs`:

```rust
use config::{get_diff_settings_at, set_diff_settings_at, set_last_seen_version_at, DiffSettings};

#[test]
fn defaults_when_never_set() {
    let dir = tempfile::TempDir::new().unwrap();
    let s = get_diff_settings_at(&dir.path().join("config.toml")).unwrap();
    assert_eq!(s, DiffSettings { ignore_whitespace: false, context_lines: 3, syntax_highlight: true });
}

#[test]
fn round_trips_and_coexists_with_other_keys() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    set_last_seen_version_at(&file, "1.0.0").unwrap();
    let wanted = DiffSettings { ignore_whitespace: true, context_lines: 7, syntax_highlight: false };
    set_diff_settings_at(&file, &wanted).unwrap();
    assert_eq!(get_diff_settings_at(&file).unwrap(), wanted);
    assert_eq!(config::get_last_seen_version_at(&file).unwrap().as_deref(), Some("1.0.0"));
}

#[test]
fn clamps_context_lines_on_write() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    set_diff_settings_at(&file, &DiffSettings { ignore_whitespace: false, context_lines: 99, syntax_highlight: true }).unwrap();
    assert_eq!(get_diff_settings_at(&file).unwrap().context_lines, 20);
}

#[test]
fn a_partial_diff_table_fills_missing_fields_with_defaults() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    std::fs::write(&file, "[diff]\nignore_whitespace = true\n").unwrap();
    let s = get_diff_settings_at(&file).unwrap();
    assert!(s.ignore_whitespace);
    assert_eq!(s.context_lines, 3);
    assert!(s.syntax_highlight);
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p config --test diff_settings`
Expected: compile error.

- [ ] **Step 3: Implement**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DiffSettings {
    pub ignore_whitespace: bool,
    pub context_lines: u32,
    pub syntax_highlight: bool,
}

impl Default for DiffSettings {
    fn default() -> Self {
        Self { ignore_whitespace: false, context_lines: 3, syntax_highlight: true }
    }
}

const MAX_CONTEXT_LINES: u32 = 20;
```

Add to `ConfigFile`, as the **first** field of the trailing table group (after `last_seen_version`, before `open_repos`; it serializes as a `[diff]` table, which must follow plain keys):

```rust
    #[serde(default)]
    diff: DiffSettings,
```

Functions, next to the last-seen-version ones:

```rust
pub fn get_diff_settings() -> Result<DiffSettings, ConfigError> {
    get_diff_settings_at(&config_file_path()?)
}

pub fn set_diff_settings(settings: &DiffSettings) -> Result<(), ConfigError> {
    set_diff_settings_at(&config_file_path()?, settings)
}

pub fn get_diff_settings_at(config_file: &Path) -> Result<DiffSettings, ConfigError> {
    Ok(read_config(config_file)?.diff)
}

pub fn set_diff_settings_at(config_file: &Path, settings: &DiffSettings) -> Result<(), ConfigError> {
    let mut config = read_config(config_file)?;
    config.diff = DiffSettings {
        context_lines: settings.context_lines.min(MAX_CONTEXT_LINES),
        ..*settings
    };
    write_config(config_file, &config)
}
```

Update the comment above the trailing fields to mention `diff` as a table.

- [ ] **Step 4: Run tests**

Run: `cargo test -p config`
Expected: PASS (all existing config tests too — this proves the TOML ordering).

- [ ] **Step 5: Commit**

```bash
git add crates/config
git commit -m "feat(config): persist global diff settings"
```

---

### Task 5: repo-service worker commands

**Files:**
- Modify: `crates/repo-service/src/worker/mod.rs` (`Command` variants, dispatch arms, tests), `crates/repo-service/src/worker/status.rs` (handlers + `WorkerHandle` methods)

**Interfaces:**
- Consumes: Tasks 1–3.
- Produces (`WorkerHandle`):
  ```rust
  pub fn get_working_diff(&self, path: String, staged: bool, opts: git_core::diff::DiffOptions) -> Result<Vec<DiffHunk>, String>
  pub fn get_commit_diff(&self, commit_id: String, path: String, opts: git_core::diff::DiffOptions) -> Result<Vec<DiffHunk>, String>
  pub fn stage_hunk(&self, path: String, old_start: u32, new_start: u32, context_lines: u32) -> Result<(), String>
  pub fn unstage_hunk(&self, path: String, old_start: u32, new_start: u32, context_lines: u32) -> Result<(), String>
  pub fn discard_hunk(&self, path: String, old_start: u32, new_start: u32, context_lines: u32) -> Result<(), String>
  pub fn stage_lines(&self, path: String, old_start: u32, new_start: u32, context_lines: u32, line_indices: Vec<u32>) -> Result<(), String>
  pub fn unstage_lines(/* same */) -> Result<(), String>
  pub fn discard_lines(/* same */) -> Result<(), String>
  pub fn discard_file(&self, path: String) -> Result<(), String>
  pub fn discard_all_unstaged(&self) -> Result<(), String>
  pub fn ignore_path(&self, pattern: String) -> Result<(), String>
  ```
  `Command` gains `opts: DiffOptions` on `GetWorkingDiff`/`GetCommitDiff`, `context_lines: u32` on the three hunk variants, and new variants `StageLines`, `UnstageLines`, `DiscardLines` (`path, old_start, new_start, context_lines, line_indices: Vec<u32>, reply`), `DiscardFile { path, reply }`, `DiscardAllUnstaged { reply }`, `IgnorePath { pattern, reply }`.

- [ ] **Step 1: Write the failing tests** — update the three existing hunk tests in `worker/mod.rs`'s test module (`get_working_diff(.., false)` → add `git_core::diff::DiffOptions::default()`; `stage_hunk(.., o, n)` → add `3`; same for unstage/discard), then append:

```rust
    #[test]
    fn line_and_file_commands_round_trip_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "f.txt", "a\nb\nc\n");
        commit_all(&repo, "base");
        write_file(dir.path(), "f.txt", "a\nB\nc\nd\n");
        write_file(dir.path(), "junk.log", "x\n");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        let opts = git_core::diff::DiffOptions::default();
        let hunk = handle.get_working_diff("f.txt".into(), false, opts).unwrap().remove(0);

        handle.stage_lines("f.txt".into(), hunk.old_start, hunk.new_start, 3, vec![4]).unwrap();
        let staged = handle.get_working_diff("f.txt".into(), true, opts).unwrap().remove(0);
        handle.unstage_lines("f.txt".into(), staged.old_start, staged.new_start, 3, vec![staged.lines.len() as u32 - 1]).unwrap();
        assert!(handle.get_working_diff("f.txt".into(), true, opts).unwrap().is_empty());

        let hunk = handle.get_working_diff("f.txt".into(), false, opts).unwrap().remove(0);
        handle.discard_lines("f.txt".into(), hunk.old_start, hunk.new_start, 3, vec![4]).unwrap();
        assert_eq!(std::fs::read_to_string(dir.path().join("f.txt")).unwrap().replace("\r\n", "\n"), "a\nB\nc\n");

        handle.ignore_path("*.log".into()).unwrap();
        handle.discard_file("f.txt".into()).unwrap();
        handle.discard_all_unstaged().unwrap();
        let status = handle.get_status().unwrap();
        assert!(status.is_empty() || status.iter().all(|e| e.path == ".gitignore"), "{status:?}");

        assert!(handle.stage_lines("f.txt".into(), 99, 99, 3, vec![0]).is_err());
        assert!(handle.get_status().is_ok(), "worker survives a failed command");
    }
```

(Note `discard_all_unstaged` deletes the untracked `.gitignore` too — that's expected; the assertion tolerates both orders.)

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p repo-service line_and_file_commands`
Expected: compile errors.

- [ ] **Step 3: Implement** — follow the exact existing pattern (`status.rs` handler fn sending `git_core::…(…).map_err(|e| e.to_string())` on `reply`; `WorkerHandle` method creating a channel, sending the `Command`, `rx.recv()` with the same two error strings; dispatch arm in `mod.rs` destructuring the variant). Replace the Task 1 temporary defaults with the passed `opts`/`context_lines`. For the line variants pass `&line_indices`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p repo-service && cargo build --workspace`
Expected: repo-service PASS. `tauri-app`/`vscode-sidecar` fail to compile (arity) — fixed in Task 6; that's acceptable within this task only if you commit after Task 6. Otherwise make the two call sites pass `DiffOptions::default()`/`3` temporarily so the workspace builds before committing.

- [ ] **Step 5: Commit**

```bash
git add crates/repo-service crates/tauri-app crates/vscode-sidecar
git commit -m "feat(repo-service): worker commands for line staging, discard and ignore"
```

---

### Task 6: Tauri commands and sidecar methods

**Files:**
- Modify: `crates/tauri-app/src/commands/status.rs`, `crates/tauri-app/src/commands/mod.rs` (DTOs, re-exports, serialization tests, `get/set_diff_settings`), `crates/tauri-app/src/main.rs` (register), `crates/vscode-sidecar/src/dispatch.rs`, `crates/vscode-sidecar/tests/protocol_roundtrip.rs`

**Interfaces:**
- Wire names (snake_case commands; camelCase params; `repoPath` first):
  - changed: `get_working_diff {repoPath, path, staged, opts}`, `get_commit_diff {repoPath, commitId, path, opts}`, `stage_hunk|unstage_hunk|discard_hunk {repoPath, path, oldStart, newStart, contextLines}`
  - new: `stage_lines|unstage_lines|discard_lines {repoPath, path, oldStart, newStart, contextLines, lineIndices}`, `discard_file {repoPath, path}`, `discard_all_unstaged {repoPath}`, `ignore_path {repoPath, pattern}`, `get_diff_settings {}`, `set_diff_settings {settings}`
- DTOs (identical in both crates):
  ```rust
  #[derive(Deserialize, Clone, Copy)] #[serde(rename_all = "camelCase")]
  pub struct DiffOptionsDto { pub ignore_whitespace: bool, pub context_lines: u32 }
  impl From<DiffOptionsDto> for git_core::diff::DiffOptions { … }
  #[derive(Serialize, Deserialize)] #[serde(rename_all = "camelCase")]
  pub struct DiffSettingsDto { pub ignore_whitespace: bool, pub context_lines: u32, pub syntax_highlight: bool }
  impl From<config::DiffSettings> for DiffSettingsDto / impl From<DiffSettingsDto> for config::DiffSettings
  ```
  JSON: `{"ignoreWhitespace":false,"contextLines":3}`, `{"ignoreWhitespace":…,"contextLines":…,"syntaxHighlight":…}`.
- `get_diff_settings`/`set_diff_settings` call `config::` directly (like `get_last_seen_version`), not the worker. They are **not** in `extension/src/sidecarBridge.ts`'s `NATIVE_METHODS`, so they reach the sidecar.

- [ ] **Step 1: Write the failing tests**

`tauri-app/src/commands/mod.rs` test module:

```rust
    #[test]
    fn diff_options_dto_parses_camel_case() {
        let dto: DiffOptionsDto =
            serde_json::from_value(serde_json::json!({"ignoreWhitespace": true, "contextLines": 5})).unwrap();
        assert_eq!(
            git_core::diff::DiffOptions::from(dto),
            git_core::diff::DiffOptions { ignore_whitespace: true, context_lines: 5 }
        );
    }

    #[test]
    fn diff_settings_dto_round_trips_camel_case() {
        let dto = DiffSettingsDto::from(config::DiffSettings { ignore_whitespace: false, context_lines: 3, syntax_highlight: true });
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(json, serde_json::json!({"ignoreWhitespace": false, "contextLines": 3, "syntaxHighlight": true}));
        let back: DiffSettingsDto = serde_json::from_value(json).unwrap();
        assert_eq!(config::DiffSettings::from(back).context_lines, 3);
    }
```

(If `config::DiffSettings` lacks `PartialEq` use, it has it from Task 4.)

`vscode-sidecar/tests/protocol_roundtrip.rs`: update `working_and_commit_diff_round_trip_through_the_sidecar` and `stage_unstage_and_discard_hunk_round_trip_through_the_sidecar` params (`"opts": {"ignoreWhitespace": false, "contextLines": 3}`, `"contextLines": 3`), then add:

```rust
#[test]
fn line_staging_discard_and_ignore_round_trip_through_the_sidecar() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "f.txt", "a\nb\nc\n");
    commit_all(&repo, "base");
    write_file(dir.path(), "f.txt", "a\nB\nc\nd\n");
    write_file(dir.path(), "junk.log", "x\n");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let diff = sidecar.call(2, "get_working_diff", serde_json::json!({
        "repoPath": repo_path, "path": "f.txt", "staged": false,
        "opts": {"ignoreWhitespace": false, "contextLines": 3}
    }));
    let hunk = &diff["result"][0];
    let staged = sidecar.call(3, "stage_lines", serde_json::json!({
        "repoPath": repo_path, "path": "f.txt",
        "oldStart": hunk["oldStart"], "newStart": hunk["newStart"],
        "contextLines": 3, "lineIndices": [4]
    }));
    assert_eq!(staged["result"], serde_json::Value::Null, "{staged}");

    let ignored = sidecar.call(4, "ignore_path", serde_json::json!({"repoPath": repo_path, "pattern": "*.log"}));
    assert_eq!(ignored["result"], serde_json::Value::Null);
    assert!(std::fs::read_to_string(dir.path().join(".gitignore")).unwrap().contains("*.log"));

    let discarded = sidecar.call(5, "discard_file", serde_json::json!({"repoPath": repo_path, "path": "f.txt"}));
    assert_eq!(discarded["result"], serde_json::Value::Null);
    let all = sidecar.call(6, "discard_all_unstaged", serde_json::json!({"repoPath": repo_path}));
    assert_eq!(all["result"], serde_json::Value::Null);
}

#[test]
fn diff_settings_round_trip_through_the_sidecar() {
    // The sidecar reads `BROWSITORY_CONFIG_DIR` (see `config_file_path`); check how other
    // config round-trip tests in this file isolate it (e.g. the workspaces test) and copy that.
    let mut sidecar = Sidecar::spawn();
    let set = sidecar.call(1, "set_diff_settings", serde_json::json!({
        "settings": {"ignoreWhitespace": true, "contextLines": 6, "syntaxHighlight": false}
    }));
    assert_eq!(set["result"], serde_json::Value::Null, "{set}");
    let got = sidecar.call(2, "get_diff_settings", serde_json::json!({}));
    assert_eq!(got["result"], serde_json::json!({"ignoreWhitespace": true, "contextLines": 6, "syntaxHighlight": false}));
}
```

If `Sidecar::spawn` doesn't isolate the config dir, add a `Sidecar::spawn_with_env(&[("BROWSITORY_CONFIG_DIR", tmp)])` helper mirroring `spawn` (read it first).

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p tauri-app diff_ && cargo test -p vscode-sidecar --test protocol_roundtrip`
Expected: FAIL.

- [ ] **Step 3: Implement**

Tauri (`commands/status.rs`), e.g.:

```rust
#[tauri::command]
pub async fn get_working_diff(
    repo_path: String,
    path: String,
    staged: bool,
    opts: DiffOptionsDto,
    state: State<'_, AppState>,
) -> Result<Vec<DiffHunkDto>, String> {
    Ok(worker_handle(&state, &repo_path)?
        .get_working_diff(path, staged, opts.into())?
        .into_iter()
        .map(DiffHunkDto::from)
        .collect())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn stage_lines(
    repo_path: String,
    path: String,
    old_start: u32,
    new_start: u32,
    context_lines: u32,
    line_indices: Vec<u32>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.stage_lines(path, old_start, new_start, context_lines, line_indices)
}
```

Same shape for `get_commit_diff`, the three hunk commands (+`context_lines`), `unstage_lines`, `discard_lines`, `discard_file`, `discard_all_unstaged`, `ignore_path`. In `commands/mod.rs`, next to `get_last_seen_version`:

```rust
#[tauri::command]
pub fn get_diff_settings() -> Result<DiffSettingsDto, String> {
    config::get_diff_settings().map(DiffSettingsDto::from).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_diff_settings(settings: DiffSettingsDto) -> Result<(), String> {
    config::set_diff_settings(&settings.into()).map_err(|e| e.to_string())
}
```

Register all new commands in `main.rs`'s `use` list and `generate_handler!`.

Sidecar (`dispatch.rs`): add `opts: DiffOptionsDto` to `GetWorkingDiffParams`/`GetCommitDiffParams`; add `context_lines: u32` to `HunkParams`; new `LinesParams { repo_path, path, old_start, new_start, context_lines, line_indices: Vec<u32> }`, `IgnorePathParams { repo_path, pattern }`, `SetDiffSettingsParams { settings: DiffSettingsDto }`; reuse `RepoFilePathParams` for `discard_file` and `RepoPathParams` for `discard_all_unstaged`. Add match arms in the method table next to `discard_hunk`; `get_diff_settings`/`set_diff_settings` arms next to the graph-selection ones (no `repos` arg).

- [ ] **Step 4: Run tests**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates
git commit -m "feat(tauri,sidecar): expose line staging, discard, ignore and diff settings"
```

---

### Task 7: `RepoClient` surface

**Files:**
- Modify: `frontend/src/ipc/RepoClient.ts`, `frontend/src/ipc/tauriRepoClient.ts`, `frontend/src/ipc/vscodeRepoClient.ts`, `frontend/src/ipc/tauriRepoClient.test.ts`, `frontend/src/ipc/vscodeRepoClient.test.ts`, every test file with a full `RepoClient` literal (find them with `pnpm exec tsc --noEmit` after Step 3)
- Modify (callers, to keep compiling): `frontend/src/components/DiffPane.tsx` (pass `DEFAULT_DIFF_OPTIONS` to `getWorkingDiff`/`getCommitDiff` for now), `frontend/src/state/useStagingActions.ts` (pass `3` for now)

**Interfaces:**
- Produces (in `RepoClient.ts`):
  ```ts
  export interface DiffOptions { ignoreWhitespace: boolean; contextLines: number }
  export interface DiffSettings extends DiffOptions { syntaxHighlight: boolean }
  export const DEFAULT_DIFF_SETTINGS: DiffSettings = { ignoreWhitespace: false, contextLines: 3, syntaxHighlight: true };
  export const DEFAULT_DIFF_OPTIONS: DiffOptions = { ignoreWhitespace: false, contextLines: 3 };
  export const MAX_CONTEXT_LINES = 20;
  // RepoClient changes:
  getWorkingDiff(repoPath: string, path: string, staged: boolean, opts: DiffOptions): Promise<DiffHunk[]>;
  getCommitDiff(repoPath: string, commitId: string, path: string, opts: DiffOptions): Promise<DiffHunk[]>;
  stageHunk(repoPath: string, path: string, oldStart: number, newStart: number, contextLines: number): Promise<void>;
  unstageHunk(…same…): Promise<void>;
  discardHunk(…same…): Promise<void>;
  stageLines(repoPath: string, path: string, oldStart: number, newStart: number, contextLines: number, lineIndices: number[]): Promise<void>;
  unstageLines(…same…): Promise<void>;
  discardLines(…same…): Promise<void>;
  discardFile(repoPath: string, path: string): Promise<void>;
  discardAllUnstaged(repoPath: string): Promise<void>;
  ignorePath(repoPath: string, pattern: string): Promise<void>;
  getDiffSettings(): Promise<DiffSettings>;
  setDiffSettings(settings: DiffSettings): Promise<void>;
  ```

- [ ] **Step 1: Write the failing tests** — `tauriRepoClient.test.ts` (match the file's mock name for `invoke`; read its top first):

```ts
  it("invokes the diff-pane commands with camelCase args", async () => {
    const opts = { ignoreWhitespace: true, contextLines: 5 };
    await client.getWorkingDiff("/repo", "f.txt", false, opts);
    expect(invoke).toHaveBeenLastCalledWith("get_working_diff", { repoPath: "/repo", path: "f.txt", staged: false, opts });
    await client.getCommitDiff("/repo", "abc", "f.txt", opts);
    expect(invoke).toHaveBeenLastCalledWith("get_commit_diff", { repoPath: "/repo", commitId: "abc", path: "f.txt", opts });
    await client.stageHunk("/repo", "f.txt", 1, 2, 5);
    expect(invoke).toHaveBeenLastCalledWith("stage_hunk", { repoPath: "/repo", path: "f.txt", oldStart: 1, newStart: 2, contextLines: 5 });
    await client.stageLines("/repo", "f.txt", 1, 2, 5, [3, 4]);
    expect(invoke).toHaveBeenLastCalledWith("stage_lines", { repoPath: "/repo", path: "f.txt", oldStart: 1, newStart: 2, contextLines: 5, lineIndices: [3, 4] });
    await client.unstageLines("/repo", "f.txt", 1, 2, 5, [3]);
    expect(invoke).toHaveBeenLastCalledWith("unstage_lines", { repoPath: "/repo", path: "f.txt", oldStart: 1, newStart: 2, contextLines: 5, lineIndices: [3] });
    await client.discardLines("/repo", "f.txt", 1, 2, 5, [3]);
    expect(invoke).toHaveBeenLastCalledWith("discard_lines", { repoPath: "/repo", path: "f.txt", oldStart: 1, newStart: 2, contextLines: 5, lineIndices: [3] });
    await client.discardFile("/repo", "f.txt");
    expect(invoke).toHaveBeenLastCalledWith("discard_file", { repoPath: "/repo", path: "f.txt" });
    await client.discardAllUnstaged("/repo");
    expect(invoke).toHaveBeenLastCalledWith("discard_all_unstaged", { repoPath: "/repo" });
    await client.ignorePath("/repo", "*.log");
    expect(invoke).toHaveBeenLastCalledWith("ignore_path", { repoPath: "/repo", pattern: "*.log" });
    await client.getDiffSettings();
    expect(invoke).toHaveBeenLastCalledWith("get_diff_settings", undefined);
    const settings = { ignoreWhitespace: false, contextLines: 3, syntaxHighlight: true };
    await client.setDiffSettings(settings);
    expect(invoke).toHaveBeenLastCalledWith("set_diff_settings", { settings });
  });
```

(Check how `getLastSeenVersion`'s no-arg call is asserted in the existing test — mirror it for `get_diff_settings`.) Add the equivalent to `vscodeRepoClient.test.ts`, asserting posted `method`/`params` per its existing pattern (`get_diff_settings` with `{}`).

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/ipc`
Expected: FAIL.

- [ ] **Step 3: Implement**

`tauriRepoClient.ts` (replace the existing diff/hunk entries, add the rest next to them):

```ts
  getWorkingDiff: (repoPath: string, path: string, staged: boolean, opts: DiffOptions) =>
    loggedInvoke<DiffHunk[]>("get_working_diff", { repoPath, path, staged, opts }),
  getCommitDiff: (repoPath: string, commitId: string, path: string, opts: DiffOptions) =>
    loggedInvoke<DiffHunk[]>("get_commit_diff", { repoPath, commitId, path, opts }),
  stageHunk: (repoPath: string, path: string, oldStart: number, newStart: number, contextLines: number) =>
    loggedInvoke("stage_hunk", { repoPath, path, oldStart, newStart, contextLines }),
  unstageHunk: (repoPath: string, path: string, oldStart: number, newStart: number, contextLines: number) =>
    loggedInvoke("unstage_hunk", { repoPath, path, oldStart, newStart, contextLines }),
  discardHunk: (repoPath: string, path: string, oldStart: number, newStart: number, contextLines: number) =>
    loggedInvoke("discard_hunk", { repoPath, path, oldStart, newStart, contextLines }),
  stageLines: (repoPath: string, path: string, oldStart: number, newStart: number, contextLines: number, lineIndices: number[]) =>
    loggedInvoke("stage_lines", { repoPath, path, oldStart, newStart, contextLines, lineIndices }),
  unstageLines: (repoPath: string, path: string, oldStart: number, newStart: number, contextLines: number, lineIndices: number[]) =>
    loggedInvoke("unstage_lines", { repoPath, path, oldStart, newStart, contextLines, lineIndices }),
  discardLines: (repoPath: string, path: string, oldStart: number, newStart: number, contextLines: number, lineIndices: number[]) =>
    loggedInvoke("discard_lines", { repoPath, path, oldStart, newStart, contextLines, lineIndices }),
  discardFile: (repoPath: string, path: string) => loggedInvoke("discard_file", { repoPath, path }),
  discardAllUnstaged: (repoPath: string) => loggedInvoke("discard_all_unstaged", { repoPath }),
  ignorePath: (repoPath: string, pattern: string) => loggedInvoke("ignore_path", { repoPath, pattern }),
  getDiffSettings: () => loggedInvoke<DiffSettings>("get_diff_settings"),
  setDiffSettings: (settings: DiffSettings) => loggedInvoke("set_diff_settings", { settings }),
```

`vscodeRepoClient.ts`: same with `call<…>(method, params)` (`get_diff_settings` → `call<DiffSettings>("get_diff_settings", {})`).

Temporary caller updates so the app compiles: `DiffPane.tsx` passes `DEFAULT_DIFF_OPTIONS` to both diff fetches; `useStagingActions.ts` passes `3` as `contextLines` (Task 8 replaces both).

Stubs for every full `RepoClient` literal in tests (match each file's convention):

```ts
    stageLines: async () => {},
    unstageLines: async () => {},
    discardLines: async () => {},
    discardFile: async () => {},
    discardAllUnstaged: async () => {},
    ignorePath: async () => {},
    getDiffSettings: async () => ({ ignoreWhitespace: false, contextLines: 3, syntaxHighlight: true }),
    setDiffSettings: async () => {},
```

- [ ] **Step 4: Run tests + typecheck + lint**

Run: `cd frontend && pnpm exec tsc --noEmit && pnpm lint && pnpm test -- --run`
Expected: all green.

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): RepoClient methods for line staging, discard, ignore and diff settings"
```

---

### Task 8: Diff settings state + staging actions

**Files:**
- Create: `frontend/src/state/useDiffSettings.tsx`, `frontend/src/state/useDiffSettings.test.tsx`
- Modify: `frontend/src/state/useStagingActions.ts` (+ its test if one exists, else `useAppState.test.ts`), `frontend/src/state/useAppState.ts` (expose new actions on `UseAppStateResult`), `frontend/src/App.tsx` (wrap the root in `DiffSettingsProvider`)

**Interfaces:**
- Consumes: Task 7 client methods, `DEFAULT_DIFF_SETTINGS`, `MAX_CONTEXT_LINES`.
- Produces:
  ```ts
  // useDiffSettings.tsx
  export interface DiffSettingsApi {
    settings: DiffSettings;
    update(partial: Partial<DiffSettings>): void; // optimistic; persists; reverts on failure
    error: string | null;
    dismissError(): void;
  }
  export function useDiffSettings(client: RepoClient): DiffSettingsApi;
  export function DiffSettingsProvider(props: { client: RepoClient; children: ReactNode }): JSX.Element;
  export function useDiffSettingsContext(): DiffSettingsApi; // defaults + no-op update without a provider
  // StagingActions / UseAppStateResult changes:
  stageHunk(path: string, oldStart: number, newStart: number, contextLines: number): Promise<void>;
  unstageHunk(…same…): Promise<void>;
  discardHunk(…same…): Promise<void>;
  stageLines(path: string, oldStart: number, newStart: number, contextLines: number, lineIndices: number[]): Promise<void>;
  unstageLines(…same…): Promise<void>;
  discardLines(…same…): Promise<void>;
  discardFile(path: string): Promise<void>;
  discardAllUnstaged(): Promise<void>;
  ignorePath(pattern: string): Promise<void>;
  ```

- [ ] **Step 1: Write the failing tests** — `useDiffSettings.test.tsx`:

```tsx
import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RepoClient } from "../ipc/RepoClient";
import { useDiffSettings, useDiffSettingsContext } from "./useDiffSettings";

function clientWith(overrides: Partial<RepoClient>): RepoClient {
  return {
    getDiffSettings: vi.fn(async () => ({ ignoreWhitespace: false, contextLines: 3, syntaxHighlight: true })),
    setDiffSettings: vi.fn(async () => {}),
    ...overrides,
  } as unknown as RepoClient; // only the two settings methods are used by this hook
}

describe("useDiffSettings", () => {
  it("loads persisted settings", async () => {
    const client = clientWith({ getDiffSettings: vi.fn(async () => ({ ignoreWhitespace: true, contextLines: 7, syntaxHighlight: false })) });
    const { result } = renderHook(() => useDiffSettings(client));
    await waitFor(() => expect(result.current.settings.contextLines).toBe(7));
    expect(result.current.settings.ignoreWhitespace).toBe(true);
  });

  it("updates optimistically and persists the merged value", async () => {
    const client = clientWith({});
    const { result } = renderHook(() => useDiffSettings(client));
    await waitFor(() => expect(client.getDiffSettings).toHaveBeenCalled());
    act(() => result.current.update({ contextLines: 5 }));
    expect(result.current.settings.contextLines).toBe(5);
    await waitFor(() =>
      expect(client.setDiffSettings).toHaveBeenCalledWith({ ignoreWhitespace: false, contextLines: 5, syntaxHighlight: true }),
    );
  });

  it("clamps context lines to 0..20", async () => {
    const client = clientWith({});
    const { result } = renderHook(() => useDiffSettings(client));
    act(() => result.current.update({ contextLines: 50 }));
    expect(result.current.settings.contextLines).toBe(20);
    act(() => result.current.update({ contextLines: -1 }));
    expect(result.current.settings.contextLines).toBe(0);
  });

  it("reverts and reports when persisting fails", async () => {
    const client = clientWith({ setDiffSettings: vi.fn(async () => { throw new Error("disk full"); }) });
    const { result } = renderHook(() => useDiffSettings(client));
    await waitFor(() => expect(client.getDiffSettings).toHaveBeenCalled());
    act(() => result.current.update({ ignoreWhitespace: true }));
    await waitFor(() => expect(result.current.error).toContain("disk full"));
    expect(result.current.settings.ignoreWhitespace).toBe(false);
  });

  it("falls back to defaults without a provider", () => {
    const { result } = renderHook(() => useDiffSettingsContext());
    expect(result.current.settings).toEqual({ ignoreWhitespace: false, contextLines: 3, syntaxHighlight: true });
  });
});
```

Staging-action tests (add to the existing `useStagingActions` test file if present, else to `useAppState.test.ts` following its harness): assert `stageLines("f.txt", 1, 2, 5, [3])` calls `client.stageLines("/repo", "f.txt", 1, 2, 5, [3])`; `discardFile`, `discardAllUnstaged`, `ignorePath` call through with `repoPath`; `stageHunk(…, 5)` passes `5`.

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/state`
Expected: FAIL.

- [ ] **Step 3: Implement** — `useDiffSettings.tsx`:

```tsx
import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import { DEFAULT_DIFF_SETTINGS, MAX_CONTEXT_LINES, type DiffSettings, type RepoClient } from "../ipc/RepoClient";

export interface DiffSettingsApi {
  settings: DiffSettings;
  update(partial: Partial<DiffSettings>): void;
  error: string | null;
  dismissError(): void;
}

function clamp(settings: DiffSettings): DiffSettings {
  const contextLines = Math.min(Math.max(Math.round(settings.contextLines), 0), MAX_CONTEXT_LINES);
  return { ...settings, contextLines };
}

export function useDiffSettings(client: RepoClient): DiffSettingsApi {
  const [settings, setSettings] = useState<DiffSettings>(DEFAULT_DIFF_SETTINGS);
  const [error, setError] = useState<string | null>(null);
  const latest = useRef(settings);
  latest.current = settings;

  useEffect(() => {
    let cancelled = false;
    client.getDiffSettings().then(
      (loaded) => !cancelled && setSettings(clamp(loaded)),
      () => {}, // keep defaults; a missing/broken config must not block the diff view
    );
    return () => {
      cancelled = true;
    };
  }, [client]);

  const update = useCallback(
    (partial: Partial<DiffSettings>) => {
      const previous = latest.current;
      const next = clamp({ ...previous, ...partial });
      latest.current = next;
      setSettings(next);
      client.setDiffSettings(next).catch((err: unknown) => {
        setSettings(previous);
        latest.current = previous;
        setError(err instanceof Error ? err.message : String(err));
      });
    },
    [client],
  );

  return { settings, update, error, dismissError: () => setError(null) };
}

const fallback: DiffSettingsApi = {
  settings: DEFAULT_DIFF_SETTINGS,
  update: () => {},
  error: null,
  dismissError: () => {},
};

const DiffSettingsContext = createContext<DiffSettingsApi>(fallback);

export function DiffSettingsProvider({ client, children }: { client: RepoClient; children: ReactNode }) {
  const api = useDiffSettings(client);
  return <DiffSettingsContext.Provider value={api}>{children}</DiffSettingsContext.Provider>;
}

export function useDiffSettingsContext(): DiffSettingsApi {
  return useContext(DiffSettingsContext);
}
```

(If the repo's ESLint `react-refresh/only-export-components` rule objects to mixing hooks and components in one file, split the provider into `DiffSettingsProvider.tsx`.)

`useStagingActions.ts`: add `contextLines` to the three hunk actions and add:

```ts
  const stageLines = useCallback(
    (path: string, oldStart: number, newStart: number, contextLines: number, lineIndices: number[]) =>
      runMutation(() => client.stageLines(repoPath, path, oldStart, newStart, contextLines, lineIndices)),
    [client, runMutation, repoPath],
  );
  // unstageLines / discardLines: identical shape, calling client.unstageLines / client.discardLines.
  const discardFile = useCallback(
    (path: string) => runMutation(() => client.discardFile(repoPath, path)),
    [client, runMutation, repoPath],
  );
  const discardAllUnstaged = useCallback(
    () => runMutation(() => client.discardAllUnstaged(repoPath)),
    [client, runMutation, repoPath],
  );
  const ignorePath = useCallback(
    (pattern: string) => runMutation(() => client.ignorePath(repoPath, pattern)),
    [client, runMutation, repoPath],
  );
```

Write `unstageLines` and `discardLines` out in full (same body, different client method). Extend the `StagingActions` interface, the return object, and `UseAppStateResult` + its destructure/return in `useAppState.ts`.

`App.tsx`: inside `export default function App`, wrap the returned tree in `<DiffSettingsProvider client={client}>…</DiffSettingsProvider>` (use whatever `client` variable `App` already holds).

DiffPane still compiles because its hunk props are `(path, oldStart, newStart) => …`; update `UncommittedFileSection` to pass `settings.contextLines` from `useDiffSettingsContext()` into `onStageHunk/onUnstageHunk/onDiscardHunk`, widening those prop types to `(path, oldStart, newStart, contextLines)` through `DiffPane` → `UncommittedDiffPane` → section, and `wrapHunkAction` accordingly. Replace Task 7's temporary `DEFAULT_DIFF_OPTIONS` in both diff fetches with `{ ignoreWhitespace: settings.ignoreWhitespace, contextLines: settings.contextLines }` and add both to the fetch effects' dependency arrays, so changing a setting refetches.

- [ ] **Step 4: Run tests + typecheck + lint**

Run: `cd frontend && pnpm exec tsc --noEmit && pnpm lint && pnpm test -- --run`
Expected: all green (fix `DiffPane.test.tsx` expectations that assert `stageHunk` call args: they now include `3`).

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): global diff settings state and line/file staging actions"
```

---

### Task 9: Line selection reducer

**Files:**
- Create: `frontend/src/lib/lineSelection.ts`, `frontend/src/lib/lineSelection.test.ts`

**Interfaces:**
- Produces:
  ```ts
  export interface LineSelection {
    hunkIndex: number | null;
    anchor: number | null;   // line index the range extends from
    cursor: number | null;   // keyboard cursor line index
    selected: ReadonlySet<number>;
  }
  export const EMPTY_SELECTION: LineSelection;
  export type SelectionAction =
    | { type: "click"; hunkIndex: number; line: number }
    | { type: "shiftClick"; hunkIndex: number; line: number }
    | { type: "toggle"; hunkIndex: number; line: number }
    | { type: "dragTo"; hunkIndex: number; line: number }
    | { type: "moveCursor"; delta: 1 | -1 }
    | { type: "extend"; delta: 1 | -1 }
    | { type: "focusHunk"; hunkIndex: number }
    | { type: "clear" };
  export function reduceSelection(state: LineSelection, action: SelectionAction, hunks: readonly DiffHunk[]): LineSelection;
  export function isSelectable(hunks: readonly DiffHunk[], hunkIndex: number, line: number): boolean; // Add/Remove and not an EOF marker
  ```
- EOF-marker lines arrive as `Context` with content starting `"\n\\ No newline"` or `"\\ No newline"` — never selectable. Context lines never selectable.

- [ ] **Step 1: Write the failing tests**

```ts
import { describe, expect, it } from "vitest";
import type { DiffHunk } from "../ipc/RepoClient";
import { EMPTY_SELECTION, reduceSelection, type LineSelection, type SelectionAction } from "./lineSelection";

const hunk = (origins: Array<"Add" | "Remove" | "Context">): DiffHunk => ({
  oldStart: 1, oldLines: 0, newStart: 1, newLines: 0,
  lines: origins.map((origin, i) => ({ origin, content: `l${i}\n` })),
});
// indices: 0 ctx, 1 rem, 2 add, 3 ctx, 4 add
const hunks = [hunk(["Context", "Remove", "Add", "Context", "Add"]), hunk(["Add", "Add"])];
const run = (actions: SelectionAction[], start: LineSelection = EMPTY_SELECTION) =>
  actions.reduce((s, a) => reduceSelection(s, a, hunks), start);
const sel = (s: LineSelection) => [...s.selected].sort((a, b) => a - b);

describe("reduceSelection", () => {
  it("click selects one changed line and ignores context", () => {
    expect(sel(run([{ type: "click", hunkIndex: 0, line: 2 }]))).toEqual([2]);
    expect(sel(run([{ type: "click", hunkIndex: 0, line: 0 }]))).toEqual([]);
  });

  it("shift-click selects the changed lines in the range", () => {
    const s = run([{ type: "click", hunkIndex: 0, line: 1 }, { type: "shiftClick", hunkIndex: 0, line: 4 }]);
    expect(sel(s)).toEqual([1, 2, 4]);
  });

  it("toggle adds and removes single lines", () => {
    const s = run([
      { type: "click", hunkIndex: 0, line: 1 },
      { type: "toggle", hunkIndex: 0, line: 4 },
      { type: "toggle", hunkIndex: 0, line: 1 },
    ]);
    expect(sel(s)).toEqual([4]);
  });

  it("selecting in another hunk replaces the selection", () => {
    const s = run([{ type: "click", hunkIndex: 0, line: 1 }, { type: "toggle", hunkIndex: 1, line: 0 }]);
    expect(s.hunkIndex).toBe(1);
    expect(sel(s)).toEqual([0]);
  });

  it("dragTo behaves like shift-click from the anchor", () => {
    const s = run([{ type: "click", hunkIndex: 0, line: 4 }, { type: "dragTo", hunkIndex: 0, line: 2 }]);
    expect(sel(s)).toEqual([2, 4]);
  });

  it("keyboard cursor skips context lines and extend grows the selection", () => {
    let s = run([{ type: "focusHunk", hunkIndex: 0 }, { type: "moveCursor", delta: 1 }]);
    expect(s.cursor).toBe(1);
    s = run([{ type: "extend", delta: 1 }, { type: "extend", delta: 1 }], s);
    expect(sel(s)).toEqual([1, 2, 4]);
    expect(s.cursor).toBe(4);
  });

  it("moveCursor clears the selection and stops at the ends", () => {
    let s = run([{ type: "click", hunkIndex: 0, line: 4 }, { type: "moveCursor", delta: 1 }]);
    expect(s.cursor).toBe(4);
    expect(sel(s)).toEqual([]);
    s = run([{ type: "moveCursor", delta: -1 }, { type: "moveCursor", delta: -1 }, { type: "moveCursor", delta: -1 }], s);
    expect(s.cursor).toBe(1);
  });

  it("never selects an EOF marker", () => {
    const eof: DiffHunk[] = [{ oldStart: 1, oldLines: 1, newStart: 1, newLines: 1, lines: [
      { origin: "Add", content: "x" },
      { origin: "Context", content: "\n\\ No newline at end of file\n" },
    ] }];
    const s = reduceSelection(EMPTY_SELECTION, { type: "shiftClick", hunkIndex: 0, line: 1 }, eof);
    expect([...s.selected]).toEqual([]);
  });

  it("clear resets everything", () => {
    expect(run([{ type: "click", hunkIndex: 0, line: 1 }, { type: "clear" }])).toEqual(EMPTY_SELECTION);
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/lib/lineSelection`
Expected: FAIL (module missing).

- [ ] **Step 3: Implement**

```ts
import type { DiffHunk } from "../ipc/RepoClient";

export interface LineSelection {
  hunkIndex: number | null;
  anchor: number | null;
  cursor: number | null;
  selected: ReadonlySet<number>;
}

export const EMPTY_SELECTION: LineSelection = { hunkIndex: null, anchor: null, cursor: null, selected: new Set() };

export type SelectionAction =
  | { type: "click"; hunkIndex: number; line: number }
  | { type: "shiftClick"; hunkIndex: number; line: number }
  | { type: "toggle"; hunkIndex: number; line: number }
  | { type: "dragTo"; hunkIndex: number; line: number }
  | { type: "moveCursor"; delta: 1 | -1 }
  | { type: "extend"; delta: 1 | -1 }
  | { type: "focusHunk"; hunkIndex: number }
  | { type: "clear" };

export function isSelectable(hunks: readonly DiffHunk[], hunkIndex: number, line: number): boolean {
  const l = hunks[hunkIndex]?.lines[line];
  return l !== undefined && (l.origin === "Add" || l.origin === "Remove");
}

function range(hunks: readonly DiffHunk[], hunkIndex: number, from: number, to: number): Set<number> {
  const [lo, hi] = from <= to ? [from, to] : [to, from];
  const out = new Set<number>();
  for (let i = lo; i <= hi; i++) if (isSelectable(hunks, hunkIndex, i)) out.add(i);
  return out;
}

function step(hunks: readonly DiffHunk[], hunkIndex: number, from: number | null, delta: 1 | -1): number | null {
  const lines = hunks[hunkIndex]?.lines ?? [];
  let i = from === null ? (delta === 1 ? -1 : lines.length) : from;
  for (i += delta; i >= 0 && i < lines.length; i += delta) {
    if (isSelectable(hunks, hunkIndex, i)) return i;
  }
  return from;
}

export function reduceSelection(state: LineSelection, action: SelectionAction, hunks: readonly DiffHunk[]): LineSelection {
  switch (action.type) {
    case "clear":
      return EMPTY_SELECTION;
    case "focusHunk":
      return { ...EMPTY_SELECTION, hunkIndex: action.hunkIndex };
    case "click": {
      if (!isSelectable(hunks, action.hunkIndex, action.line)) return { ...EMPTY_SELECTION, hunkIndex: action.hunkIndex };
      return { hunkIndex: action.hunkIndex, anchor: action.line, cursor: action.line, selected: new Set([action.line]) };
    }
    case "shiftClick":
    case "dragTo": {
      const sameHunk = state.hunkIndex === action.hunkIndex && state.anchor !== null;
      const anchor = sameHunk ? state.anchor! : action.line;
      return { hunkIndex: action.hunkIndex, anchor, cursor: action.line, selected: range(hunks, action.hunkIndex, anchor, action.line) };
    }
    case "toggle": {
      if (!isSelectable(hunks, action.hunkIndex, action.line)) return state;
      const base = state.hunkIndex === action.hunkIndex ? new Set(state.selected) : new Set<number>();
      if (base.has(action.line)) base.delete(action.line);
      else base.add(action.line);
      return { hunkIndex: action.hunkIndex, anchor: action.line, cursor: action.line, selected: base };
    }
    case "moveCursor": {
      if (state.hunkIndex === null) return state;
      const cursor = step(hunks, state.hunkIndex, state.cursor, action.delta);
      return { hunkIndex: state.hunkIndex, anchor: cursor, cursor, selected: new Set() };
    }
    case "extend": {
      if (state.hunkIndex === null) return state;
      const start = state.cursor ?? step(hunks, state.hunkIndex, null, action.delta);
      if (start === null) return state;
      const anchor = state.anchor ?? start;
      const cursor = state.cursor === null ? start : step(hunks, state.hunkIndex, state.cursor, action.delta);
      return { hunkIndex: state.hunkIndex, anchor, cursor, selected: range(hunks, state.hunkIndex, anchor, cursor ?? anchor) };
    }
  }
}
```

The keyboard test expects `focusHunk` + `moveCursor(+1)` to land on index 1 and two `extend(+1)` calls from cursor 1 (anchor 1) to give `[1, 2, 4]` with cursor 4: first extend → cursor 2, range 1..2 = {1,2}; second → cursor 4, range 1..4 = {1,2,4}. But `moveCursor` sets `selected` empty and `anchor = cursor`; so the first `extend` uses anchor 1. Verify against the tests and adjust if off by one.

- [ ] **Step 4: Run tests**

Run: `cd frontend && pnpm test -- --run src/lib/lineSelection`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/lineSelection.ts frontend/src/lib/lineSelection.test.ts
git commit -m "feat(frontend): line selection reducer for partial staging"
```

---

### Task 10: `DiffView` — line selection, line actions, options toolbar

**Files:**
- Modify: `frontend/src/components/DiffView.tsx`, `DiffView.module.css`, `DiffView.test.tsx`, `DiffPane.tsx` (pass new props), `DiffPane.test.tsx`

**Interfaces:**
- Consumes: Task 8 `useDiffSettingsContext`, Task 9 reducer.
- Produces — new `DiffView` props:
  ```ts
  onStageLines?: (oldStart: number, newStart: number, lineIndices: number[]) => void;
  onUnstageLines?: (oldStart: number, newStart: number, lineIndices: number[]) => void;
  onDiscardLines?: (oldStart: number, newStart: number, lineIndices: number[]) => void;
  partialActionsDisabled?: boolean;
  ```
  Each selectable line row gets `data-line-index={lineIndex}` and `data-hunk-index={hunkIndex}`; selected rows `aria-selected="true"` and class `styles.lineSelected`.
- Toolbar (inside `DiffView`'s existing `Toolbar`, reading/writing `useDiffSettingsContext()`): checkbox-style toggle buttons with `aria-pressed`: "Ignore whitespace", "Syntax highlighting"; context stepper: button `aria-label="Fewer context lines"` (−), `<span aria-label="Context lines">{n}</span>`, button `aria-label="More context lines"` (+), disabled at 0 / 20.

- [ ] **Step 1: Write the failing tests** — append to `DiffView.test.tsx` (reuse its existing hunk fixtures/render helpers; read the file top first):

```tsx
const lineHunk: DiffHunk = {
  oldStart: 1, oldLines: 3, newStart: 1, newLines: 4,
  lines: [
    { origin: "Context", content: "a\n" },
    { origin: "Remove", content: "b\n" },
    { origin: "Add", content: "B\n" },
    { origin: "Context", content: "c\n" },
    { origin: "Add", content: "d\n" },
  ],
};

const row = (container: HTMLElement, index: number) =>
  container.querySelector<HTMLElement>(`[data-hunk-index="0"][data-line-index="${index}"]`)!;

describe("DiffView line selection", () => {
  it("click then shift-click selects a range and stages those lines", async () => {
    const onStageLines = vi.fn();
    const { container } = render(<DiffView hunks={[lineHunk]} onStageHunk={vi.fn()} onStageLines={onStageLines} />);
    fireEvent.click(row(container, 1));
    fireEvent.click(row(container, 4), { shiftKey: true });
    expect(screen.getByRole("button", { name: "Stage lines (3)" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Stage lines (3)" }));
    expect(onStageLines).toHaveBeenCalledWith(1, 1, [1, 2, 4]);
  });

  it("ctrl-click toggles single lines", () => {
    const onStageLines = vi.fn();
    const { container } = render(<DiffView hunks={[lineHunk]} onStageHunk={vi.fn()} onStageLines={onStageLines} />);
    fireEvent.click(row(container, 2));
    fireEvent.click(row(container, 4), { ctrlKey: true });
    expect(row(container, 2)).toHaveAttribute("aria-selected", "true");
    expect(row(container, 4)).toHaveAttribute("aria-selected", "true");
    expect(screen.getByText("2 lines selected")).toBeInTheDocument();
  });

  it("s stages the selection, or the hunk when nothing is selected", () => {
    const onStageHunk = vi.fn();
    const onStageLines = vi.fn();
    const { container } = render(<DiffView hunks={[lineHunk]} onStageHunk={onStageHunk} onStageLines={onStageLines} />);
    const group = screen.getByRole("group", { name: "Diff hunks" });
    fireEvent.keyDown(group, { key: "s" });
    expect(onStageHunk).toHaveBeenCalledWith(1, 1);
    fireEvent.click(row(container, 4));
    fireEvent.keyDown(group, { key: "s" });
    expect(onStageLines).toHaveBeenCalledWith(1, 1, [4]);
  });

  it("arrow keys move a line cursor and shift-arrow extends", () => {
    const onStageLines = vi.fn();
    render(<DiffView hunks={[lineHunk]} onStageHunk={vi.fn()} onStageLines={onStageLines} />);
    const group = screen.getByRole("group", { name: "Diff hunks" });
    fireEvent.keyDown(group, { key: "ArrowDown" });
    fireEvent.keyDown(group, { key: "ArrowDown", shiftKey: true });
    fireEvent.keyDown(group, { key: "s" });
    expect(onStageLines).toHaveBeenCalledWith(1, 1, [1, 2]);
  });

  it("d twice discards the selected lines", () => {
    const onDiscardLines = vi.fn();
    const { container } = render(
      <DiffView hunks={[lineHunk]} onStageHunk={vi.fn()} onDiscardHunk={vi.fn()} onDiscardLines={onDiscardLines} />,
    );
    fireEvent.click(row(container, 2));
    const group = screen.getByRole("group", { name: "Diff hunks" });
    fireEvent.keyDown(group, { key: "d" });
    fireEvent.keyDown(group, { key: "d" });
    expect(onDiscardLines).toHaveBeenCalledWith(1, 1, [2]);
  });

  it("partialActionsDisabled hides hunk and line actions and shows the hint", () => {
    render(<DiffView hunks={[lineHunk]} onStageHunk={vi.fn()} onStageLines={vi.fn()} partialActionsDisabled />);
    expect(screen.queryByRole("button", { name: "Stage hunk" })).toBeNull();
    expect(screen.getByText("Turn off Ignore whitespace to stage partially.")).toBeInTheDocument();
  });

  it("selection clears when hunks change", () => {
    const { container, rerender } = render(<DiffView hunks={[lineHunk]} onStageHunk={vi.fn()} onStageLines={vi.fn()} />);
    fireEvent.click(row(container, 2));
    rerender(<DiffView hunks={[{ ...lineHunk }]} onStageHunk={vi.fn()} onStageLines={vi.fn()} />);
    expect(screen.queryByText(/lines? selected/)).toBeNull();
  });
});

describe("DiffView options toolbar", () => {
  it("toggles ignore whitespace and steps context lines through the settings context", () => {
    const update = vi.fn();
    render(
      <TestDiffSettings value={{ settings: { ignoreWhitespace: false, contextLines: 3, syntaxHighlight: true }, update }}>
        <DiffView hunks={[lineHunk]} />
      </TestDiffSettings>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Ignore whitespace" }));
    expect(update).toHaveBeenCalledWith({ ignoreWhitespace: true });
    fireEvent.click(screen.getByRole("button", { name: "More context lines" }));
    expect(update).toHaveBeenCalledWith({ contextLines: 4 });
  });
});
```

`TestDiffSettings` — export a tiny test-only provider from `useDiffSettings.tsx`: `export const DiffSettingsContextForTests = DiffSettingsContext;` and in the test `const TestDiffSettings = ({ value, children }) => <DiffSettingsContextForTests.Provider value={{ error: null, dismissError: () => {}, ...value }}>{children}</DiffSettingsContextForTests.Provider>;`.

`DiffPane.test.tsx`: add one test — with `getDiffSettings`-free default context, render the uncommitted pane, click a line row, click "Stage lines (1)", and assert `onStageLines` prop (new `DiffPane` prop, see Step 3) was called with `("f.txt", oldStart, newStart, 3, [index])`. And Review Focus 5: wrap `DiffPane` in `TestDiffSettings` with `ignoreWhitespace: true`; assert both the unstaged *and* staged section show the hint and no "Stage hunk"/"Unstage hunk", while the file-level "Stage f.txt" button still exists; and that `CommitDiffPane` (selectedRow = a commit) shows diff lines with no hint.

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/components/DiffView src/components/DiffPane`
Expected: FAIL.

- [ ] **Step 3: Implement**

In `DiffView.tsx`:
1. `const [selection, dispatchSel] = useReducer((s: LineSelection, a: SelectionAction) => reduceSelection(s, a, hunks ?? []), EMPTY_SELECTION);` — add `dispatchSel({ type: "clear" })` to the existing reset-during-render block (where `confirmingDiscardIndex`/`activeIndex` reset). Because `useReducer`'s reducer closes over `hunks`, define the reducer with `useCallback` keyed on `hunks`, or pass hunks in the action; simplest: keep `reduceSelection` pure and call `setSelection(prev => reduceSelection(prev, action, hunks))` with `useState`.
2. `const selectedIn = (hunkIndex: number) => selection.hunkIndex === hunkIndex ? [...selection.selected].sort((a, b) => a - b) : [];`
3. Row handlers on every unified row and split column cell whose line is `Add`/`Remove`:
   ```tsx
   onMouseDown={(event) => { if (event.button === 0) dragging.current = { hunkIndex, line: lineIndex }; }}
   onMouseEnter={() => { if (dragging.current?.hunkIndex === hunkIndex) apply({ type: "dragTo", hunkIndex, line: lineIndex }); }}
   onClick={(event) => apply(
     event.shiftKey ? { type: "shiftClick", hunkIndex, line: lineIndex }
     : event.ctrlKey || event.metaKey ? { type: "toggle", hunkIndex, line: lineIndex }
     : { type: "click", hunkIndex, line: lineIndex })}
   ```
   with `const dragging = useRef<{ hunkIndex: number; line: number } | null>(null);` cleared on a `mouseup` listener on `window` (add/remove in an effect). Clicking a row also `setActiveIndex(hunkIndex)`. Skip all handlers when `partialActionsDisabled` or no line callbacks are given.
   In split view, the line index for a cell is the index of that `DiffLine` object in `hunk.lines` — precompute `const indexOf = new Map(hunk.lines.map((l, i) => [l, i]))` per hunk and use `indexOf.get(row.oldLine)` / `indexOf.get(row.newLine)` / `indexOf.get(row.line)`.
4. Keyboard (`handleKeyDown`): add
   ```ts
   } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
     const delta = event.key === "ArrowDown" ? 1 : -1;
     if (selection.hunkIndex !== active) apply({ type: "focusHunk", hunkIndex: active });
     apply(event.shiftKey ? { type: "extend", delta } : { type: "moveCursor", delta });
   ```
   (dispatch the `focusHunk` and then the move; with `setSelection(prev => …)` both apply in order). `[`/`]` additionally `apply({ type: "clear" })`. `s`: if `selectedIn(active).length > 0` → `onStageLines ?? onUnstageLines` with the indices, else existing hunk behavior. `d`/`d`: same split between `onDiscardLines` and `onDiscardHunk`. Update `aria-keyshortcuts` to `"[ ] s d ArrowUp ArrowDown Shift+ArrowUp Shift+ArrowDown"`. The `ArrowDown`/`ArrowUp` branch only runs when `hasActions`; it must `preventDefault` + `stopPropagation` like the others.
5. Hunk header labels: with `n = selectedIn(hunkIndex).length`, show `Stage lines (n)` / `Unstage lines (n)` / `Discard lines (n)` (armed: `Confirm discard`) and call the line callbacks; otherwise the existing hunk buttons.
6. `partialActionsDisabled`: `hasActions` becomes `!partialActionsDisabled && (…existing…)`; when `partialActionsDisabled` render `<p className={styles.hint}>Turn off Ignore whitespace to stage partially.</p>` above the hunks.
7. Live region: `<span role="status" aria-live="polite" className={styles.srOnly}>{count > 0 ? `${count} ${count === 1 ? "line" : "lines"} selected` : ""}</span>` — use the repo's existing visually-hidden class if one exists (grep `srOnly|visuallyHidden`), else add `.srOnly` to the module CSS.
8. Toolbar additions from `useDiffSettingsContext()`:
   ```tsx
   <button type="button" aria-pressed={settings.ignoreWhitespace} onClick={() => update({ ignoreWhitespace: !settings.ignoreWhitespace })}>Ignore whitespace</button>
   <span className={styles.contextStepper}>
     <button type="button" aria-label="Fewer context lines" disabled={settings.contextLines <= 0} onClick={() => update({ contextLines: settings.contextLines - 1 })}>−</button>
     <span aria-label="Context lines">{settings.contextLines}</span>
     <button type="button" aria-label="More context lines" disabled={settings.contextLines >= MAX_CONTEXT_LINES} onClick={() => update({ contextLines: settings.contextLines + 1 })}>+</button>
   </span>
   <button type="button" aria-pressed={settings.syntaxHighlight} onClick={() => update({ syntaxHighlight: !settings.syntaxHighlight })}>Syntax highlighting</button>
   ```
   The toolbar must render even when `hunks` is `[]` (so you can turn ignore-whitespace back off when it hides everything): move the "No text differences" early return below the toolbar.
9. CSS: `.lineSelected { background: var(--color-selection, rgba(56, 132, 255, 0.22)); }` (use the existing selection token from the theme if present), `.hint`, `.contextStepper { display: inline-flex; gap: 4px; align-items: center; }`, `user-select: none` on selectable rows while dragging is not needed — keep text selectable.

`DiffPane.tsx`: new props `onStageLines/onUnstageLines/onDiscardLines: (path, oldStart, newStart, contextLines, lineIndices) => void | Promise<void>` threaded like the hunk ones (wrap with a `wrapLineAction` that bumps path versions, mirroring `wrapHunkAction`); in `UncommittedFileSection` pass to `DiffView`:
```tsx
onStageLines={!entry.staged ? (o, n, idx) => onStageLines(entry.path, o, n, settings.contextLines, idx) : undefined}
onUnstageLines={entry.staged ? (o, n, idx) => onUnstageLines(entry.path, o, n, settings.contextLines, idx) : undefined}
onDiscardLines={!entry.staged ? (o, n, idx) => onDiscardLines(entry.path, o, n, settings.contextLines, idx) : undefined}
partialActionsDisabled={settings.ignoreWhitespace}
```
Also fix the existing bug-adjacent default: `onDiscardHunk` is currently passed even for staged entries — keep behavior as is (out of scope).

`App.tsx`: pass `onStageLines={(p, o, n, c, idx) => announce(linesToast("Staged", idx.length), appState.stageLines)(p, o, n, c, idx)}` (and Unstaged/Discarded), with `const linesToast = (verb: string, n: number) => \`${verb} ${n} ${n === 1 ? "line" : "lines"}\`;`.

- [ ] **Step 4: Run tests + typecheck + lint**

Run: `cd frontend && pnpm exec tsc --noEmit && pnpm lint && pnpm test -- --run`
Expected: all green.

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): select diff lines to stage, unstage or discard; diff options toolbar"
```

---

### Task 11: Syntax highlighting

**Files:**
- Modify: `frontend/package.json`, `frontend/pnpm-lock.yaml`, `docs/LICENSE_COMPLIANCE.md`
- Create: `frontend/src/lib/languageForPath.ts` (+ `.test.ts`), `frontend/src/lib/highlight.ts` (+ `.test.ts`), `frontend/src/components/DiffLineContent.tsx`
- Modify: `frontend/src/components/DiffView.tsx` (new prop `path?: string`; use `DiffLineContent`), `DiffPane.tsx` (pass `path`), the global theme stylesheet (find where `[data-theme="dark"]` tokens are defined — `grep -rn 'data-theme' frontend/src --include=*.css`)

**Interfaces:**
- Produces:
  ```ts
  // languageForPath.ts
  export type HighlightLanguage = "typescript" | "tsx" | "javascript" | "jsx" | "json" | "rust" | "python" | "go" | "java" | "c" | "cpp" | "csharp" | "css" | "scss" | "html" | "markdown" | "yaml" | "toml" | "shellscript" | "sql" | "ruby" | "php" | "kotlin" | "swift";
  export function languageForPath(path: string): HighlightLanguage | null;
  // highlight.ts
  export interface Token { content: string; color: string | undefined }
  export const HIGHLIGHT_LINE_LIMIT = 2000;
  export function highlightLines(lang: HighlightLanguage, code: string): Promise<Token[][]>;
  export function useHunkTokens(hunks: DiffHunk[] | null, lang: HighlightLanguage | null, enabled: boolean): Map<DiffLine, Token[]> | null;
  // DiffLineContent.tsx
  export function DiffLineContent(props: { line: DiffLine; tokens: Token[] | undefined; segments: Segment[] | undefined }): JSX.Element;
  ```
- Token colors are `var(--shiki-token-*)` strings from Shiki's CSS-variables theme.

- [ ] **Step 1: Add the dependency + license row**

```bash
cd frontend && npm info shiki@4.5.0 license   # expect MIT
pnpm add shiki@4.5.0 --save-exact
ls node_modules/shiki/dist/langs | head      # confirm per-language module paths for Step 4
```

`docs/LICENSE_COMPLIANCE.md`, JS table, new row:
`| shiki 4.5.0 | MIT | \`frontend\` — syntax highlighting in diffs, using its JavaScript regex engine (no WebAssembly, so no CSP change) and CSS-variables theme. Languages load on demand. Source: [npm](https://www.npmjs.com/package/shiki/v/4.5.0); verified with \`npm info shiki@4.5.0 license\` on 2026-10-16. |`

- [ ] **Step 2: Write the failing tests**

`languageForPath.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { languageForPath } from "./languageForPath";

describe("languageForPath", () => {
  it.each([
    ["src/a.ts", "typescript"], ["App.tsx", "tsx"], ["x.mjs", "javascript"], ["lib.rs", "rust"],
    ["Cargo.toml", "toml"], ["a.yml", "yaml"], ["README.md", "markdown"], ["run.sh", "shellscript"],
    ["dir.with.dots/file.PY", "python"],
  ])("%s → %s", (path, lang) => expect(languageForPath(path)).toBe(lang));

  it("returns null for unknown or extensionless files", () => {
    expect(languageForPath("LICENSE")).toBeNull();
    expect(languageForPath("data.bin")).toBeNull();
  });
});
```

`highlight.test.ts` (real Shiki; it runs in Node/jsdom without WASM):

```ts
import { describe, expect, it } from "vitest";
import { highlightLines } from "./highlight";

describe("highlightLines", () => {
  it("tokenizes per line with css-variable colors", async () => {
    const lines = await highlightLines("typescript", "const a = 1;\n// note");
    expect(lines).toHaveLength(2);
    expect(lines[0].map((t) => t.content).join("")).toBe("const a = 1;");
    expect(lines[0].some((t) => t.color?.startsWith("var(--shiki-token"))).toBe(true);
  });

  it("caches identical requests", async () => {
    const a = await highlightLines("rust", "fn main() {}");
    const b = await highlightLines("rust", "fn main() {}");
    expect(b).toBe(a);
  });
});
```

`DiffView.test.tsx` addition:

```tsx
it("renders plain text when highlighting fails", async () => {
  vi.spyOn(highlightModule, "highlightLines").mockRejectedValue(new Error("boom"));
  render(<DiffView hunks={[lineHunk]} path="f.ts" />);
  expect(await screen.findByText("B")).toBeInTheDocument();
});

it("colors tokens when highlighting is on", async () => {
  const { container } = render(<DiffView hunks={[{ ...lineHunk, lines: [{ origin: "Add", content: "const x = 1;\n" }] }]} path="f.ts" />);
  await waitFor(() => expect(container.querySelector('[style*="--shiki-token"]')).not.toBeNull());
});
```

(`import * as highlightModule from "../lib/highlight";` — if ESM spying on the module namespace fails under Vitest, use `vi.mock("../lib/highlight", async (orig) => ({ ...(await orig()), highlightLines: vi.fn().mockRejectedValue(new Error("boom")) }))` in a separate test file `DiffView.highlightFailure.test.tsx`.)

- [ ] **Step 3: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/lib/languageForPath src/lib/highlight src/components/DiffView`
Expected: FAIL.

- [ ] **Step 4: Implement**

`languageForPath.ts`:

```ts
export type HighlightLanguage =
  | "typescript" | "tsx" | "javascript" | "jsx" | "json" | "rust" | "python" | "go" | "java"
  | "c" | "cpp" | "csharp" | "css" | "scss" | "html" | "markdown" | "yaml" | "toml"
  | "shellscript" | "sql" | "ruby" | "php" | "kotlin" | "swift";

const BY_EXTENSION: Record<string, HighlightLanguage> = {
  ts: "typescript", mts: "typescript", cts: "typescript", tsx: "tsx",
  js: "javascript", mjs: "javascript", cjs: "javascript", jsx: "jsx",
  json: "json", rs: "rust", py: "python", go: "go", java: "java",
  c: "c", h: "c", cc: "cpp", cpp: "cpp", hpp: "cpp", cs: "csharp",
  css: "css", scss: "scss", html: "html", htm: "html", md: "markdown",
  yml: "yaml", yaml: "yaml", toml: "toml", sh: "shellscript", bash: "shellscript", zsh: "shellscript",
  sql: "sql", rb: "ruby", php: "php", kt: "kotlin", kts: "kotlin", swift: "swift",
};

export function languageForPath(path: string): HighlightLanguage | null {
  const name = path.split("/").pop() ?? "";
  const dot = name.lastIndexOf(".");
  if (dot <= 0) return null;
  return BY_EXTENSION[name.slice(dot + 1).toLowerCase()] ?? null;
}
```

`highlight.ts`:

```ts
import { useEffect, useState } from "react";
import { createHighlighterCore, createCssVariablesTheme, type HighlighterCore } from "shiki/core";
import { createJavaScriptRegexEngine } from "shiki/engine/javascript";
import type { DiffHunk, DiffLine } from "../ipc/RepoClient";
import type { HighlightLanguage } from "./languageForPath";

export interface Token {
  content: string;
  color: string | undefined;
}

export const HIGHLIGHT_LINE_LIMIT = 2000;
const THEME = createCssVariablesTheme({ name: "browsitory", variablePrefix: "--shiki-", fontStyle: false });

// Each loader is a static string so Vite can split it into its own chunk. Confirm the module
// path against `node_modules/shiki/dist/langs` (Task 11 Step 1).
const LOADERS: Record<HighlightLanguage, () => Promise<unknown>> = {
  typescript: () => import("shiki/langs/typescript.mjs"),
  tsx: () => import("shiki/langs/tsx.mjs"),
  javascript: () => import("shiki/langs/javascript.mjs"),
  jsx: () => import("shiki/langs/jsx.mjs"),
  json: () => import("shiki/langs/json.mjs"),
  rust: () => import("shiki/langs/rust.mjs"),
  python: () => import("shiki/langs/python.mjs"),
  go: () => import("shiki/langs/go.mjs"),
  java: () => import("shiki/langs/java.mjs"),
  c: () => import("shiki/langs/c.mjs"),
  cpp: () => import("shiki/langs/cpp.mjs"),
  csharp: () => import("shiki/langs/csharp.mjs"),
  css: () => import("shiki/langs/css.mjs"),
  scss: () => import("shiki/langs/scss.mjs"),
  html: () => import("shiki/langs/html.mjs"),
  markdown: () => import("shiki/langs/markdown.mjs"),
  yaml: () => import("shiki/langs/yaml.mjs"),
  toml: () => import("shiki/langs/toml.mjs"),
  shellscript: () => import("shiki/langs/shellscript.mjs"),
  sql: () => import("shiki/langs/sql.mjs"),
  ruby: () => import("shiki/langs/ruby.mjs"),
  php: () => import("shiki/langs/php.mjs"),
  kotlin: () => import("shiki/langs/kotlin.mjs"),
  swift: () => import("shiki/langs/swift.mjs"),
};

let highlighter: Promise<HighlighterCore> | null = null;
const loaded = new Map<HighlightLanguage, Promise<void>>();
const cache = new Map<string, Promise<Token[][]>>();
const CACHE_LIMIT = 500;

function getHighlighter(): Promise<HighlighterCore> {
  highlighter ??= createHighlighterCore({ themes: [THEME], langs: [], engine: createJavaScriptRegexEngine() });
  return highlighter;
}

async function ensureLanguage(h: HighlighterCore, lang: HighlightLanguage): Promise<void> {
  let p = loaded.get(lang);
  if (p === undefined) {
    p = LOADERS[lang]().then((mod) => h.loadLanguage((mod as { default: Parameters<HighlighterCore["loadLanguage"]>[0] }).default));
    loaded.set(lang, p);
  }
  return p;
}

export function highlightLines(lang: HighlightLanguage, code: string): Promise<Token[][]> {
  const key = `${lang}\u0000${code}`;
  const hit = cache.get(key);
  if (hit !== undefined) return hit;
  const result = (async () => {
    const h = await getHighlighter();
    await ensureLanguage(h, lang);
    const { tokens } = h.codeToTokens(code, { lang, theme: "browsitory" });
    return tokens.map((line) => line.map((t) => ({ content: t.content, color: t.color })));
  })();
  result.catch(() => cache.delete(key));
  cache.set(key, result);
  if (cache.size > CACHE_LIMIT) cache.delete(cache.keys().next().value!);
  return result;
}

const stripEol = (s: string) => s.replace(/\r?\n$/, "");
const isEofMarker = (l: DiffLine) => l.origin === "Context" && l.content.includes("\\ No newline at end of file");

/**
 * Tokenizes each hunk's old side (context + removed) and new side (context + added) as two
 * continuous blocks, so multi-line strings/comments inside a hunk come out right, then maps the
 * tokens back onto the hunk's lines. Returns null while loading, when disabled, over the line
 * cap, for unknown languages, or on failure (callers render plain text).
 */
export function useHunkTokens(hunks: DiffHunk[] | null, lang: HighlightLanguage | null, enabled: boolean, onError?: (e: unknown) => void): Map<DiffLine, Token[]> | null {
  const [tokens, setTokens] = useState<Map<DiffLine, Token[]> | null>(null);
  useEffect(() => {
    setTokens(null);
    if (!enabled || lang === null || hunks === null) return;
    const total = hunks.reduce((n, h) => n + h.lines.length, 0);
    if (total > HIGHLIGHT_LINE_LIMIT) return;
    let cancelled = false;
    (async () => {
      const map = new Map<DiffLine, Token[]>();
      for (const hunk of hunks) {
        const oldSide = hunk.lines.filter((l) => !isEofMarker(l) && l.origin !== "Add");
        const newSide = hunk.lines.filter((l) => !isEofMarker(l) && l.origin !== "Remove");
        const [oldTokens, newTokens] = await Promise.all([
          highlightLines(lang, oldSide.map((l) => stripEol(l.content)).join("\n")),
          highlightLines(lang, newSide.map((l) => stripEol(l.content)).join("\n")),
        ]);
        oldSide.forEach((l, i) => l.origin === "Remove" && map.set(l, oldTokens[i] ?? []));
        newSide.forEach((l, i) => map.set(l, newTokens[i] ?? []));
      }
      if (!cancelled) setTokens(map);
    })().catch((e: unknown) => onError?.(e));
    return () => {
      cancelled = true;
    };
  }, [hunks, lang, enabled, onError]);
  return tokens;
}
```

(The `react-hooks/set-state-in-effect` lint rule exists in this repo — see `DiffPane.tsx:503`; add the same `// eslint-disable-next-line` comment above `setTokens(null)`.)

`DiffLineContent.tsx` — merges tokens with word-diff segments: word-diff `<mark>` boundaries win; inside each segment, slice the token runs by character offset:

```tsx
import type { DiffLine } from "../ipc/RepoClient";
import type { Token } from "../lib/highlight";
import type { Segment } from "../lib/wordDiff";

type Run = { text: string; color: string | undefined; changed: boolean };

function runs(text: string, tokens: Token[] | undefined, segments: Segment[] | undefined): Run[] {
  const tokenRuns = tokens ?? [{ content: text, color: undefined }];
  const segs = segments ?? [{ text, changed: false }];
  const out: Run[] = [];
  let ti = 0;
  let tOffset = 0;
  for (const seg of segs) {
    let remaining = seg.text.length;
    while (remaining > 0 && ti < tokenRuns.length) {
      const tok = tokenRuns[ti];
      const take = Math.min(remaining, tok.content.length - tOffset);
      out.push({ text: tok.content.slice(tOffset, tOffset + take), color: tok.color, changed: seg.changed });
      remaining -= take;
      tOffset += take;
      if (tOffset >= tok.content.length) {
        ti++;
        tOffset = 0;
      }
    }
    if (remaining > 0) out.push({ text: seg.text.slice(seg.text.length - remaining), color: undefined, changed: seg.changed });
  }
  return out;
}

export function DiffLineContent({ line, tokens, segments }: { line: DiffLine; tokens: Token[] | undefined; segments: Segment[] | undefined }) {
  if (tokens === undefined && segments === undefined) return <>{line.content}</>;
  const text = segments === undefined ? line.content.replace(/\r?\n$/, "") : segments.map((s) => s.text).join("");
  return (
    <>
      {runs(text, tokens, segments).map((r, i) => {
        const style = r.color === undefined ? undefined : { color: r.color };
        return r.changed ? <mark key={i} style={style}>{r.text}</mark> : <span key={i} style={style}>{r.text}</span>;
      })}
    </>
  );
}
```

Check `lib/wordDiff.ts` exports `Segment` (`{ text, changed }`); if its texts include the trailing newline, keep it consistent (tokens exclude it — the leftover goes in the uncolored tail run, which is fine).

`DiffView.tsx`: new prop `path?: string`; `const { settings } = useDiffSettingsContext(); const tokens = useHunkTokens(hunks, path ? languageForPath(path) : null, settings.syntaxHighlight, logHighlightError);` where `logHighlightError` is a module-level function that calls `logger` from `lib/logger.ts` (check its API; it routes to `RepoClient.logFrontendError`). Replace `renderContent(line, wordSegments)` and the plain `{row.line.content}` / `{line.content}` spots with `<DiffLineContent line={l} tokens={tokens?.get(l)} segments={wordSegments.get(l)} />`; delete `renderContent`.

`DiffPane.tsx`: pass `path={entry.path}` / `path={path}` to both `DiffView` usages.

Theme CSS — add for light and dark next to the other color tokens:

```css
:root {
  --shiki-foreground: inherit;
  --shiki-token-constant: #005cc5;
  --shiki-token-string: #032f62;
  --shiki-token-comment: #6a737d;
  --shiki-token-keyword: #d73a49;
  --shiki-token-parameter: #24292e;
  --shiki-token-function: #6f42c1;
  --shiki-token-string-expression: #22863a;
  --shiki-token-punctuation: #24292e;
  --shiki-token-link: #032f62;
}
[data-theme="dark"] {
  --shiki-token-constant: #79b8ff;
  --shiki-token-string: #9ecbff;
  --shiki-token-comment: #959da5;
  --shiki-token-keyword: #f97583;
  --shiki-token-parameter: #e1e4e8;
  --shiki-token-function: #b392f0;
  --shiki-token-string-expression: #85e89d;
  --shiki-token-punctuation: #e1e4e8;
  --shiki-token-link: #9ecbff;
}
```

(Match the selectors the repo actually uses for theme tokens; `applyTheme` sets `document.documentElement.dataset.theme`.)

- [ ] **Step 5: Run tests + typecheck + lint + build**

Run: `cd frontend && pnpm exec tsc --noEmit && pnpm lint && pnpm test -- --run && pnpm build`
Expected: all green; `pnpm build` output shows separate language chunks and no `.wasm` asset.

- [ ] **Step 6: Commit**

```bash
git add frontend docs/LICENSE_COMPLIANCE.md
git commit -m "feat(frontend): syntax highlighting in diffs (Shiki, JS regex engine)"
```

---

### Task 12: File actions menu, discard all, ignore

**Files:**
- Create: `frontend/src/components/FileActionsMenu.tsx`, `FileActionsMenu.test.tsx`
- Modify: `frontend/src/components/DiffPane.tsx` (+ test), `DiffPane.module.css`, `App.tsx`

**Interfaces:**
- Consumes: Task 8 actions.
- Produces:
  ```ts
  export interface FileAction { kind: "discard" } | { kind: "ignore"; pattern: string };
  export function fileMenuItems(entry: StatusEntry, onPick: (action: FileAction) => void): ContextMenuItem[];
  export function ignorePatterns(path: string): Array<{ label: string; pattern: string }>;
  export function discardConfirmMessage(entry: StatusEntry): string;
  export function discardAllConfirmMessage(entries: StatusEntry[]): string;
  // DiffPane new props:
  onDiscardFile: (path: string) => void;
  onDiscardAllUnstaged: () => void;
  onIgnorePath: (pattern: string) => void;
  ```
- Untracked ⇔ `entry.kind === "New" && !entry.staged` (check `StatusKind` values in `RepoClient.ts`; use the variant `status.rs` reports for untracked files).

- [ ] **Step 1: Write the failing tests** — `FileActionsMenu.test.tsx`:

```ts
import { describe, expect, it, vi } from "vitest";
import { discardAllConfirmMessage, discardConfirmMessage, fileMenuItems, ignorePatterns } from "./FileActionsMenu";

const tracked = { path: "src/a.ts", staged: false, kind: "Modified" as const };
const untracked = { path: "logs/run.log", staged: false, kind: "New" as const };

describe("FileActionsMenu", () => {
  it("offers only discard for tracked files", () => {
    expect(fileMenuItems(tracked, vi.fn()).map((i) => i.label)).toEqual(["Discard changes…"]);
  });

  it("offers ignore options for untracked files", () => {
    expect(fileMenuItems(untracked, vi.fn()).map((i) => i.label)).toEqual([
      "Discard changes…", "Ignore file", "Ignore all *.log", "Ignore folder logs/",
    ]);
  });

  it("builds root-anchored ignore patterns", () => {
    expect(ignorePatterns("logs/run.log")).toEqual([
      { label: "Ignore file", pattern: "/logs/run.log" },
      { label: "Ignore all *.log", pattern: "*.log" },
      { label: "Ignore folder logs/", pattern: "/logs/" },
    ]);
    expect(ignorePatterns("Makefile")).toEqual([{ label: "Ignore file", pattern: "/Makefile" }]);
  });

  it("marks discard destructive and reports the picked action", () => {
    const onPick = vi.fn();
    const items = fileMenuItems(untracked, onPick);
    expect(items[0].destructive).toBe(true);
    items[2].onSelect();
    expect(onPick).toHaveBeenCalledWith({ kind: "ignore", pattern: "*.log" });
  });

  it("words the confirm dialogs", () => {
    expect(discardConfirmMessage(tracked)).toBe("Discard changes to src/a.ts? This can't be undone.");
    expect(discardConfirmMessage(untracked)).toBe("Delete untracked file logs/run.log? This can't be undone.");
    expect(discardAllConfirmMessage([tracked, untracked])).toBe(
      "Discard changes to 2 files (1 untracked will be deleted)? This can't be undone.",
    );
    expect(discardAllConfirmMessage([tracked])).toBe("Discard changes to 1 file? This can't be undone.");
  });
});
```

`DiffPane.test.tsx` additions:
- Clicking the "⋯" button (`aria-label="Actions for src/a.ts"`) opens a menu; choosing "Discard changes…" shows a dialog with the tracked message; confirming calls `onDiscardFile("src/a.ts")`; cancelling does not.
- Right-click (`fireEvent.contextMenu`) on the file header opens the same menu.
- "Discard all…" button next to "Stage all" opens the discard-all dialog; confirming calls `onDiscardAllUnstaged()`. The button is disabled when every unstaged entry is conflicted. Conflicted files get no "⋯" discard item.
- "Ignore all *.log" calls `onIgnorePath("*.log")` directly (no confirm).

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/components/FileActionsMenu src/components/DiffPane`
Expected: FAIL.

- [ ] **Step 3: Implement** — `FileActionsMenu.tsx`:

```tsx
import type { StatusEntry } from "../ipc/RepoClient";
import type { ContextMenuItem } from "./primitives/ContextMenu";

export type FileAction = { kind: "discard" } | { kind: "ignore"; pattern: string };

export const isUntracked = (entry: StatusEntry) => !entry.staged && entry.kind === "New";

export function ignorePatterns(path: string): Array<{ label: string; pattern: string }> {
  const out = [{ label: "Ignore file", pattern: `/${path}` }];
  const name = path.split("/").pop() ?? path;
  const dot = name.lastIndexOf(".");
  if (dot > 0) {
    const ext = name.slice(dot);
    out.push({ label: `Ignore all *${ext}`, pattern: `*${ext}` });
  }
  const slash = path.lastIndexOf("/");
  if (slash > 0) {
    const folder = path.slice(0, slash + 1);
    out.push({ label: `Ignore folder ${folder}`, pattern: `/${folder}` });
  }
  return out;
}

export function fileMenuItems(entry: StatusEntry, onPick: (action: FileAction) => void): ContextMenuItem[] {
  const items: ContextMenuItem[] = [
    { label: "Discard changes…", destructive: true, onSelect: () => onPick({ kind: "discard" }) },
  ];
  if (isUntracked(entry)) {
    for (const { label, pattern } of ignorePatterns(entry.path)) {
      items.push({ label, onSelect: () => onPick({ kind: "ignore", pattern }) });
    }
  }
  return items;
}

export function discardConfirmMessage(entry: StatusEntry): string {
  return isUntracked(entry)
    ? `Delete untracked file ${entry.path}? This can't be undone.`
    : `Discard changes to ${entry.path}? This can't be undone.`;
}

export function discardAllConfirmMessage(entries: StatusEntry[]): string {
  const n = entries.length;
  const m = entries.filter(isUntracked).length;
  const files = `${n} ${n === 1 ? "file" : "files"}`;
  const untracked = m > 0 ? ` (${m} untracked will be deleted)` : "";
  return `Discard changes to ${files}${untracked}? This can't be undone.`;
}
```

`DiffPane.tsx`, `UncommittedFileSection` (unstaged, non-conflicted entries only):
- State `menu: { x: number; y: number } | null` and `confirmingDiscard: boolean`.
- Header: `onContextMenu={(e) => { e.preventDefault(); setMenu({ x: e.clientX, y: e.clientY }); }}`; add before the stage button:
  ```tsx
  <button type="button" aria-label={`Actions for ${entry.path}`} aria-haspopup="menu"
    onClick={(e) => { e.stopPropagation(); const r = e.currentTarget.getBoundingClientRect(); setMenu({ x: r.left, y: r.bottom }); }}>
    <MoreHorizontal size={14} aria-hidden="true" />
  </button>
  ```
  (`MoreHorizontal` from `lucide-react`, already a dependency.) Shift+F10 / Menu key: the `ListRow` gets an `onKeyDown` that opens the menu at the header's rect when `event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")` — follow `BranchTree.tsx:352`'s pattern.
- `{menu && <ContextMenu x={menu.x} y={menu.y} items={fileMenuItems(entry, (a) => a.kind === "discard" ? setConfirmingDiscard(true) : onIgnorePath(a.pattern))} onClose={() => setMenu(null)} />}`
- `{confirmingDiscard && <ConfirmDialog ariaLabel="Discard changes" message={discardConfirmMessage(entry)} confirmLabel={isUntracked(entry) ? "Delete file" : "Discard changes"} onConfirm={() => { setConfirmingDiscard(false); onDiscardFile(entry.path); }} onCancel={() => setConfirmingDiscard(false)} />}`

`UncommittedDiffPane`: next to "Stage all", `<button type="button" onClick={() => setConfirmingDiscardAll(true)} disabled={discardablePaths.length === 0}>Discard all…</button>` where `discardableEntries = unstagedEntries.filter((e) => e.kind !== "Conflicted")`; dialog uses `discardAllConfirmMessage(discardableEntries)`, confirm label "Discard all", confirm calls `onDiscardAllUnstaged()`.

Thread `onDiscardFile`, `onDiscardAllUnstaged`, `onIgnorePath` through `DiffPane` → `UncommittedDiffPane` → `UncommittedFileSection`. `onDiscardFile` should also bump that path's diff versions (wrap like `wrapHunkAction`).

`App.tsx`:
```tsx
onDiscardFile={(path) => announce(`Discarded changes to ${path}`, appState.discardFile)(path)}
onDiscardAllUnstaged={announce("Discarded all unstaged changes", appState.discardAllUnstaged)}
onIgnorePath={(pattern) => announce(`Added ${pattern} to .gitignore`, appState.ignorePath)(pattern)}
```

- [ ] **Step 4: Run tests + typecheck + lint**

Run: `cd frontend && pnpm exec tsc --noEmit && pnpm lint && pnpm test -- --run`
Expected: all green.

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): file actions menu with discard and ignore; discard all unstaged"
```

---

### Task 13: Command palette and shortcut sheet

**Files:**
- Modify: `frontend/src/lib/commands.ts` (+ its test), `frontend/src/App.tsx` (pass the diff-settings API and current-file callbacks to `buildCommands`), `frontend/src/components/ShortcutSheet.tsx` (+ test)

**Interfaces:**
- `buildCommands` gains one trailing optional parameter object (keeps existing positional params unchanged):
  ```ts
  diffCommands?: {
    settings: DiffSettings;
    update(partial: Partial<DiffSettings>): void;
    currentFile: string | null;           // DiffPane's current unstaged file, if any
    requestDiscardCurrentFile(): void;    // opens the same confirm dialog as the menu
    requestDiscardAllUnstaged(): void;    // opens the discard-all confirm dialog
  }
  ```
- Command ids/labels: `toggle-ignore-whitespace` "Toggle ignore whitespace"; `increase-context-lines` "Increase context lines"; `decrease-context-lines` "Decrease context lines"; `toggle-syntax-highlighting` "Toggle syntax highlighting"; `discard-current-file` "Discard changes in current file" (only when `currentFile !== null` and not `repositoryOperationDisabled`); `discard-all-unstaged` "Discard all unstaged changes" (only when there are non-conflicted unstaged entries and not `repositoryOperationDisabled`).

- [ ] **Step 1: Write the failing tests** — in `commands.test.ts`, following its existing `buildCommands` harness:

```ts
it("adds diff option commands that update settings", () => {
  const update = vi.fn();
  const commands = buildCommands(appStateFixture(), undefined, [], undefined, undefined, undefined, undefined, {
    settings: { ignoreWhitespace: false, contextLines: 3, syntaxHighlight: true },
    update,
    currentFile: null,
    requestDiscardCurrentFile: vi.fn(),
    requestDiscardAllUnstaged: vi.fn(),
  });
  commands.find((c) => c.id === "toggle-ignore-whitespace")!.run();
  expect(update).toHaveBeenCalledWith({ ignoreWhitespace: true });
  commands.find((c) => c.id === "increase-context-lines")!.run();
  expect(update).toHaveBeenCalledWith({ contextLines: 4 });
  expect(commands.some((c) => c.id === "discard-current-file")).toBe(false);
});

it("offers discard commands only when there is something to discard and no operation is running", () => {
  const requestDiscardCurrentFile = vi.fn();
  const state = appStateFixture({ status: [{ path: "a.txt", staged: false, kind: "Modified" }] });
  const commands = buildCommands(state, undefined, [], undefined, undefined, undefined, undefined, {
    settings: { ignoreWhitespace: false, contextLines: 3, syntaxHighlight: true },
    update: vi.fn(),
    currentFile: "a.txt",
    requestDiscardCurrentFile,
    requestDiscardAllUnstaged: vi.fn(),
  });
  commands.find((c) => c.id === "discard-current-file")!.run();
  expect(requestDiscardCurrentFile).toHaveBeenCalled();
  expect(commands.some((c) => c.id === "discard-all-unstaged")).toBe(true);
});
```

(Use whatever fixture builder the existing test file has in place of `appStateFixture`.) `ShortcutSheet.test.tsx`: assert the sheet lists "Move line cursor" (`↑ / ↓`), "Extend line selection" (`Shift+↑ / Shift+↓`), and that the `s`/`d` rows mention "lines or hunk".

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/lib/commands src/components/ShortcutSheet`
Expected: FAIL.

- [ ] **Step 3: Implement** — push the commands in `buildCommands` after the existing diff/staging commands, using `keywords` like `["diff", "whitespace", "ignore"]`, `["diff", "context", "lines", "more"]`, `["discard", "revert", "file"]`. "Discard" commands must not discard directly — they call the `request…` callbacks, which open the confirm dialogs. Lift the "current file" discard-confirm state so both the menu and the palette can open it: simplest is a `discardRequest` state in `RepoWorkspace` (`{ kind: "file"; path } | { kind: "all" } | null`) passed into `DiffPane` as `discardRequest`/`onDiscardRequestHandled` props, rendered by `UncommittedDiffPane` with the Task 12 dialogs. `currentFile` comes from a new `DiffPane` prop `onCurrentFileChange(path | null)` called when `current` changes (unstaged entries only). Add a `commandGroup` mapping if `commandGroup` classifies by id prefix (check it). Update `ShortcutSheet` rows.

- [ ] **Step 4: Run tests + typecheck + lint**

Run: `cd frontend && pnpm exec tsc --noEmit && pnpm lint && pnpm test -- --run`
Expected: all green.

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): palette commands for diff options and discard; shortcut sheet"
```

---

### Task 14: E2E flows

**Files:**
- Create: `e2e/specs/line-staging.spec.ts`, `extension/e2e/src/specs/line-staging.spec.ts`

- [ ] **Step 1: Write the Tauri spec** — modeled on `e2e/specs/hunk-staging.spec.ts`:

```ts
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const FILE = "line-fixture.txt";
const UNTRACKED = "line-untracked.tmp";

describe("Browsitory line staging and discard", () => {
  before(() => {
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), "a\nb\nc\n");
    execFileSync("git", ["add", FILE], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", "e2e: seed line fixture"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), "a\nB\nc\nd\n");
    fs.writeFileSync(path.join(E2E_REPO_PATH, UNTRACKED), "tmp\n");
  });

  after(() => {
    execFileSync("git", ["reset", "-q", "--", FILE], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["checkout", "--", FILE], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    fs.rmSync(path.join(E2E_REPO_PATH, UNTRACKED), { force: true });
  });

  it("stages two selected lines of a hunk", async () => {
    const row = await $(`span=${FILE} (Modified)`);
    await row.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), row);

    const addLines = await $$(".diff-line-add");
    await browser.execute((el) => (el as HTMLElement).click(), addLines[0]);
    await browser.execute(
      (el) => el.dispatchEvent(new MouseEvent("click", { bubbles: true, ctrlKey: true })),
      addLines[1],
    );
    const stageLines = await $("button=Stage lines (2)");
    await browser.execute((el) => (el as HTMLElement).click(), stageLines);

    await browser.waitUntil(
      () => execFileSync("git", ["show", `:${FILE}`], { cwd: E2E_REPO_PATH, encoding: "utf8" }) === "a\nb\nB\nc\nd\n",
      { timeout: 10000, timeoutMsg: "expected exactly the two added lines in the index" },
    );
  });

  it("discards an untracked file through the file menu", async () => {
    const actions = await $(`button[aria-label="Actions for ${UNTRACKED}"]`);
    await actions.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), actions);
    await browser.execute((el) => (el as HTMLElement).click(), await $("button=Discard changes…"));
    await browser.execute((el) => (el as HTMLElement).click(), await $("button=Delete file"));
    await browser.waitUntil(() => !fs.existsSync(path.join(E2E_REPO_PATH, UNTRACKED)), {
      timeout: 10000,
      timeoutMsg: "expected the untracked file to be deleted",
    });
    await expect($(`span=${UNTRACKED} (New)`)).not.toBeExisting();
  });
});
```

(Menu items may render as `[role="menuitem"]` rather than `<button>` — check `ContextMenu.tsx` and adjust the selector. The `(New)` kind label must match what `StatusKind` renders for untracked.)

- [ ] **Step 2: Write the VSCode spec** — modeled on `cherry-pick.spec.ts`: write `vscode-lines.txt` `"a\nb\nc\n"`, commit, modify to `"a\nB\nc\nd\n"`; open Browsitory; `webview.clickByText("span", "vscode-lines.txt (Modified)")`; click the last `.diff-line-add` row (add a `clickBySelector(selector, index)` helper to `extension/e2e/src/support/webviewPage.ts` if none exists, mirroring `clickByText`); `clickByText("button", "Stage lines (1)")`; poll `git show :vscode-lines.txt` until it equals `"a\nb\nc\nd\n"` (15 s deadline); `finally` close webview + `workbench.action.closeAllEditors`; afterwards `git reset -q`, `git checkout -- vscode-lines.txt`.

- [ ] **Step 3: Run E2E** (order-sensitive build steps — copy from `CLAUDE.md`):

```bash
cd frontend && VITE_E2E_REPO_PATH=/tmp/browsitory-e2e-repo pnpm build && cd ..
cargo build --workspace --features tauri-app/custom-protocol,tauri-app/forge-fixture-override
cd e2e && pnpm install && xvfb-run -a pnpm test && cd ..
cargo build --workspace
cd frontend && pnpm run generate:release-notes && VITE_E2E_REPO_PATH=/tmp/browsitory-vscode-e2e-repo pnpm exec vite build --config vite.vscode.config.ts && cd ..
cd extension && pnpm install && pnpm run compile && cd ..
cd extension/e2e && pnpm install --ignore-workspace && xvfb-run -a pnpm test
```

Expected: all specs pass, including the existing `hunk-staging.spec.ts` (regression check for Task 1's arity change).

- [ ] **Step 4: Commit**

```bash
git add e2e extension/e2e
git commit -m "test(e2e): line staging and file discard flows (Tauri and VSCode)"
```

---

### Task 15: Docs and changelog

**Files:**
- Modify: `CHANGELOG.md`, `docs/USER_GUIDE.md`

- [ ] **Step 1: CHANGELOG** — under `## [Unreleased]`, new top section:

```markdown
### Line staging, diff options, syntax highlighting, discard and ignore
- Select individual lines in a diff (click, Shift+click, Ctrl/Cmd+click, drag, or ↑/↓ with
  Shift) and stage, unstage or discard just those lines. `s` and `d` act on the selection when
  there is one, otherwise on the hunk.
- Diff toolbar: Ignore whitespace, context lines (0–20) and Syntax highlighting, remembered
  across sessions. While whitespace is ignored, partial staging is turned off in the working
  tree.
- Diffs are syntax-highlighted for common languages (up to 2,000 lines per file).
- File "⋯" menu (also right-click / Shift+F10): Discard changes…, and for untracked files
  Ignore file / Ignore all *.ext / Ignore folder, which append to `.gitignore`.
- "Discard all…" next to "Stage all" discards every unstaged change, with confirmation.
- Palette commands for each diff option and for discarding.
```

- [ ] **Step 2: USER_GUIDE** — in "Staging and committing" add subsections "Staging individual lines" (selection gestures, keys, header buttons) and "Discarding and ignoring files" (menu, confirm dialogs, no undo, `.gitignore` patterns); in "Browsing history and diffs" add "Diff options" (three toolbar controls, persistence, partial-staging hint) and one line on highlighting. Sentence case; match existing guide tone.

- [ ] **Step 3: Full verification**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check && (cd frontend && pnpm exec tsc --noEmit && pnpm lint && pnpm test -- --run) && (cd extension && pnpm test -- --run)`
Expected: all green.

- [ ] **Step 4: Commit**

```bash
git add CHANGELOG.md docs/USER_GUIDE.md
git commit -m "docs: line staging, diff options, highlighting, discard and ignore"
```

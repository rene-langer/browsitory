# Feature parity, batch 2a: diff pane — line staging, diff options, syntax highlighting, discard/ignore file

## Goal

Close the daily-use diff-pane gaps vs mature Git clients: stage/unstage/discard individual lines,
ignore-whitespace and context-lines diff options, syntax highlighting in diffs, and file-level
discard / ignore actions. Works in both frontends (Tauri app, VSCode extension) through
`RepoClient`.

Out of scope (separate specs): commit search (existing plan
`docs/superpowers/plans/2026-10-05-commit-search.md`, ships first on its own branch), file
history (batch 2b), undo stack, clone, signing (batch 3).

## Current state

- `git-core/diff.rs`: `working_diff(repo, path, staged)` and `commit_diff(repo, commit_id, path)`
  build `git2::DiffOptions` inline (literal pathspec; untracked content for the unstaged side) and
  convert via `hunks_from_diff`. `DiffLineOrigin` is `Add | Remove | Context`; libgit2's
  end-of-file markers (`=`, `>`, `<`) fall through to `Context`.
- `git-core/stage.rs`: `stage_file`, `unstage_file`, `stage_hunk`, `unstage_hunk`, `discard_hunk`.
  Hunks are identified by `(old_start, new_start)` and applied with `repo.apply` plus a
  `hunk_callback` filter. Each function rebuilds its `git2::DiffOptions` by hand to match
  `working_diff`.
- `frontend/src/components/DiffView.tsx`: unified/split toggle in a `Toolbar`; keyboard group
  with `[`/`]` (hunk nav), `s` (stage, or unstage in the staged view), `d`/`d` (armed discard);
  word-level diff via `lib/wordDiff.ts`. No line selection, no highlighting.
- `DiffPane.tsx` file section header: Blame, and a stage/unstage +/− button. No file menu,
  no discard-file, no ignore.
- `config` crate: global `config.toml` with atomic writes.
- Both webview CSPs (Tauri `tauri.conf.json`, VSCode `extension/src/webviewHtml.ts`) disallow
  `wasm-unsafe-eval`.

## Decisions

- **Line staging = synthesized patch applied with `repo.apply`.** Same apply path as hunk
  staging, so libgit2 keeps handling CRLF/filters, new files and missing EOF newline.
- **Diff options apply everywhere.** With ignore-whitespace on in the working tree, hunk and line
  actions are disabled (hint shown). File-level stage/discard stay enabled. Context-lines is
  allowed during staging because staging rebuilds the diff with the same value.
- **Diff settings are global**, in `config.toml`.
- **Syntax highlighting = Shiki with the JavaScript regex engine** (MIT, no WASM, no CSP change),
  lazy-loaded per language, colors from theme CSS variables.
- **Discard is a hard discard behind a confirm dialog.** No trash/backup (undo stack is batch 3).
- **Selection UX follows common desktop conventions**: click / Shift+click / drag / Ctrl-Cmd+click; existing
  `s` and `d`/`d` keys act on the selection when there is one, else the hunk.

## Backend (`git-core`)

### `diff.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiffOptions {
    pub ignore_whitespace: bool, // default false
    pub context_lines: u32,      // default 3, clamped to 0..=20
}

impl Default for DiffOptions { /* false, 3 */ }

pub fn working_diff(repo, path, staged, opts: &DiffOptions) -> Result<Vec<DiffHunk>, DiffError>
pub fn commit_diff(repo, commit_id, path, opts: &DiffOptions) -> Result<Vec<DiffHunk>, DiffError>

/// Single source of the git2 options used for working-tree diffs. `stage.rs` calls this too,
/// so the (old_start, new_start) hunk identity cannot drift between display and apply.
pub(crate) fn working_diff_options(path: &str, staged: bool, opts: &DiffOptions) -> git2::DiffOptions
```

`ignore_whitespace` maps to `git2::DiffOptions::ignore_whitespace(true)`; `context_lines` to
`context_lines(n)` after clamping.

### `stage.rs`

Existing `stage_hunk`, `unstage_hunk`, `discard_hunk` gain a `context_lines: u32` parameter and
build their diff via `working_diff_options` (ignore-whitespace always off).

New:

```rust
pub fn stage_lines(repo, path, old_start, new_start, context_lines, line_indices: &[u32]) -> Result<(), StageError>
pub fn unstage_lines(repo, path, old_start, new_start, context_lines, line_indices: &[u32]) -> Result<(), StageError>
pub fn discard_lines(repo, path, old_start, new_start, context_lines, line_indices: &[u32]) -> Result<(), StageError>
pub fn discard_file(repo, path) -> Result<(), StageError>
pub fn discard_all_unstaged(repo) -> Result<(), StageError>
pub fn ignore_path(repo, pattern: &str) -> Result<(), StageError>
```

**Line index contract.** `line_indices` index into the hunk's `lines` exactly as
`hunks_from_diff` emits them (the same array the frontend renders), for the forward diff the user
sees (index→workdir for stage/discard, HEAD→index for unstage).

**Patch synthesis** (shared private helper):

1. Recompute the forward diff with `working_diff_options(path, staged, {false, context_lines})`
   and find the hunk by `(old_start, new_start)`; missing ⇒ `HunkNotFound`.
2. Validate: every index in range, else `HunkNotFound`; any index on a context/EOF-marker line ⇒
   `ContextLineSelected`; no `+`/`-` line selected ⇒ `EmptySelection`.
3. Build the filtered hunk body:
   - **Stage** (forward patch, applied to `Index`): selected `+` kept; unselected `+` dropped;
     selected `-` kept; unselected `-` emitted as context (` `); context kept.
   - **Unstage** (applied to `Index`) and **discard** (applied to `WorkDir`): build the forward
     filtered patch from the *new* side's perspective — selected `+` kept; unselected `+` emitted
     as context; selected `-` kept; unselected `-` dropped — then reverse it (`+`↔`-`, swap
     header ranges) before applying.
   - The `\ No newline at end of file` marker follows the line it belongs to; it is kept iff
     that line is emitted.
4. Recount `@@ -a,b +c,d @@` from the emitted lines.
5. Emit file headers: `diff --git a/p b/p`, `--- a/p` / `+++ b/p`; `--- /dev/null` plus
   `new file mode` when the old side is absent (untracked file staging); mode taken from the
   delta.
6. `git2::Diff::from_buffer(patch)` and `repo.apply(&diff, location, None)`.

**`discard_file`**: tracked path ⇒ `checkout_index` with `force()` and `path(p)` (restores index
content; staged changes are untouched). Untracked path ⇒ `remove_file`, or `remove_dir_all` for an
untracked directory entry. Path must be inside the workdir (reject `..` / absolute) ⇒
`InvalidPath`.

**`discard_all_unstaged`**: same per unstaged entry from `status::status()`.

**`ignore_path`**: reject empty or newline-containing patterns (`InvalidPattern`). Open
`<workdir>/.gitignore` (create if missing). If the exact line already exists, no-op. Otherwise
append, inserting a leading `\n` when the file doesn't end with one.

New `StageError` variants (user-facing text):

- `EmptySelection`: "Select at least one added or removed line."
- `ContextLineSelected`: "Only added or removed lines can be staged."
- `InvalidPath`: "That path is outside the repository."
- `InvalidPattern`: "Ignore pattern can't be empty or span several lines."

### `config` crate

```rust
pub struct DiffSettings { pub ignore_whitespace: bool, pub context_lines: u32, pub syntax_highlight: bool }
// defaults: false, 3, true
pub fn get_diff_settings() / get_diff_settings_at(path)
pub fn set_diff_settings(&DiffSettings) / set_diff_settings_at(path, &DiffSettings)
```

Stored under a `[diff]` table in `config.toml`; missing table ⇒ defaults. Atomic write like the
rest of the crate.

## Transport

`repo-service` worker commands, Tauri commands and `vscode-sidecar` JSON-RPC methods for every new
or changed function. DTOs camelCase. `RepoClient`:

```ts
interface DiffOptions { ignoreWhitespace: boolean; contextLines: number }
interface DiffSettings extends DiffOptions { syntaxHighlight: boolean }

getWorkingDiff(repoPath, path, staged, opts: DiffOptions): Promise<DiffHunk[]>;
getCommitDiff(repoPath, commitId, path, opts: DiffOptions): Promise<DiffHunk[]>;
stageHunk(repoPath, path, oldStart, newStart, contextLines): Promise<void>;
unstageHunk(repoPath, path, oldStart, newStart, contextLines): Promise<void>;
discardHunk(repoPath, path, oldStart, newStart, contextLines): Promise<void>;
stageLines(repoPath, path, oldStart, newStart, contextLines, lineIndices: number[]): Promise<void>;
unstageLines(repoPath, path, oldStart, newStart, contextLines, lineIndices: number[]): Promise<void>;
discardLines(repoPath, path, oldStart, newStart, contextLines, lineIndices: number[]): Promise<void>;
discardFile(repoPath, path): Promise<void>;
discardAllUnstaged(repoPath): Promise<void>;
ignorePath(repoPath, pattern): Promise<void>;
getDiffSettings(): Promise<DiffSettings>;
setDiffSettings(settings: DiffSettings): Promise<void>;
```

## Frontend

### Line selection — `lib/lineSelection.ts`

Pure reducer, unit-tested on its own.

- State: `{ hunkIndex: number | null, anchor: number | null, cursor: number | null, selected: ReadonlySet<number> }`.
- Actions: `click(hunk, line)`, `shiftClick`, `toggle` (Ctrl/Cmd), `dragTo`, `moveCursor(±1)`,
  `extend(±1)`, `clear`. Context/EOF lines are never added; cursor skips them. Selection is
  confined to one hunk; acting in another hunk replaces it.
- Cleared when `hunks` changes (reset-during-render, like the existing discard-arm reset) and on
  `[`/`]`.

### `DiffView.tsx`

- Mouse: click, Shift+click, Ctrl/Cmd+click, drag on `+`/`-` rows. Split view: left cell selects
  the removed line, right cell the added line.
- Keyboard (focused diff group): `↑`/`↓` move the line cursor in the active hunk, `Shift+↑/↓`
  extend; `s` stages (unstages in staged view) the selection, or the hunk when nothing is
  selected; `d`/`d` the same for discard. `aria-keyshortcuts` updated.
- Hunk header buttons read "Stage lines (n)" / "Unstage lines (n)" / "Discard lines (n)" with a
  selection, hunk labels otherwise.
- Selected rows get `aria-selected="true"`; a polite live region announces "n lines selected".
- New props: `onStageLines?`, `onUnstageLines?`, `onDiscardLines?` `(oldStart, newStart, lineIndices)`,
  `partialActionsDisabled?: boolean` (hides hunk/line actions, shows
  "Turn off Ignore whitespace to stage partially."), `language?: string | null`,
  `syntaxHighlight: boolean`.
- `DiffView.tsx` is ~420 lines; selection logic lives in the reducer and token rendering in a
  small `DiffLineContent` component to keep it from growing much.

### Diff options — `state/useDiffSettings.ts`

- Loads `getDiffSettings()` once per app; exposes settings + `update(partial)` (optimistic, then
  `setDiffSettings`; reverts and shows `InlineError` on failure).
- `DiffView`'s `Toolbar` gains: "Ignore whitespace" toggle, context-lines stepper (−/+, 0–20,
  shows the value), "Syntax highlighting" toggle, alongside the existing split toggle.
- Changing `ignoreWhitespace`/`contextLines` refetches working and commit diffs with the new
  options; every hunk/line `RepoClient` call passes the current `contextLines`.
- Working-tree diffs with `ignoreWhitespace` on pass `partialActionsDisabled`.
- Palette commands: "Toggle ignore whitespace", "Increase context lines", "Decrease context
  lines", "Toggle syntax highlighting".

### Syntax highlighting — `lib/highlight.ts`

- Lazy singleton: `createHighlighterCore` + `createJavaScriptRegexEngine`, languages imported on
  demand from a fixed extension→language map (common languages only; unknown ⇒ no highlight).
- `highlightLines(lang, text): Promise<Token[][]>` — token = `{ content, scope }`; LRU cache keyed
  by `(lang, text)`.
- Per hunk, the old side (context + `-`) and the new side (context + `+`) are tokenized as
  separate continuous blocks, then mapped back onto rows.
- Scope → CSS class → `--syntax-*` variables defined in `lib/theme.ts` / theme CSS for light and
  dark.
- Skipped when the toggle is off or the file's diff exceeds 2,000 lines. Load or tokenize failure
  ⇒ plain text, logged via `RepoClient.logFrontendError`, no user-facing error.
- Layering: word-diff background underneath, token color on top.

### File actions — `FileActionsMenu.tsx`

- "⋯" button in each unstaged file section header, plus right-click and Shift+F10/Menu key on the
  header; built on the `ContextMenu` primitive.
- Items: "Discard changes…"; for untracked files also "Ignore file" (`/path`), "Ignore all
  `*.ext`" (when the file has an extension), "Ignore folder `dir/`" (when not at root).
- "Discard changes…" ⇒ `ConfirmDialog`: tracked "Discard changes to ⟨path⟩? This can't be
  undone."; untracked "Delete untracked file ⟨path⟩? This can't be undone."
- "Discard all unstaged…" button beside stage-all ⇒ `ConfirmDialog`: "Discard changes to n files
  (m untracked will be deleted)? This can't be undone."
- Palette: "Discard changes in current file", "Discard all unstaged changes".
- Wired through `useStagingActions`: `stageLines`, `unstageLines`, `discardLines`, `discardFile`,
  `discardAllUnstaged`, `ignorePath`; success toasts ("Discarded changes to ⟨path⟩", "Added
  ⟨pattern⟩ to .gitignore", "Staged n lines").
- Labels sentence case (FB-007).

## Error handling

- Backend errors surface as `error.to_string()` into the existing `InlineError`/toast paths.
- `HunkNotFound` after a stale view: refetch the diff (as hunk staging does today).
- Highlighting never surfaces errors to the user.

## Dependencies

- `shiki` and `@shikijs/engine-javascript` (MIT), exact version pinned ≥2 weeks old. Rows added to
  `docs/LICENSE_COMPLIANCE.md` in the same commit. No new Rust dependencies.

## Testing

- `git-core` (real temp repos): stage a subset of lines (only `+`, only `-`, mixed); stage lines
  of an untracked file; file without trailing newline; unstage lines; discard lines; stale hunk ⇒
  `HunkNotFound`; context/empty selection errors; context-lines option changes hunk boundaries and
  staging still matches; ignore-whitespace hides whitespace-only hunks; `discard_file` tracked,
  untracked, untracked dir, staged-and-modified (index kept); path escape rejected;
  `discard_all_unstaged`; `ignore_path` create / append / no trailing newline / duplicate /
  invalid pattern.
- `config`: diff settings round trip, defaults when missing.
- `tauri-app`: DTO serialization for `DiffOptions`/`DiffSettings`.
- Frontend (Vitest, mocking `RepoClient`): selection reducer; `DiffView` mouse and keyboard
  selection and the calls they make; toolbar options refetch with the right args; whitespace hint
  disables partial actions; `FileActionsMenu` items per file kind and confirm dialogs;
  highlight fallback renders plain text.
- E2E Tauri: stage 2 lines of a hunk, verify staged diff; discard a file via menu + confirm.
- E2E VSCode: stage lines flow.

## Docs

- `CHANGELOG.md` Unreleased entry.
- `docs/USER_GUIDE.md`: "Staging and committing" (line staging, discard, ignore), "Browsing
  history and diffs" (diff options, highlighting).
- Shortcut sheet: line cursor/selection keys.

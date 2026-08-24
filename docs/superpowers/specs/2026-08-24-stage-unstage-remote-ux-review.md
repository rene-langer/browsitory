# Stage/unstage + Add-remote UX review — actionable findings

Status: review only, no code changed. Written for an implementation agent to pick up.
Scope: `frontend/src/components/DiffPane.tsx` (uncommitted file list / stage-unstage) and
`frontend/src/components/RemotePanel.tsx` (remote list + "Add remote" form).

Context: per `CLAUDE.md`, Phase 5 reskinned the core commit-review loop (`CommitBox`,
`CommitGraph`, `DiffPane`/`DiffView`/`BlameView`) onto the design-token + primitive system
(`frontend/src/components/primitives/`: `Panel`, `SplitView`, `Toolbar`, `ListRow`,
`AccordionSection`). The rollout to the rest of the app is explicitly listed as not done yet.
The two areas below are the clearest unmigrated spots and also the two most-used destructive/
setup flows in the app (stage/unstage happens constantly; add-remote is the first thing a new
repo needs), so they're worth prioritizing next.

Note on "reskinned" claim in CLAUDE.md: `DiffPane.tsx` itself (the file list + stage/unstage
row) was **not** actually migrated even though `DiffView` (the hunk view it renders below the
list) was — see Evidence 1. Worth calling this out explicitly since it's easy to assume the
whole file is done because `DiffView` is.

## Evidence — current markup is unstyled native HTML

Method: read directly from the component source (JSX return blocks below compile ~1:1 to DOM;
CSS-module `className`s shown are the *only* styling hooks present — no others exist on these
elements). A live-rendered DOM dump was attempted via `@testing-library/react` but this
checkout has no `node_modules`/`pnpm` available in-session to run vitest; source is the
next-best evidence and is exact since JSX *is* the render output here (no runtime class logic
on these specific elements).

**Evidence 1 — `DiffPane.tsx:227-252`, the staged/unstaged file list.** Only class on the whole
list is `.fileList` (`list-style:none; margin:0; padding:0` — see
`DiffPane.module.css:1-5`, the entire file). Every row is:

```tsx
<li key={`${entry.staged}:${entry.path}`}>
  <button onClick={() => { setSelected(...); setViewMode(...); }}>
    {entry.path} ({entry.kind})
  </button>
  <button onClick={() => { setSelected(...); setViewMode("blame"); }}>Blame</button>
  {entry.staged
    ? <button onClick={() => onUnstageFile(entry.path)}>Unstage</button>
    : <button onClick={() => onStageFile(entry.path)}>Stage</button>}
</li>
```

Three unstyled `<button>` elements per row, browser-default appearance (platform font, default
padding, default focus outline), no icon, no hover state beyond UA default, no `className` at
all. No `ListRow` primitive, no status-kind icon, no selected-row highlight (`selected` state
exists at `DiffPane.tsx:132` but nothing reads it into a CSS class). Compare to `DiffView.tsx`
(the hunk pane rendered right below this list, in the same file's render tree), which *does*
use `styles.*` classes throughout — the row list is the one piece of this screen left behind.

**Evidence 2 — `RemotePanel.tsx:241-247`, the Add-remote form:**

```tsx
<form className={styles.form} onSubmit={submitAdd} aria-label="Add remote">
  <h3 className={styles.formHeading}>Add remote</h3>
  <label className={styles.label}>Remote name<input value={newName} onChange={...} /></label>
  <label className={styles.label}>Fetch URL<input data-testid="add-remote-fetch-url" value={newFetchUrl} onChange={...} /></label>
  <label className={styles.label}>Push URL (optional)<input value={newPushUrl} onChange={...} /></label>
  <button type="submit" disabled={fetchDisabled}>Add remote</button>
</form>
```

`.form` is just `display:flex; flex-wrap:wrap; gap:var(--space-2)` (`RemotePanel.module.css:20-26`).
No placeholder text on any of the three `<input>`s, no primary-button styling on submit (same
default browser button as every other button in the panel), no inline validation UI even
though the backend already rejects bad input (`crates/git-core/src/remote.rs:378-390`,
`validate_urls`) — a rejected submit currently only surfaces however `runMutation`/the app's
generic error path renders it, not next to the field.

**Evidence 3 — `RemotePanel.tsx:179-191`, one remote's action row:**

```tsx
<strong>{remote.name}</strong>
<span>Fetch: {remote.fetchUrl}</span>
{remote.pushUrl !== null && <span>Push: {remote.pushUrl}</span>}
<Toolbar>
  <button type="button" disabled={fetchDisabled} onClick={...}>Fetch {remote.name}</button>
  <button type="button" disabled={pushDisabled} onClick={...}>Push branch to {remote.name}</button>
  <button type="button" onClick={() => beginEdit(remote)}>Edit {remote.name}</button>
  <button type="button" onClick={() => beginCredentialEdit(remote)}>Credentials for {remote.name}</button>
  <button type="button" onClick={() => requestRemove(remote)}>Remove {remote.name}</button>
</Toolbar>
```

Five buttons, identical default styling — the destructive one ("Remove") is visually
indistinguishable from the safe ones (Fetch/Push/Edit/Credentials).

**Evidence 4 — no danger/error color token exists yet.** Full token list,
`frontend/src/styles/tokens.css:9-24`: `--color-bg`, `--color-bg-subtle`, `--color-surface`,
`--color-border`, `--color-text`, `--color-text-muted`, `--color-selected-bg`, `--color-accent`,
`--color-accent-text`, `--color-diff-add-*`, `--color-diff-remove-*`, `--color-scrim`. No
`--color-danger`/`--color-error` (light+dark). Needed before a destructive-button style can be
implemented per token convention — do not hardcode a red.

**Evidence 5 — `className=` count, migrated vs. not** (grep, `frontend/src/components/`):

| File | `className=` count | Uses a `primitives/*` component? |
|---|---|---|
| `DiffPane.tsx` | 2 | no |
| `RemotePanel.tsx` | 19 (via `AccordionSection`, `Toolbar`, and its own `.module.css`) | `AccordionSection`, `Toolbar` only — no `ListRow` |
| `CommitBox.tsx` (reskinned, reference) | 1 | `Panel`, `Toolbar` |
| `CommitGraph.tsx` (reskinned, reference) | 4 | uses `ListRow`-style row semantics |
| `DiffView.tsx` (reskinned, reference) | 2 | styled via `.module.css`, consistent with the rest of Phase 5 |

`RemotePanel` is partially migrated (accordion shell + toolbar rows use primitives); its list
rows and both forms are not. `DiffPane`'s file list is essentially unmigrated.

## Actionable changes

### A. `DiffPane.tsx` file list (stage/unstage)

1. Split `status` into two groups before rendering: staged (`entry.staged === true`) and
   unstaged, each under its own small heading ("Staged (N)" / "Changes (N)"), instead of one
   flat `<ul>` (`DiffPane.tsx:227-252`).
2. Replace the raw `<li><button>...` row with `ListRow` (`components/primitives/ListRow.tsx`),
   passing `selected={selected?.path === entry.path && selected?.staged === entry.staged}` so
   the currently-viewed file gets the primitive's built-in selected styling — this alone fixes
   "selected file not visually indicated."
3. Add a status-kind icon (lucide-react, already a project dependency per `CLAUDE.md`) mapped
   from `entry.kind` (`Modified`/`New`/`Deleted`/`Conflicted`/etc. — check
   `RepoClient.ts`'s `StatusEntry`/`StatusKind` union for the exact set and
   `commands.rs`'s test module for the wire strings). Put it left of the path.
4. Turn the stage/unstage `<button>` into an icon-only affordance (`+`/`−` or a checkbox glyph)
   that appears on row hover, not a permanently-visible text button — reduces per-row visual
   noise; keep it keyboard-reachable (icon button, not `:hover`-only CSS) since `ListRow`
   already gives the row itself a focus ring.
5. Add a "Stage all" button above the unstaged group and "Unstage all" above the staged group.
   Wire to new `onStageAll`/`onUnstageAll` callbacks — check whether `RepoClient` already has a
   bulk stage/unstage method before adding new IPC surface; if not, the plainest
   implementation is looping `onStageFile`/`onUnstageFile` client-side, but confirm there's no
   existing backend batch op first (`crates/git-core/src/stage.rs`) to avoid N separate IPC
   round-trips if a single-call version already exists or is cheap to add.
6. Keep "Blame" as a secondary action — fine as a small text/icon button, not the focus of this
   pass.
7. New/updated tests: extend `DiffPane.test.tsx` for the staged/unstaged grouping, the
   hover-reveal stage control (accessible via keyboard even without hover — assert the button
   is present and clickable, not just CSS-hidden), and selected-row `aria-selected`.

### B. `RemotePanel.tsx` — Add remote

1. Add `placeholder` text to all three inputs (`RemotePanel.tsx:243-245`), e.g.
   `git@github.com:user/repo.git` on Fetch URL.
2. Auto-derive the remote name from the URL when the name field is still empty at submit time
   (e.g. `origin` as the default suggestion, or parse the repo slug) — reduces the common case
   to one field. Keep the name field editable/visible, just prefill it live via `onChange` of
   the URL field when `newName === ""`.
3. Move "Push URL (optional)" behind a disclosure (`<details>` or reuse `AccordionSection`) —
   collapsed by default. Most remotes don't need a separate push URL.
4. Give the submit button primary styling (new/reused button variant using `--color-accent`) —
   distinct from "Cancel"/"Remove"-class buttons. This needs a button-variant convention if one
   doesn't already exist in `primitives/`; check `Toolbar.tsx` and its `.module.css` first
   before inventing a new one.
5. Inline validation: `onAddRemote` (`RemotePanel.tsx:35`, wired to `add_remote` /
   `validate_urls` in `crates/git-core/src/remote.rs:378-390`) rejects malformed URLs — catch
   the rejection in `submitAdd` (`RemotePanel.tsx:72-81`) and render the message next to the
   Fetch URL field instead of only whatever the app-wide error path currently shows.
6. Empty state (`RemotePanel.tsx:155-157`, `remotes.length === 0`): replace the plain
   `<p>No remotes configured.</p>` with a prompt that visually points at the Add-remote form
   below it (e.g. "Add a remote to push and pull." styled as a callout, or move the Add-remote
   form above the (empty) list when there are zero remotes so the user isn't scrolling past
   nothing to find it).

### C. `RemotePanel.tsx` — per-remote action row

1. Add a `--color-danger`/`--color-danger-text` token pair to `frontend/src/styles/tokens.css`
   (light block `:root`, dark blocks at lines 59-79 and 81-99 — all three need it, same pattern
   as every existing token). Pick colors consistent with the existing diff-remove palette
   (`--color-diff-remove-text: #b31d28` light / `#f87171` dark) rather than inventing new hues.
2. Style "Remove {remote.name}" (`RemotePanel.tsx:188`) with the new danger token — text color
   at minimum, optionally a subtle danger-tinted hover background matching the existing
   `--color-diff-remove-bg` pattern.
3. Optional (lower priority): icon-ify the five toolbar buttons
   (`RemotePanel.tsx:184-188`) using lucide-react icons with `aria-label`/tooltip, matching
   whatever icon-button pattern (if any) exists elsewhere in the reskinned components — check
   `CommitGraph.tsx`/`CommitBox.tsx` for precedent before inventing one.

## Order of implementation

B and C.1-2 (add-remote + danger token) are self-contained and low-risk — do first. A (file
list) is the larger change since it touches `ListRow` integration and possibly new IPC for bulk
stage/unstage — confirm scope of A.5 (does a bulk backend op exist) before starting, per this
repo's `test-driven-development` convention: write the failing test for the new grouped/hover
behavior first, then implement.

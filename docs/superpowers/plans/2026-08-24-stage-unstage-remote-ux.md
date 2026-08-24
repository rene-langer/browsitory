# Stage/Unstage + Add-Remote UX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Migrate `DiffPane.tsx`'s uncommitted file list and `RemotePanel.tsx`'s add-remote/
remove-remote UI onto the Phase 5 design-token + primitive system, closing the gaps identified
in the review.

**Architecture:** No IPC/backend changes — `crates/git-core::stage` has no bulk stage/unstage op
and none is added; "Stage all"/"Unstage all" loop the existing per-file `onStageFile`/
`onUnstageFile` callbacks client-side. All changes are `frontend/src/components/DiffPane.tsx`,
`frontend/src/components/RemotePanel.tsx`, their `.module.css` files, and
`frontend/src/styles/tokens.css` (new danger color tokens). Follows the established primitive
patterns already in the reskinned components: `ListRow` with container-owned selection +
listbox arrow-key nav (`CommitGraph.tsx`'s pattern) for `DiffPane`'s file list, `<details>` for
a collapsed optional field, and a plain literal class name alongside the CSS-module class
wherever a test needs to assert a styling variant (`DiffView.tsx`'s `diff-line-add`/
`diff-line-remove` pattern) rather than asserting hashed module class names.

**Tech Stack:** React 18 + TypeScript, CSS Modules, `lucide-react` icons, Vitest +
`@testing-library/react`.

**Spec:** `docs/superpowers/specs/2026-08-24-stage-unstage-remote-ux-review.md`

## Global Constraints

- No backend/IPC changes — confirmed no bulk stage/unstage exists in `crates/git-core/src/stage.rs`
  and none is needed (`RepoClient.stageFile`/`unstageFile` already take one path; loop them).
- No new hardcoded colors — any new color must be a token in `frontend/src/styles/tokens.css`,
  defined in all three blocks (light `:root`, dark media query, dark `[data-theme="dark"]`).
- `frontend/src/components` may not import `@tauri-apps/*` directly (ESLint-enforced) — not
  relevant here since no task touches IPC, but do not add such an import.
- Every new/changed interactive element needs an accessible name (`aria-label` for icon-only
  buttons) — several steps below add icon-only buttons.
- Run `cd frontend && pnpm test -- --run <File>.test.tsx` after each step; run the full
  `cd frontend && pnpm test -- --run` and `cd frontend && pnpm lint` at the end of each task.

---

### Task 1: Danger color tokens

**Files:**
- Modify: `frontend/src/styles/tokens.css:9-24` (light block), `:59-79` (dark media query
  block), `:81-99` (explicit dark block)

**Interfaces:**
- Produces: CSS custom properties `--color-danger` and `--color-danger-text`, consumable by any
  component's CSS module via `var(--color-danger)` / `var(--color-danger-text)`.

This task has no test file of its own (it's a pure CSS token addition, verified visually by the
components that consume it in Tasks 2 and 3) — skip the standard test-first steps and go
straight to the change, then verify with a build.

- [ ] **Step 1: Add the danger tokens to all three blocks**

In `frontend/src/styles/tokens.css`, add to the light `:root` block (after
`--color-diff-remove-text: #b31d28;` on line 22):

```css
  --color-danger: #ffeef0;
  --color-danger-text: #b31d28;
```

Add to the `@media (prefers-color-scheme: dark) { :root:not([data-theme="light"]) { ... } }`
block (after `--color-diff-remove-text: #f87171;`):

```css
    --color-danger: #2b1013;
    --color-danger-text: #f87171;
```

Add to the `:root[data-theme="dark"] { ... }` block (same values, same position):

```css
  --color-danger: #2b1013;
  --color-danger-text: #f87171;
```

These match the existing `--color-diff-remove-bg`/`--color-diff-remove-text` values exactly
(per the spec's instruction to reuse the existing diff-remove palette rather than invent a new
hue).

- [ ] **Step 2: Verify the frontend still builds**

Run: `cd frontend && pnpm build`
Expected: builds with no CSS or type errors.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/styles/tokens.css
git commit -m "feat(tokens): add --color-danger and --color-danger-text"
```

---

### Task 2: `RemotePanel.tsx` — danger-styled Remove button

**Files:**
- Modify: `frontend/src/components/RemotePanel.tsx:188`
- Modify: `frontend/src/components/RemotePanel.module.css`
- Test: `frontend/src/components/RemotePanel.test.tsx`

**Interfaces:**
- Consumes: `--color-danger` / `--color-danger-text` tokens from Task 1.
- Produces: nothing new consumed by later tasks — this task is self-contained.

- [ ] **Step 1: Write the failing test**

Add to `frontend/src/components/RemotePanel.test.tsx` (inside the `describe("RemotePanel", ...)`
block, near the other remove-button tests):

```tsx
  it("styles the Remove button with the danger variant", () => {
    renderPanel({});

    expect(screen.getByRole("button", { name: "Remove origin" })).toHaveClass("danger");
  });
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd frontend && pnpm test -- --run RemotePanel.test.tsx`
Expected: FAIL — the "Remove origin" button has no `danger` class yet.

- [ ] **Step 3: Add the CSS class**

In `frontend/src/components/RemotePanel.module.css`, add:

```css
.dangerButton {
  color: var(--color-danger-text);
}

.dangerButton:hover:not(:disabled) {
  background: var(--color-danger);
  border-color: var(--color-danger-text);
}
```

- [ ] **Step 4: Apply the class**

In `frontend/src/components/RemotePanel.tsx`, change line 188 from:

```tsx
                    <button type="button" onClick={() => requestRemove(remote)}>Remove {remote.name}</button>
```

to:

```tsx
                    <button
                      type="button"
                      className={`${styles.dangerButton} danger`}
                      onClick={() => requestRemove(remote)}
                    >
                      Remove {remote.name}
                    </button>
```

(The plain `danger` string alongside the CSS-module class is the same pattern
`DiffView.tsx:71` uses for `diff-line-add`/`diff-line-remove` — a stable, non-hashed hook for
tests, matching this codebase's existing convention rather than asserting on the hashed module
class name.)

- [ ] **Step 5: Run test to verify it passes**

Run: `cd frontend && pnpm test -- --run RemotePanel.test.tsx`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/RemotePanel.tsx frontend/src/components/RemotePanel.module.css frontend/src/components/RemotePanel.test.tsx
git commit -m "feat(remote-panel): style Remove button with the danger token"
```

---

### Task 3: `RemotePanel.tsx` — Add-remote UX

**Files:**
- Modify: `frontend/src/components/RemotePanel.tsx:56-81` (state/`submitAdd`), `:241-247`
  (Add-remote form), `:155-157` (empty state)
- Modify: `frontend/src/components/RemotePanel.module.css`
- Test: `frontend/src/components/RemotePanel.test.tsx`

**Interfaces:**
- Consumes: `RemoteInfo[]` (`remotes` prop, already present), `onAddRemote: (name, fetchUrl,
  pushUrl) => Promise<void>` (already present — this task is the first caller that handles its
  rejection).
- Produces: nothing consumed by later tasks — self-contained.

This task bundles six small, tightly related changes to the same form (placeholders,
auto-derive, disclosure, primary button, inline validation, empty state) as one task rather than
six, since they all touch the same ~15 lines of JSX and splitting them would mean repeatedly
re-deriving the same render block.

- [ ] **Step 1: Write the failing tests**

Add to `frontend/src/components/RemotePanel.test.tsx`, inside `describe("RemotePanel", ...)`:

```tsx
  it("shows example placeholder text on the add-remote URL fields", () => {
    renderPanel({});

    expect(screen.getByLabelText("Fetch URL")).toHaveAttribute(
      "placeholder",
      "git@github.com:user/repo.git",
    );
  });

  it("auto-derives the remote name as origin when no remote is named origin yet", () => {
    renderPanel({ remotes: [] });

    fireEvent.change(screen.getByLabelText("Fetch URL"), {
      target: { value: "git@github.com:user/repo.git" },
    });

    expect(screen.getByLabelText("Remote name")).toHaveValue("origin");
  });

  it("auto-derives the remote name from the URL slug when origin already exists", () => {
    renderPanel({});

    fireEvent.change(screen.getByLabelText("Fetch URL"), {
      target: { value: "git@github.com:user/repo.git" },
    });

    expect(screen.getByLabelText("Remote name")).toHaveValue("repo");
  });

  it("does not overwrite a manually entered remote name when the URL changes", () => {
    renderPanel({});

    fireEvent.change(screen.getByLabelText("Remote name"), { target: { value: "custom" } });
    fireEvent.change(screen.getByLabelText("Fetch URL"), {
      target: { value: "git@github.com:user/repo.git" },
    });

    expect(screen.getByLabelText("Remote name")).toHaveValue("custom");
  });

  it("keeps the Push URL field collapsed behind a disclosure by default", () => {
    renderPanel({});

    expect(screen.queryByLabelText("Push URL")).not.toBeInTheDocument();

    fireEvent.click(screen.getByText("Push URL (optional)"));

    expect(screen.getByLabelText("Push URL")).toBeInTheDocument();
  });

  it("gives the Add remote submit button primary styling", () => {
    renderPanel({});

    expect(screen.getByRole("button", { name: "Add remote" })).toHaveClass("primary");
  });

  it("shows a rejected add-remote error next to the Fetch URL field and keeps the entered values", async () => {
    const onAddRemote = vi.fn().mockRejectedValue(new Error("invalid fetch URL"));
    renderPanel({ onAddRemote });

    fireEvent.change(screen.getByLabelText("Fetch URL"), { target: { value: "not-a-url" } });
    fireEvent.click(screen.getByRole("button", { name: "Add remote" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("invalid fetch URL");
    expect(screen.getByLabelText("Fetch URL")).toHaveValue("not-a-url");
  });

  it("shows a callout pointing at the Add-remote form when there are no remotes", () => {
    renderPanel({ remotes: [], remoteUpstreams: {} });

    expect(screen.getByText(/add a remote below to push and pull/i)).toBeInTheDocument();
  });
```

Update the existing test at line 227-237 (`"adds a remote using the labelled URL form"`) — it
fills the Push URL field, which is now collapsed by default:

```tsx
  it("adds a remote using the labelled URL form", () => {
    const onAddRemote = vi.fn();
    renderPanel({ onAddRemote });

    fireEvent.change(screen.getByLabelText("Remote name"), { target: { value: "backup" } });
    fireEvent.change(screen.getByLabelText("Fetch URL"), { target: { value: "../backup.git" } });
    fireEvent.click(screen.getByText("Push URL (optional)"));
    fireEvent.change(screen.getByLabelText("Push URL"), { target: { value: "../push-backup.git" } });
    fireEvent.click(screen.getByRole("button", { name: "Add remote" }));

    expect(onAddRemote).toHaveBeenCalledWith("backup", "../backup.git", "../push-backup.git");
  });
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && pnpm test -- --run RemotePanel.test.tsx`
Expected: FAIL — placeholders, auto-derive, disclosure, primary class, inline error, and empty
callout don't exist yet; the updated add-remote test fails because Push URL is still always
visible with the old label text.

- [ ] **Step 3: Add the CSS classes**

In `frontend/src/components/RemotePanel.module.css`, add:

```css
.primaryButton {
  background: var(--color-accent);
  color: var(--color-accent-text);
  border-color: var(--color-accent);
}

.primaryButton:hover:not(:disabled) {
  background: var(--color-accent);
  opacity: 0.9;
}

.fieldError {
  flex-basis: 100%;
  margin: 0;
  color: var(--color-danger-text);
  font-size: var(--text-sm);
}

.emptyState {
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-sm);
  background: var(--color-bg-subtle);
  color: var(--color-text-muted);
}

.disclosure {
  flex-basis: 100%;
}

.disclosure summary {
  cursor: pointer;
  font-size: var(--text-sm);
  color: var(--color-text-muted);
}
```

- [ ] **Step 4: Add the name-derivation helper**

In `frontend/src/components/RemotePanel.tsx`, add this function above the `RemotePanel` export
(after the imports):

```tsx
function deriveRemoteName(fetchUrl: string, existingNames: string[]): string {
  if (!existingNames.includes("origin")) return "origin";
  const withoutGitSuffix = fetchUrl.replace(/\.git\/?$/, "");
  const slug = withoutGitSuffix
    .split(/[/:]/)
    .filter((part) => part !== "")
    .pop();
  return slug ?? "";
}
```

- [ ] **Step 5: Wire auto-derive into the Fetch URL field and add the error state**

In `frontend/src/components/RemotePanel.tsx`, add a new piece of state near the other
add-remote state (after `const [newPushUrl, setNewPushUrl] = useState("");` around line 60):

```tsx
  const [addError, setAddError] = useState<string | null>(null);
```

Replace `submitAdd` (lines 72-81) with:

```tsx
  const submitAdd = async (event: React.FormEvent) => {
    event.preventDefault();
    setAddError(null);
    const fetchUrl = newFetchUrl.trim();
    const name = (newName.trim() || deriveRemoteName(fetchUrl, remotes.map((remote) => remote.name))).trim();
    if (name === "" || fetchUrl === "") return;
    try {
      await onAddRemote(name, fetchUrl, newPushUrl.trim() || null);
      setNewName("");
      setNewFetchUrl("");
      setNewPushUrl("");
    } catch (err) {
      setAddError(String(err));
    }
  };
```

- [ ] **Step 6: Replace the Add-remote form**

In `frontend/src/components/RemotePanel.tsx`, replace lines 241-247:

```tsx
      <form className={styles.form} onSubmit={submitAdd} aria-label="Add remote">
        <h3 className={styles.formHeading}>Add remote</h3>
        <label className={styles.label}>Remote name<input value={newName} onChange={(event) => setNewName(event.target.value)} /></label>
        <label className={styles.label}>Fetch URL<input data-testid="add-remote-fetch-url" value={newFetchUrl} onChange={(event) => setNewFetchUrl(event.target.value)} /></label>
        <label className={styles.label}>Push URL (optional)<input value={newPushUrl} onChange={(event) => setNewPushUrl(event.target.value)} /></label>
        <button type="submit" disabled={fetchDisabled}>Add remote</button>
      </form>
```

with:

```tsx
      <form className={styles.form} onSubmit={submitAdd} aria-label="Add remote">
        <h3 className={styles.formHeading}>Add remote</h3>
        <label className={styles.label}>
          Remote name
          <input placeholder="origin" value={newName} onChange={(event) => setNewName(event.target.value)} />
        </label>
        <label className={styles.label}>
          Fetch URL
          <input
            data-testid="add-remote-fetch-url"
            placeholder="git@github.com:user/repo.git"
            value={newFetchUrl}
            onChange={(event) => {
              const value = event.target.value;
              setNewFetchUrl(value);
              if (newName === "") {
                setNewName(deriveRemoteName(value, remotes.map((remote) => remote.name)));
              }
            }}
          />
        </label>
        {addError !== null && (
          <p role="alert" className={styles.fieldError}>
            {addError}
          </p>
        )}
        <details className={styles.disclosure}>
          <summary>Push URL (optional)</summary>
          <label className={styles.label}>
            Push URL
            <input
              placeholder="git@github.com:user/repo.git"
              value={newPushUrl}
              onChange={(event) => setNewPushUrl(event.target.value)}
            />
          </label>
        </details>
        <button type="submit" className={`${styles.primaryButton} primary`} disabled={fetchDisabled}>
          Add remote
        </button>
      </form>
```

- [ ] **Step 7: Replace the empty state**

In `frontend/src/components/RemotePanel.tsx`, replace line 156:

```tsx
        <p>No remotes configured.</p>
```

with:

```tsx
        <p className={styles.emptyState}>Add a remote below to push and pull.</p>
```

- [ ] **Step 8: Run tests to verify they pass**

Run: `cd frontend && pnpm test -- --run RemotePanel.test.tsx`
Expected: PASS — all new and updated tests, and every pre-existing test in the file.

- [ ] **Step 9: Lint**

Run: `cd frontend && pnpm lint`
Expected: no errors.

- [ ] **Step 10: Commit**

```bash
git add frontend/src/components/RemotePanel.tsx frontend/src/components/RemotePanel.module.css frontend/src/components/RemotePanel.test.tsx
git commit -m "feat(remote-panel): improve add-remote UX (placeholders, auto-derive, disclosure, inline validation)"
```

---

### Task 4: `DiffPane.tsx` — staged/unstaged grouping + `ListRow` migration + status icons

**Files:**
- Modify: `frontend/src/components/DiffPane.tsx:1-16` (imports), `:132-321`
  (`UncommittedDiffPane`)
- Modify: `frontend/src/components/DiffPane.module.css`
- Test: `frontend/src/components/DiffPane.test.tsx`

**Interfaces:**
- Consumes: `ListRow` (`frontend/src/components/primitives/ListRow.tsx`) — `selected?: boolean`,
  `onClick?: () => void`, `className?: string`, `children: ReactNode`. Passing `selected` (even
  `false`, never `undefined`) puts `ListRow` in "container owns selection" mode: `role="option"`,
  no individual `tabIndex` — the *container* must be the tab stop and own arrow-key navigation
  (see `ListRow.tsx`'s doc comment and `CommitGraph.tsx:72-94` for the established pattern this
  task follows).
- Produces: a `FileListRow` component (this file, module-private) that Task 5 extends with
  icon-only stage/unstage controls — its prop shape changes in Task 5, so nothing outside this
  file should depend on it.

This task does **not** touch the Stage/Unstage buttons' text or behavior — they keep their
exact current text-button form, just relocated inside the new grouped/`ListRow` structure. Task
5 converts them to icon-only hover controls. This keeps this task's diff reviewable on its own
and keeps all of `DiffPane.test.tsx`'s existing Stage/Unstage-button assertions passing
unchanged through this task.

- [ ] **Step 1: Write the failing tests**

Add to `frontend/src/components/DiffPane.test.tsx`, inside `describe("uncommitted", ...)`:

```tsx
    it("groups unstaged and staged entries under labelled headings with counts", () => {
      const client = fakeClient({});

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      expect(screen.getByText("Changes (1)")).toBeInTheDocument();
      expect(screen.getByText("Staged (1)")).toBeInTheDocument();
    });

    it("marks the selected file's row as aria-selected", async () => {
      const client = fakeClient({ getWorkingDiff: async () => [] });

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      const row = screen.getByText("a.txt (Modified)").closest('[role="option"]');
      expect(row).toHaveAttribute("aria-selected", "false");

      fireEvent.click(screen.getByText("a.txt (Modified)"));
      await screen.findByText(/no changes/i).catch(() => {});

      expect(row).toHaveAttribute("aria-selected", "true");
    });

    it("renders a status-kind icon for each file row", () => {
      const client = fakeClient({});

      const { container } = render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      // One status icon per file row (2 entries in `status`).
      expect(container.querySelectorAll("svg").length).toBeGreaterThanOrEqual(2);
    });

    it("navigates the unstaged group with ArrowDown/ArrowUp and updates the diff", async () => {
      const twoUnstaged: StatusEntry[] = [
        { path: "a.txt", staged: false, kind: "Modified" },
        { path: "c.txt", staged: false, kind: "New" },
      ];
      const getWorkingDiff = vi.fn(async (_repoPath: string, path: string) => [
        { oldStart: 1, oldLines: 1, newStart: 1, newLines: 1, lines: [{ origin: "Context" as const, content: path }] },
      ]);
      const client = fakeClient({ getWorkingDiff });

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={twoUnstaged}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      fireEvent.click(screen.getByText("a.txt (Modified)"));
      await screen.findByText("a.txt");

      fireEvent.keyDown(screen.getByRole("listbox", { name: "Unstaged changes" }), { key: "ArrowDown" });

      expect(await screen.findByText("c.txt")).toBeInTheDocument();
    });
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && pnpm test -- --run DiffPane.test.tsx`
Expected: FAIL — no "Changes (N)"/"Staged (N)" headings, no `role="option"` rows, no listbox
named "Unstaged changes" exist yet.

- [ ] **Step 3: Add the CSS**

In `frontend/src/components/DiffPane.module.css`, replace the file's contents with:

```css
.fileList {
  list-style: none;
  margin: 0;
  padding: 0;
}

.groupHeading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--space-1) var(--space-3);
  font-size: var(--text-sm);
  font-weight: 600;
  color: var(--color-text-muted);
}

.fileRow {
  justify-content: flex-start;
}

.statusIcon {
  flex-shrink: 0;
  color: var(--color-text-muted);
}

.path {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.rowActions {
  display: flex;
  align-items: center;
  gap: var(--space-1);
  margin-left: auto;
}
```

- [ ] **Step 4: Add the imports**

In `frontend/src/components/DiffPane.tsx`, replace the top of the file (lines 1-16) with:

```tsx
import { AlertTriangle, ArrowRightLeft, FileDiff, FileMinus, FilePlus, Pencil, type LucideIcon } from "lucide-react";
import { useEffect, useState, type KeyboardEvent } from "react";
import type {
  BlameLine,
  DiffHunk,
  FileConflictChoice,
  RepoClient,
  StatusEntry,
  StatusKind,
} from "../ipc/RepoClient";
import type { SelectedRow } from "../state/useAppState";
import { BlameView } from "./BlameView";
import { CommitBox } from "./CommitBox";
import { ConflictResolutionPane } from "./ConflictResolutionPane";
import { DiffView } from "./DiffView";
import styles from "./DiffPane.module.css";
import { ListRow } from "./primitives/ListRow";
import { RebaseProgressPanel } from "./RebaseProgressPanel";
```

- [ ] **Step 5: Add the status-icon map and `FileListRow` component**

In `frontend/src/components/DiffPane.tsx`, add above the `DiffPane` export:

```tsx
const STATUS_ICONS: Record<StatusKind, LucideIcon> = {
  New: FilePlus,
  Modified: Pencil,
  Deleted: FileMinus,
  Renamed: ArrowRightLeft,
  TypeChange: FileDiff,
  Conflicted: AlertTriangle,
};

function FileListRow({
  entry,
  selected,
  onSelect,
  onBlame,
  onStageFile,
  onUnstageFile,
}: {
  entry: StatusEntry;
  selected: boolean;
  onSelect: () => void;
  onBlame: () => void;
  onStageFile: (path: string) => void;
  onUnstageFile: (path: string) => void;
}) {
  const Icon = STATUS_ICONS[entry.kind];
  return (
    <ListRow selected={selected} onClick={onSelect} className={styles.fileRow}>
      <Icon size={14} className={styles.statusIcon} aria-hidden="true" />
      <span className={styles.path}>
        {entry.path} ({entry.kind})
      </span>
      <div className={styles.rowActions}>
        <button
          type="button"
          onClick={(event) => {
            event.stopPropagation();
            onBlame();
          }}
        >
          Blame
        </button>
        {entry.staged ? (
          <button
            type="button"
            onClick={(event) => {
              event.stopPropagation();
              onUnstageFile(entry.path);
            }}
          >
            Unstage
          </button>
        ) : (
          <button
            type="button"
            onClick={(event) => {
              event.stopPropagation();
              onStageFile(entry.path);
            }}
          >
            Stage
          </button>
        )}
      </div>
    </ListRow>
  );
}
```

- [ ] **Step 6: Replace the file list markup and add grouping/selection/keyboard-nav logic**

In `frontend/src/components/DiffPane.tsx`, inside `UncommittedDiffPane`, add these
computations right after the existing `const stagedCount = ...` / `const displayedHunks = ...`
/ `const displayedBlameLines = ...` lines (around line 221-223):

```tsx
  const stagedEntries = status.filter((entry) => entry.staged);
  const unstagedEntries = status.filter((entry) => !entry.staged);

  const isEntrySelected = (entry: StatusEntry) =>
    selected !== null && selected.path === entry.path && selected.staged === entry.staged;

  const selectEntry = (entry: StatusEntry) => {
    setSelected({ path: entry.path, staged: entry.staged });
    setViewMode(entry.kind === "Conflicted" ? "conflict" : "diff");
  };

  const blameEntry = (entry: StatusEntry) => {
    setSelected({ path: entry.path, staged: entry.staged });
    setViewMode("blame");
  };

  const navigateGroup = (entries: StatusEntry[], direction: 1 | -1) => {
    if (entries.length === 0) return;
    const currentIndex = entries.findIndex(
      (entry) => selected !== null && entry.path === selected.path && entry.staged === selected.staged,
    );
    const nextIndex =
      currentIndex === -1 ? 0 : Math.min(Math.max(currentIndex + direction, 0), entries.length - 1);
    selectEntry(entries[nextIndex]);
  };

  const handleGroupKeyDown = (entries: StatusEntry[]) => (event: KeyboardEvent<HTMLUListElement>) => {
    if (event.key === "ArrowDown" || event.key === "j") {
      event.preventDefault();
      navigateGroup(entries, 1);
    } else if (event.key === "ArrowUp" || event.key === "k") {
      event.preventDefault();
      navigateGroup(entries, -1);
    }
  };
```

Then replace the file-list block (lines 227-253):

```tsx
      <ul className={styles.fileList}>
        {status.map((entry) => (
          <li key={`${entry.staged}:${entry.path}`}>
            <button
              onClick={() => {
                setSelected({ path: entry.path, staged: entry.staged });
                setViewMode(entry.kind === "Conflicted" ? "conflict" : "diff");
              }}
            >
              {entry.path} ({entry.kind})
            </button>
            <button
              onClick={() => {
                setSelected({ path: entry.path, staged: entry.staged });
                setViewMode("blame");
              }}
            >
              Blame
            </button>
            {entry.staged ? (
              <button onClick={() => onUnstageFile(entry.path)}>Unstage</button>
            ) : (
              <button onClick={() => onStageFile(entry.path)}>Stage</button>
            )}
          </li>
        ))}
      </ul>
```

with:

```tsx
      {unstagedEntries.length > 0 && (
        <div>
          <div className={styles.groupHeading}>
            <span>Changes ({unstagedEntries.length})</span>
          </div>
          <ul
            className={styles.fileList}
            role="listbox"
            aria-label="Unstaged changes"
            tabIndex={0}
            onKeyDown={handleGroupKeyDown(unstagedEntries)}
          >
            {unstagedEntries.map((entry) => (
              <FileListRow
                key={`${entry.staged}:${entry.path}`}
                entry={entry}
                selected={isEntrySelected(entry)}
                onSelect={() => selectEntry(entry)}
                onBlame={() => blameEntry(entry)}
                onStageFile={onStageFile}
                onUnstageFile={onUnstageFile}
              />
            ))}
          </ul>
        </div>
      )}
      {stagedEntries.length > 0 && (
        <div>
          <div className={styles.groupHeading}>
            <span>Staged ({stagedEntries.length})</span>
          </div>
          <ul
            className={styles.fileList}
            role="listbox"
            aria-label="Staged changes"
            tabIndex={0}
            onKeyDown={handleGroupKeyDown(stagedEntries)}
          >
            {stagedEntries.map((entry) => (
              <FileListRow
                key={`${entry.staged}:${entry.path}`}
                entry={entry}
                selected={isEntrySelected(entry)}
                onSelect={() => selectEntry(entry)}
                onBlame={() => blameEntry(entry)}
                onStageFile={onStageFile}
                onUnstageFile={onUnstageFile}
              />
            ))}
          </ul>
        </div>
      )}
```

(Unstaged renders before staged so the DOM order of the two-entry `status` fixture used
throughout the existing test file — `a.txt` unstaged, `b.txt` staged — is unchanged, keeping
every pre-existing `getAllByText("Blame")[0]`-style assertion pointed at the same file it was
before.)

- [ ] **Step 7: Run tests to verify they pass**

Run: `cd frontend && pnpm test -- --run DiffPane.test.tsx`
Expected: PASS — all new tests and every pre-existing test in the file (Stage/Unstage buttons
are untouched in this task, so those assertions still match).

- [ ] **Step 8: Lint**

Run: `cd frontend && pnpm lint`
Expected: no errors.

- [ ] **Step 9: Commit**

```bash
git add frontend/src/components/DiffPane.tsx frontend/src/components/DiffPane.module.css frontend/src/components/DiffPane.test.tsx
git commit -m "feat(diff-pane): group staged/unstaged files, migrate file list to ListRow with status icons"
```

---

### Task 5: `DiffPane.tsx` — icon-only stage/unstage controls + Stage all/Unstage all

**Files:**
- Modify: `frontend/src/components/DiffPane.tsx` (the `FileListRow` component and
  `UncommittedDiffPane` from Task 4)
- Modify: `frontend/src/components/DiffPane.module.css`
- Test: `frontend/src/components/DiffPane.test.tsx`

**Interfaces:**
- Consumes: `FileListRow`, `stagedEntries`/`unstagedEntries`, and the group `<ul>` structure
  produced by Task 4.
- Produces: nothing consumed elsewhere — this is the final task.

- [ ] **Step 1: Write the failing tests**

In `frontend/src/components/DiffPane.test.tsx`, replace the existing test
`"renders a Stage button for unstaged entries and Unstage for staged ones"` (around line 104)
with:

```tsx
    it("renders an icon-only Stage control for unstaged entries and Unstage for staged ones", () => {
      const client = fakeClient({});

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      expect(screen.getByRole("button", { name: "Stage a.txt" })).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Unstage b.txt" })).toBeInTheDocument();
    });
```

Replace the existing test `"clicking Stage calls onStageFile with that path"` (around line 246)
with:

```tsx
    it("clicking the Stage control calls onStageFile with that path", () => {
      const client = fakeClient({});
      const onStageFile = vi.fn();

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={onStageFile}
          onUnstageFile={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      fireEvent.click(screen.getByRole("button", { name: "Stage a.txt" }));

      expect(onStageFile).toHaveBeenCalledWith("a.txt");
    });
```

Add two new tests for Stage all/Unstage all, in the same `describe("uncommitted", ...)` block:

```tsx
    it("Stage all calls onStageFile for every unstaged entry", () => {
      const threeStatus: StatusEntry[] = [
        { path: "a.txt", staged: false, kind: "Modified" },
        { path: "c.txt", staged: false, kind: "New" },
        { path: "b.txt", staged: true, kind: "New" },
      ];
      const client = fakeClient({});
      const onStageFile = vi.fn();

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={threeStatus}
          onStageFile={onStageFile}
          onUnstageFile={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      fireEvent.click(screen.getByRole("button", { name: "Stage all" }));

      expect(onStageFile).toHaveBeenCalledWith("a.txt");
      expect(onStageFile).toHaveBeenCalledWith("c.txt");
      expect(onStageFile).toHaveBeenCalledTimes(2);
    });

    it("Unstage all calls onUnstageFile for every staged entry", () => {
      const client = fakeClient({});
      const onUnstageFile = vi.fn();

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={onUnstageFile}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      fireEvent.click(screen.getByRole("button", { name: "Unstage all" }));

      expect(onUnstageFile).toHaveBeenCalledWith("b.txt");
      expect(onUnstageFile).toHaveBeenCalledTimes(1);
    });

    it("does not render Stage all when there are no unstaged entries", () => {
      const stagedOnly: StatusEntry[] = [{ path: "b.txt", staged: true, kind: "New" }];
      const client = fakeClient({});

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={stagedOnly}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      expect(screen.queryByRole("button", { name: "Stage all" })).not.toBeInTheDocument();
    });
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && pnpm test -- --run DiffPane.test.tsx`
Expected: FAIL — no `aria-label`-named Stage/Unstage buttons, no "Stage all"/"Unstage all"
buttons exist yet.

- [ ] **Step 3: Add the hover-reveal CSS**

In `frontend/src/components/DiffPane.module.css`, add:

```css
.stageToggle {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: var(--space-1);
  opacity: 0;
  transition: opacity var(--motion-duration-fast) var(--motion-easing-standard);
}

.fileRow:hover .stageToggle,
.fileRow:focus-within .stageToggle,
.stageToggle:focus-visible {
  opacity: 1;
}
```

- [ ] **Step 4: Convert the Stage/Unstage buttons to icon-only controls**

In `frontend/src/components/DiffPane.tsx`, add `Minus, Plus` to the `lucide-react` import (from
Task 4's `import { AlertTriangle, ArrowRightLeft, FileDiff, FileMinus, FilePlus, Pencil, type LucideIcon } from "lucide-react";`):

```tsx
import { AlertTriangle, ArrowRightLeft, FileDiff, FileMinus, FilePlus, Minus, Pencil, Plus, type LucideIcon } from "lucide-react";
```

In `FileListRow`, replace:

```tsx
        {entry.staged ? (
          <button
            type="button"
            onClick={(event) => {
              event.stopPropagation();
              onUnstageFile(entry.path);
            }}
          >
            Unstage
          </button>
        ) : (
          <button
            type="button"
            onClick={(event) => {
              event.stopPropagation();
              onStageFile(entry.path);
            }}
          >
            Stage
          </button>
        )}
```

with:

```tsx
        {entry.staged ? (
          <button
            type="button"
            className={styles.stageToggle}
            aria-label={`Unstage ${entry.path}`}
            onClick={(event) => {
              event.stopPropagation();
              onUnstageFile(entry.path);
            }}
          >
            <Minus size={14} aria-hidden="true" />
          </button>
        ) : (
          <button
            type="button"
            className={styles.stageToggle}
            aria-label={`Stage ${entry.path}`}
            onClick={(event) => {
              event.stopPropagation();
              onStageFile(entry.path);
            }}
          >
            <Plus size={14} aria-hidden="true" />
          </button>
        )}
```

- [ ] **Step 5: Add Stage all/Unstage all**

In `UncommittedDiffPane`, add next to the other handlers introduced in Task 4 (after
`handleGroupKeyDown`):

```tsx
  const handleStageAll = () => {
    for (const entry of unstagedEntries) {
      onStageFile(entry.path);
    }
  };

  const handleUnstageAll = () => {
    for (const entry of stagedEntries) {
      onUnstageFile(entry.path);
    }
  };
```

Update the two group headings to include the bulk-action button:

```tsx
          <div className={styles.groupHeading}>
            <span>Changes ({unstagedEntries.length})</span>
            <button type="button" onClick={handleStageAll}>
              Stage all
            </button>
          </div>
```

```tsx
          <div className={styles.groupHeading}>
            <span>Staged ({stagedEntries.length})</span>
            <button type="button" onClick={handleUnstageAll}>
              Unstage all
            </button>
          </div>
```

(Each button only renders when its group does, since both group `<div>`s are already gated on
`unstagedEntries.length > 0` / `stagedEntries.length > 0` from Task 4 — satisfies the "does not
render Stage all when there are no unstaged entries" test without extra conditionals.)

- [ ] **Step 6: Run tests to verify they pass**

Run: `cd frontend && pnpm test -- --run DiffPane.test.tsx`
Expected: PASS — all tests in the file.

- [ ] **Step 7: Run the full frontend suite and lint**

Run: `cd frontend && pnpm test -- --run`
Expected: PASS — no regressions in other components.

Run: `cd frontend && pnpm lint`
Expected: no errors.

- [ ] **Step 8: Commit**

```bash
git add frontend/src/components/DiffPane.tsx frontend/src/components/DiffPane.module.css frontend/src/components/DiffPane.test.tsx
git commit -m "feat(diff-pane): icon-only hover stage/unstage controls, Stage all/Unstage all"
```

---

## Out of scope (explicitly deferred by the spec)

- C.3 (icon-ify the five `RemotePanel` toolbar buttons — Fetch/Push/Edit/Credentials/Remove):
  the spec marks this "optional, lower priority" and defers to precedent that doesn't yet exist
  elsewhere in the reskinned components. Not included here; revisit once an icon-button pattern
  is established.

# Keyboard Context Menus Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

## Pre-flight notes (T0, recorded during implementation)

User decisions: D1 A, D2 B, D3 A (tree widget is follow-up #143), D4 A, D5 A, D6 A — all as
recommended, so no "Decisions assumed" variant applies.

- **Branch state:** `feat/keyboard-context-menus` already sat on `origin/main` (`cd68b124`); no
  checkout or rebase was needed.
- **Names:** every grep in Step 2 matched the plan (props, `RowContextMenu`, the remote dialog
  state, `openAddRemoteDraft`, the three inline Menu-key checks, the remotes loop, `COMMAND_GROUPS`).
- **Baseline:** green (1478 frontend tests, lint clean). The first full run after Task 4 hit the
  #135 race in `App.test.tsx` "opens the shortcut sheet on ?" (key fired before the active
  workspace mounted); fixed in its own commit with the #135 pattern.

Deviations from the tasks below (code won where they disagreed):

- **Task 2:** `ContextMenu` also swallows the Menu key and Shift+F10 inside an open menu
  (`preventDefault` + `stopPropagation`), so the key cannot reach a row or list underneath and
  open a second menu (DiffPane and ReadOnlyFileSection render their menu inside the row).
- **Task 3:** `ListRow` only gets the key handler on standalone rows, which already require
  `onClick`; every `onOpenMenu` caller has one, so "attach when `onClick` *or* `onOpenMenu`" was
  not needed.
- **Task 4:** the remote folder header also carries `aria-keyshortcuts`. The folder "…" stays
  visible while its menu is open (`aria-expanded="true"`).
- **Task 5:** with focus inside a changed-file row, the Menu key opens *that* row's menu or
  nothing (no fallback to the selected row's menu).
- **Task 6:** the dialog-internal tests moved from `BranchTree.test.tsx` into the two new
  component tests; `BranchTree.test.tsx` keeps one smoke test per dialog plus the controlled
  tests. A dialog whose remote is gone renders nothing (the state is not cleared by
  `removeRemote`). A successful rename in Edit re-targets `remoteDialog` to the new name, so the
  dialog does not vanish while the URLs are saved. Added an `App.test.tsx` test for the palette
  opening Edit remote with Branches collapsed.
- **Task 9:** the E2E spec has a third test, Shift+F10 on a remote folder header. Shift+F10 works
  through WebKitGTK's WebDriver, so no Menu-key fallback was needed.

**Goal:** Every context-menu action is reachable and usable from the keyboard (Menu key / Shift+F10 on every row that has a menu, a complete APG menu pattern in `ContextMenu`, focus returning to the trigger), and the command palette gains "Edit remote", "Manage credentials", "Remove remote" and "Check out tag". Closes issue #44.

**Architecture:** Frontend only. A small `lib/menuKeys.ts` (`isMenuKey`, `menuAnchor`, `rectAnchor`) is shared by every menu trigger. `ContextMenu` captures its trigger on first render and restores focus to it on Escape, Tab and activation, and adds Tab-closes, explicit Enter/Space, typeahead and native-menu suppression. `ListRow` gains `onOpenMenu`, which wires both right-click and the keyboard for standalone rows; `BranchTree` rows and remote folder headers use it. Remote dialogs (edit, credentials, remove) move into app state (`remoteDialog`) and into two extracted components rendered outside the Branches accordion body, so the palette can open them while the section is collapsed.

**Tech Stack:** React 19 + TypeScript, Vitest + Testing Library (`fireEvent`; no `user-event` in the repo), WebdriverIO (tauri-driver).

**Spec:** `docs/superpowers/specs/2026-10-28-keyboard-context-menus-design.md`

## Decisions assumed

The spec lists six decisions for the user (D1–D6). This plan implements the recommended option
for each. If the user picks differently:

- **D1 B/C (single palette entry with a picker):** Task 7 adds one `edit-remote` / `manage-credentials` entry each, and Task 6's dialogs gain a remote `<select>` when opened with `remoteName: null` (copy `DeleteRemoteBranchDialog`'s pattern). `remoteDialog.remoteName` becomes `string | null`.
- **D2 A (only the two issue entries):** drop `remove-remote:` and `checkout-tag:` from Task 7. Keep Task 6's `"remove"` kind anyway (it fixes the stale "clear upstreams" snapshot).
- **D2 C:** add a Task 7b for Rename/Delete branch and Compare (needs `compare.compareBranch` passed into `buildCommands`, and rename state lifted like `remoteDialog`).
- **D3 B/C (roving tree):** out of this plan; write a follow-up spec.
- **D4 B (no typeahead):** skip Task 2 Step 1's typeahead tests and the matching code.
- **D5 B (no remote "…"):** skip Task 4 Step 1's "remote folder '…'" test and the button.
- **D6 B (tag items in the commit menu):** replace `checkout-tag:` in Task 7 with a Task 5b in `CommitGraph.singleCommitItems`.

## Global Constraints

- No Rust changes, no new dependencies; `docs/LICENSE_COMPLIANCE.md` untouched.
- Branch: `feat/keyboard-context-menus` (spec and plan are already committed there). One PR.
- Tests mock `RepoClient` only (never `@tauri-apps/api` or `postMessage`). Components under test that take callbacks get `vi.fn()`s.
- Use `fireEvent`. Where a test asserts a position, stub rects with `vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue(rect(…))` and restore in `afterEach(() => vi.restoreAllMocks())`.
- Key events: Menu key is `{ key: "ContextMenu" }`, Shift+F10 is `{ key: "F10", shiftKey: true }`.
- UI copy, exactly:
  - Remote folder button: aria-label `Actions for remote {name}`.
  - Palette: `Edit remote {name}…`, `Manage credentials for {name}…`, `Remove remote {name}…`, `Check out tag {name} (detached)`.
  - Shortcut sheet group `Context menus`: `Menu / Shift+F10` "Open the focused item's menu"; `Up / Down` "Move between menu items"; `Home / End` "First / last menu item"; `A letter` "Jump to the next item starting with it"; `Enter / Space` "Run the focused menu item"; `Esc` "Close the menu and return to where it opened"; `Tab` "Close the menu and move on". "Changed files" gains `Menu / Shift+F10` "Open the selected file's menu".
- Existing dialog aria-labels stay: `Edit {name}`, `Credentials for {name}`, `Remove remote confirmation`.
- Every push touching `frontend/src/` or `e2e/` needs a `CHANGELOG.md` update (pre-push hook). The changelog lands in Task 10; push only after it (or use `SKIP_CHANGELOG_CHECK=1` for an early WIP push and say so in the PR).

## Review Focus

1. **Focus never ends on `<body>` after a keyboard menu interaction** (Task 2: Escape, Tab, activation; Task 4: Escape back to a branch row; dialog opened from an item returns to the trigger).
2. **A key press opens exactly one menu.** Explicit Menu/Shift+F10 handlers `preventDefault()`; a native `contextmenu` that still arrives lands on the open menu and is swallowed (Task 2 `swallows contextmenu events on its items`). Check this manually in Tauri on Linux and in VSCode (Task 10).
3. **The rename input keeps its keys.** Shift+F10 inside the rename field does not open the row menu (Task 3).
4. **Remote dialogs open while Branches is collapsed** and the controlled/local split does not double-open them (Task 6).
5. **Palette guards:** new entries are omitted while an operation runs (Task 7).

---

## File Structure

| File | Responsibility |
|---|---|
| `frontend/src/lib/menuKeys.ts` (new, + `.test.ts`) | `isMenuKey`, `menuAnchor`, `rectAnchor` |
| `frontend/src/components/primitives/ContextMenu.tsx`, `.module.css` (+ test) | focus return, Tab, Enter/Space, typeahead, native-menu suppression, empty-menu focus, `:focus-visible` |
| `frontend/src/components/primitives/ListRow.tsx` (+ test) | `onOpenMenu` prop |
| `frontend/src/components/BranchTree.tsx`, `.module.css` (+ test) | rows via `onOpenMenu`, remote folder keys + "…", `aria-expanded`, controlled `remoteDialog`, dialogs outside the accordion |
| `frontend/src/components/EditRemoteDialog.tsx`, `RemoteCredentialsDialog.tsx` (new, + tests) | extracted remote forms |
| `frontend/src/components/ReadOnlyFileSection.tsx` (+ test) | header Menu key, `menuAnchor` |
| `frontend/src/components/DiffPane.tsx` (+ test) | `menuAnchor`, Menu key opens the focused row's menu |
| `frontend/src/components/CommitGraph.tsx`, `RepoTabs.tsx` | `isMenuKey` / `menuAnchor` (no behaviour change) |
| `frontend/src/state/useRemoteTransferActions.ts`, `useAppState.ts` (+ `useAppState.test.ts`) | `remoteDialog`, `openRemoteDialog`, `closeRemoteDialog` |
| `frontend/src/App.tsx` | pass `remoteDialog` / `onRemoteDialogChange` to `BranchTree` |
| `frontend/src/lib/commands.ts` (+ test) | palette entries, groups |
| `frontend/src/lib/shortcuts.ts` (+ `ShortcutSheet.test.tsx`) | shortcut sheet |
| `e2e/specs/keyboard-context-menus.spec.ts` (new) | Tauri E2E |
| `CHANGELOG.md`, `docs/USER_GUIDE.md` | docs |

---

### Task 0: Pre-flight

**Files:** none modified unless a check fails.

- [ ] **Step 1: Branch state**

```bash
git fetch origin
git checkout feat/keyboard-context-menus
git rebase origin/main          # the spec/plan commit only; must apply cleanly
git log --oneline -3
```

- [ ] **Step 2: Re-check every name the plan relies on**

```bash
cd frontend/src
grep -n "export function ContextMenu" -A12 components/primitives/ContextMenu.tsx    # props x, y, items, onClose
grep -n "onContextMenu\|standalone\|handleKeyDown" components/primitives/ListRow.tsx
grep -n "type RowContextMenu" -A5 components/BranchTree.tsx                        # local-branch, add, remote-folder, remote-branch
grep -n "editingRemote\|credentialRemote\|removeConfirmation" components/BranchTree.tsx | head
grep -n "openAddRemoteDraft\|openDeleteRemoteBranchDialog" state/useRemoteTransferActions.ts
grep -n "event.key === \"ContextMenu\"\|key === \"F10\"" components/*.tsx            # RepoTabs, CommitGraph, DiffPane
grep -n "for (const remote of state.remotes)" lib/commands.ts
grep -n "COMMAND_GROUPS" lib/commands.ts
```

If a name moved, adapt the affected task and note it in the PR description.

- [ ] **Step 3: Baseline is green**

```bash
cd frontend && pnpm install && pnpm test -- --run && pnpm lint && cd ..
```

Expected: PASS. If main is red, stop and report.

---

### Task 1: `lib/menuKeys.ts`

**Files:**
- Create: `frontend/src/lib/menuKeys.ts`, `frontend/src/lib/menuKeys.test.ts`
- Modify: `frontend/src/components/RepoTabs.tsx`, `CommitGraph.tsx`, `DiffPane.tsx` (use `isMenuKey`; right-click handlers use `menuAnchor`)

- [ ] **Step 1: Write the failing tests**

```ts
import { describe, expect, it } from "vitest";
import { isMenuKey, menuAnchor, rectAnchor } from "./menuKeys";

function elementAt(left: number, top: number, width: number, height: number): Element {
  const element = document.createElement("div");
  element.getBoundingClientRect = () =>
    ({ left, top, right: left + width, bottom: top + height, width, height, x: left, y: top, toJSON: () => ({}) }) as DOMRect;
  return element;
}

describe("isMenuKey", () => {
  it("accepts the Menu key and Shift+F10 only", () => {
    expect(isMenuKey({ key: "ContextMenu", shiftKey: false })).toBe(true);
    expect(isMenuKey({ key: "F10", shiftKey: true })).toBe(true);
    expect(isMenuKey({ key: "F10", shiftKey: false })).toBe(false);
    expect(isMenuKey({ key: "Enter", shiftKey: true })).toBe(false);
  });
});

describe("menuAnchor", () => {
  const row = elementAt(100, 200, 300, 22);

  it("uses the pointer when it is inside the element (a right-click)", () => {
    expect(menuAnchor({ clientX: 150, clientY: 210 }, row)).toEqual({ x: 150, y: 210 });
  });

  it("uses the element's bottom-left corner for a keyboard-generated event at (0,0)", () => {
    expect(menuAnchor({ clientX: 0, clientY: 0 }, row)).toEqual({ x: 100, y: 222 });
  });

  it("uses the element's bottom-left corner when the point is outside it", () => {
    expect(menuAnchor({ clientX: 5, clientY: 900 }, row)).toEqual({ x: 100, y: 222 });
  });
});

describe("rectAnchor", () => {
  it("is the bottom-left corner", () => {
    expect(rectAnchor(elementAt(10, 20, 30, 40))).toEqual({ x: 10, y: 60 });
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/lib/menuKeys`
Expected: FAIL (module not found).

- [ ] **Step 3: Implement**

```ts
/** Keys that open a context menu: the Menu key, or Shift+F10. */
export function isMenuKey(event: { key: string; shiftKey: boolean }): boolean {
  return event.key === "ContextMenu" || (event.shiftKey && event.key === "F10");
}

/** The element's bottom-left corner, for a menu opened by a key or a button. */
export function rectAnchor(element: Element): { x: number; y: number } {
  const rect = element.getBoundingClientRect();
  return { x: rect.left, y: rect.bottom };
}

/**
 * Where a menu opened by a `contextmenu` event goes: the pointer when it lies inside `element`
 * (a real right-click), else the element's bottom-left corner. Keyboard-generated events carry
 * `(0,0)` or browser-chosen coordinates, and `event.button` is not reliable across WebKitGTK,
 * WKWebView, WebView2 and Electron.
 */
export function menuAnchor(event: { clientX: number; clientY: number }, element: Element): { x: number; y: number } {
  const rect = element.getBoundingClientRect();
  const inside =
    event.clientX >= rect.left && event.clientX <= rect.right && event.clientY >= rect.top && event.clientY <= rect.bottom;
  return inside ? { x: event.clientX, y: event.clientY } : { x: rect.left, y: rect.bottom };
}
```

Then replace the inline checks:
- `RepoTabs.tsx` `handleTabKeyDown`: `if (tabMenuItems !== undefined && isMenuKey(event))`; its `onContextMenu` → `setMenu({ path: repo.path, ...menuAnchor(event, event.currentTarget) })`.
- `CommitGraph.tsx` `handleKeyDown`: `event.key === "Enter" || isMenuKey(event)`; `handleContextMenu(event, commitId)` → `setContextMenu({ commitId, ...menuAnchor(event, event.currentTarget as Element) })` (pass the row element; `ListRow`'s `onContextMenu` gives the event whose `currentTarget` is the `li`). The tag badge handler: `menuAnchor(event, event.currentTarget)`.
- `DiffPane.tsx` `handleGroupKeyDown`: `else if (isMenuKey(event))`; the header `onContextMenu` → `setMenu(menuAnchor(event, event.currentTarget))`.
- `ReadOnlyFileSection.tsx` header `onContextMenu` → `setMenu(menuAnchor(event, event.currentTarget))`.

Note: jsdom rects are all zero, so `menuAnchor` returns `(0,0)`-based anchors in existing tests; no existing assertion depends on right-click coordinates (`grep -n "top:\|left:" src/components/*.test.tsx` to confirm before relying on it; adapt any that do by stubbing the rect).

- [ ] **Step 4: Run tests and lint**

Run: `cd frontend && pnpm test -- --run && pnpm lint`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "refactor(frontend): shared menu key and anchor helpers"
```

---

### Task 2: `ContextMenu` — complete the APG menu pattern

**Files:**
- Modify: `frontend/src/components/primitives/ContextMenu.tsx`, `ContextMenu.module.css`
- Test: `frontend/src/components/primitives/ContextMenu.test.tsx`

**Interface:** new optional prop `returnFocusTo?: HTMLElement | null` (overrides the captured trigger). No other prop changes.

- [ ] **Step 1: Write the failing tests** (add a `describe("focus and keys")` block)

```tsx
import { useState } from "react";
import { ConfirmDialog } from "./ConfirmDialog";

function Host({ items, onClose = () => {} }: { items: ContextMenuItem[]; onClose?: () => void }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button onClick={() => setOpen(true)}>Trigger</button>
      <button>After</button>
      {open && (
        <ContextMenu
          x={0}
          y={0}
          items={items}
          onClose={() => {
            onClose();
            setOpen(false);
          }}
        />
      )}
    </>
  );
}

function openFromTrigger() {
  const trigger = screen.getByRole("button", { name: "Trigger" });
  trigger.focus();
  fireEvent.click(trigger);
  return trigger;
}

describe("focus and keys", () => {
  it("returns focus to the trigger on Escape", () => {
    render(<Host items={[{ label: "One", onSelect: () => {} }]} />);
    const trigger = openFromTrigger();
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "One" }), { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    expect(trigger).toHaveFocus();
  });

  it("Tab closes the menu, refocuses the trigger and lets the browser move on", () => {
    render(<Host items={[{ label: "One", onSelect: () => {} }]} />);
    const trigger = openFromTrigger();
    const notPrevented = fireEvent.keyDown(screen.getByRole("menuitem", { name: "One" }), { key: "Tab" });
    expect(notPrevented).toBe(true);
    expect(screen.queryByRole("menu")).toBeNull();
    expect(trigger).toHaveFocus();
  });

  it("refocuses the trigger before running the item, so a dialog it opens returns there", () => {
    function DialogHost() {
      const [dialog, setDialog] = useState(false);
      return (
        <>
          <Host items={[{ label: "Ask", onSelect: () => setDialog(true) }]} />
          {dialog && (
            <ConfirmDialog ariaLabel="Sure" message="Sure?" confirmLabel="Yes" onConfirm={() => setDialog(false)} onCancel={() => setDialog(false)} />
          )}
        </>
      );
    }
    render(<DialogHost />);
    const trigger = openFromTrigger();
    fireEvent.click(screen.getByRole("menuitem", { name: "Ask" }));
    fireEvent.click(screen.getByRole("button", { name: "Cancel", hidden: true }));
    expect(trigger).toHaveFocus();
  });

  it("does not move focus on an outside click", () => {
    render(<Host items={[{ label: "One", onSelect: () => {} }]} />);
    openFromTrigger();
    const after = screen.getByRole("button", { name: "After" });
    fireEvent.mouseDown(after);
    after.focus();
    expect(screen.queryByRole("menu")).toBeNull();
    expect(after).toHaveFocus();
  });

  it("still closes when the trigger was removed meanwhile", () => {
    const trigger = document.createElement("button");
    document.body.appendChild(trigger);
    trigger.focus();
    const onClose = vi.fn();
    render(<ContextMenu x={0} y={0} items={[{ label: "One", onSelect: () => {} }]} onClose={onClose} />);
    trigger.remove();
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "One" }), { key: "Escape" });
    expect(onClose).toHaveBeenCalled();
  });

  it.each(["Enter", " "])("%j activates the focused item once and closes", (key) => {
    const onSelect = vi.fn();
    const onClose = vi.fn();
    render(<Host items={[{ label: "One", onSelect }]} onClose={onClose} />);
    openFromTrigger();
    const prevented = !fireEvent.keyDown(screen.getByRole("menuitem", { name: "One" }), { key });
    expect(prevented).toBe(true);
    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("Enter on a disabled item does nothing", () => {
    // Disabled items can't hold focus, so dispatch on the menu itself.
    const onSelect = vi.fn();
    render(<ContextMenu x={0} y={0} onClose={() => {}} items={[{ label: "Off", onSelect, disabled: true }]} />);
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Enter" });
    expect(onSelect).not.toHaveBeenCalled();
  });

  it("typeahead jumps to the next enabled item starting with the letter, wrapping", () => {
    render(
      <ContextMenu
        x={0}
        y={0}
        onClose={() => {}}
        items={[
          { label: "Alpha", onSelect: () => {} },
          { label: "Beta", onSelect: () => {} },
          { label: "Blocked", onSelect: () => {}, disabled: true },
          { label: "Bravo", onSelect: () => {} },
        ]}
      />,
    );
    const menu = screen.getByRole("menu");
    fireEvent.keyDown(menu, { key: "b" });
    expect(screen.getByRole("menuitem", { name: "Beta" })).toHaveFocus();
    fireEvent.keyDown(menu, { key: "B" });
    expect(screen.getByRole("menuitem", { name: "Bravo" })).toHaveFocus();
    fireEvent.keyDown(menu, { key: "b" });
    expect(screen.getByRole("menuitem", { name: "Beta" })).toHaveFocus();
    fireEvent.keyDown(menu, { key: "a", ctrlKey: true });
    expect(screen.getByRole("menuitem", { name: "Beta" })).toHaveFocus();
    fireEvent.keyDown(menu, { key: "z" });
    expect(screen.getByRole("menuitem", { name: "Beta" })).toHaveFocus();
  });

  it("swallows contextmenu events on its items (no native menu, no second row menu)", () => {
    const onRowMenu = vi.fn();
    render(
      <div onContextMenu={onRowMenu}>
        <ContextMenu x={0} y={0} onClose={() => {}} items={[{ label: "One", onSelect: () => {} }]} />
      </div>,
    );
    const allowed = fireEvent.contextMenu(screen.getByRole("menuitem", { name: "One" }));
    expect(allowed).toBe(false);
    expect(onRowMenu).not.toHaveBeenCalled();
  });

  it("focuses the menu itself when every item is disabled, and Escape still closes", () => {
    const onClose = vi.fn();
    render(<ContextMenu x={0} y={0} onClose={onClose} items={[{ label: "Off", onSelect: () => {}, disabled: true }]} />);
    const menu = screen.getByRole("menu");
    expect(menu).toHaveFocus();
    fireEvent.keyDown(menu, { key: "Escape" });
    expect(onClose).toHaveBeenCalled();
  });

  it("returnFocusTo overrides the captured trigger", () => {
    const other = document.createElement("button");
    document.body.appendChild(other);
    const onClose = vi.fn();
    render(<ContextMenu x={0} y={0} onClose={onClose} returnFocusTo={other} items={[{ label: "One", onSelect: () => {} }]} />);
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "One" }), { key: "Escape" });
    expect(other).toHaveFocus();
    other.remove();
  });

  it("Escape inside the menu does not reach ancestor key handlers", () => {
    const onAncestorKey = vi.fn();
    render(
      <div onKeyDown={onAncestorKey}>
        <ContextMenu x={0} y={0} onClose={() => {}} items={[{ label: "One", onSelect: () => {} }]} />
      </div>,
    );
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "One" }), { key: "Escape" });
    expect(onAncestorKey).not.toHaveBeenCalled();
  });
});
```

Keep the existing "closes on Escape" test (focus outside the menu path) and "stays open when the pointer leaves it".

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/components/primitives/ContextMenu`
Expected: FAIL on the new tests.

- [ ] **Step 3: Implement**

- Capture the trigger once: `const [capturedTrigger] = useState(() => (document.activeElement instanceof HTMLElement && document.activeElement !== document.body ? document.activeElement : null));` and `const trigger = returnFocusTo !== undefined ? returnFocusTo : capturedTrigger;`. It runs during the first render, before the mount effect moves focus into the menu.
- `restoreFocus()`: `if (trigger?.isConnected) trigger.focus();`.
- `activate(index)`: `const item = items[index]; if (item === undefined || item.disabled === true) return; restoreFocus(); item.onSelect(); onClose();`. The item button's `onClick` calls `activate(index)`.
- `handleMenuKeyDown` additions:
  - `Escape`: `preventDefault(); stopPropagation(); restoreFocus(); onClose();`
  - `Tab`: `restoreFocus(); onClose();` (no `preventDefault`).
  - `Enter` / `" "`: `preventDefault(); activate(focusedIndex);`
  - Typeahead: `if (event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey && event.key !== " ")` → search `enabledIndexes` starting after the current position, wrapping, for `items[i].label.toLowerCase().startsWith(event.key.toLowerCase())`; on a match `preventDefault()`, set and focus it.
- The `document` keydown listener keeps handling Escape for focus outside the menu: `if (event.key === "Escape" && !menuRef.current?.contains(event.target as Node)) { restoreFocus(); onClose(); }` (the in-menu case is handled, and stopped, above). The `mousedown` outside listener just calls `onClose()`.
- `<ul … tabIndex={enabledIndexes.length === 0 ? -1 : undefined} onContextMenu={(event) => { event.preventDefault(); event.stopPropagation(); }}>`; in the mount effect, `else menuRef.current?.focus()` when there is no enabled item.
- CSS: `.item:focus-visible:not(:disabled) { background: var(--color-selected-bg); outline: none; }` next to the `:hover` rule. Check contrast of the selected background against `--color-text` in both themes; keep the global focus outline instead if it reads better (`outline: none` is optional).
- Update the comment block above the mount effect; remove the stale "and hover below" wording.

- [ ] **Step 4: Run tests and lint**

Run: `cd frontend && pnpm test -- --run && pnpm lint`
Expected: PASS. `CommitGraph`'s `onClose` still calls `listRef.current?.focus()`; that runs after `restoreFocus()` and wins, which is the intended graph behaviour.

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): context menus return focus, close on Tab, support typeahead"
```

---

### Task 3: `ListRow` — `onOpenMenu`

**Files:**
- Modify: `frontend/src/components/primitives/ListRow.tsx`
- Test: `frontend/src/components/primitives/ListRow.test.tsx`

**Interface:** `onOpenMenu?: (anchor: { x: number; y: number }) => void`. The props type makes `onContextMenu` and `onOpenMenu` mutually exclusive (`{ onContextMenu?: …; onOpenMenu?: never } | { onOpenMenu?: …; onContextMenu?: never }` intersected with the rest).

- [ ] **Step 1: Write the failing tests**

```tsx
describe("onOpenMenu", () => {
  afterEach(() => vi.restoreAllMocks());

  function rect(left: number, top: number, width: number, height: number) {
    return { left, top, right: left + width, bottom: top + height, width, height, x: left, y: top, toJSON: () => ({}) } as DOMRect;
  }

  function renderRow(onOpenMenu = vi.fn(), selected?: boolean) {
    render(
      <ul>
        <ListRow onClick={vi.fn()} selected={selected} onOpenMenu={onOpenMenu}>
          row content <button>inner</button> <input aria-label="field" />
        </ListRow>
      </ul>,
    );
    return { onOpenMenu, row: screen.getByText(/row content/).closest("li") as HTMLElement };
  }

  it.each([
    [{ key: "ContextMenu" }],
    [{ key: "F10", shiftKey: true }],
  ])("opens at the row's bottom-left on %j", (init) => {
    vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue(rect(10, 20, 200, 22));
    const { onOpenMenu, row } = renderRow();
    const prevented = !fireEvent.keyDown(row, init);
    expect(prevented).toBe(true);
    expect(onOpenMenu).toHaveBeenCalledWith({ x: 10, y: 42 });
  });

  it("opens from a key pressed on a nested button", () => {
    const { onOpenMenu } = renderRow();
    fireEvent.keyDown(screen.getByRole("button", { name: "inner" }), { key: "ContextMenu" });
    expect(onOpenMenu).toHaveBeenCalledTimes(1);
  });

  it("ignores the key inside a text field", () => {
    const { onOpenMenu } = renderRow();
    fireEvent.keyDown(screen.getByRole("textbox", { name: "field" }), { key: "F10", shiftKey: true });
    expect(onOpenMenu).not.toHaveBeenCalled();
  });

  it("opens at the pointer on right-click", () => {
    vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue(rect(0, 0, 300, 30));
    const { onOpenMenu, row } = renderRow();
    const allowed = fireEvent.contextMenu(row, { clientX: 50, clientY: 12 });
    expect(allowed).toBe(false);
    expect(onOpenMenu).toHaveBeenCalledWith({ x: 50, y: 12 });
  });

  it("advertises the menu", () => {
    const { row } = renderRow();
    expect(row).toHaveAttribute("aria-haspopup", "menu");
    expect(row).toHaveAttribute("aria-keyshortcuts", "Shift+F10 ContextMenu");
  });

  it("leaves keys to the container on a container-owned row", () => {
    const { onOpenMenu, row } = renderRow(vi.fn(), false);
    fireEvent.keyDown(row, { key: "ContextMenu" });
    expect(onOpenMenu).not.toHaveBeenCalled();
    fireEvent.contextMenu(row);
    expect(onOpenMenu).toHaveBeenCalledTimes(1);
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/components/primitives/ListRow`
Expected: FAIL.

- [ ] **Step 3: Implement**

- In `handleKeyDown` (standalone rows), before the `event.target !== event.currentTarget` guard: `if (onOpenMenu !== undefined && isMenuKey(event)) { const target = event.target as HTMLElement; if (target.closest("input, textarea, select") !== null) return; event.preventDefault(); onOpenMenu(rectAnchor(event.currentTarget)); return; }`.
- Attach `onKeyDown` for standalone rows when `onClick` **or** `onOpenMenu` is set (today it requires `onClick`).
- `onContextMenu={onOpenMenu !== undefined ? (event) => { event.preventDefault(); onOpenMenu(menuAnchor(event, event.currentTarget)); } : onContextMenu}`.
- `aria-haspopup="menu"` and `aria-keyshortcuts="Shift+F10 ContextMenu"` when `onOpenMenu` is set; only `aria-haspopup` on container-owned rows (the container owns the keys).
- Extend the doc comment: one paragraph on `onOpenMenu`.

- [ ] **Step 4: Run tests and lint**

Run: `cd frontend && pnpm test -- --run && pnpm lint`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): ListRow opens its menu from the Menu key and Shift+F10"
```

---

### Task 4: `BranchTree` — keyboard menus on every row

**Files:**
- Modify: `frontend/src/components/BranchTree.tsx`, `BranchTree.module.css`
- Test: `frontend/src/components/BranchTree.test.tsx`

- [ ] **Step 1: Write the failing tests** (new `describe("BranchTree — keyboard menus")`; reuse `renderTree`)

```tsx
describe("BranchTree — keyboard menus", () => {
  const origin = { name: "origin", fetchUrl: "git@github.com:user/repo.git", pushUrl: null, authMode: null, authUsername: null };
  function rect(left: number, top: number, width: number, height: number) {
    return { left, top, right: left + width, bottom: top + height, width, height, x: left, y: top, toJSON: () => ({}) } as DOMRect;
  }
  beforeEach(() => localStorage.clear());
  afterEach(() => vi.restoreAllMocks());

  it("Shift+F10 on a local branch row opens its menu at the row and Escape returns focus to the row", () => {
    vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue(rect(8, 100, 240, 22));
    renderTree();
    const row = screen.getByTitle("feat/foo").closest('[role="button"]') as HTMLElement;
    row.focus();
    fireEvent.keyDown(row, { key: "F10", shiftKey: true });
    const menu = screen.getByRole("menu");
    expect(menu).toHaveStyle({ left: "8px" });
    expect(screen.getByRole("menuitem", { name: "Checkout" })).toHaveFocus();
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "Checkout" }), { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    expect(row).toHaveFocus();
  });

  it("the Menu key on the current branch focuses the first enabled item (Checkout is disabled)", () => {
    renderTree();
    const row = screen.getByTitle("main").closest('[role="button"]') as HTMLElement;
    fireEvent.keyDown(row, { key: "ContextMenu" });
    expect(screen.getByRole("menuitem", { name: "Rename" })).toHaveFocus();
  });

  it("does not open the row menu from the rename field", () => {
    renderTree();
    const row = screen.getByTitle("feat/foo").closest('[role="button"]') as HTMLElement;
    fireEvent.keyDown(row, { key: "ContextMenu" });
    fireEvent.click(screen.getByRole("menuitem", { name: "Rename" }));
    fireEvent.keyDown(screen.getByRole("textbox", { name: "Rename feat/foo" }), { key: "F10", shiftKey: true });
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("remote-tracking branch rows are focusable and open their menu from the keyboard", async () => {
    renderTree({ remotes: [origin], onListRemoteBranches: vi.fn().mockResolvedValue(["topic"]) });
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    const row = (await screen.findByTitle("topic")).closest('[role="button"]') as HTMLElement;
    expect(row.tabIndex).toBe(0);
    row.focus();
    expect(row).toHaveFocus();
    fireEvent.keyDown(row, { key: "ContextMenu" });
    expect(screen.getByRole("menuitem", { name: "Checkout" })).toHaveFocus();
  });

  it("the Menu key on a remote folder header opens the remote's menu at the header", () => {
    vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue(rect(4, 300, 240, 22));
    renderTree({ remotes: [origin] });
    const header = screen.getByRole("button", { name: "origin" });
    header.focus();
    fireEvent.keyDown(header, { key: "ContextMenu" });
    expect(screen.getByRole("menu")).toHaveStyle({ left: "4px" });
    expect(screen.getByRole("menuitem", { name: "Edit remote" })).toBeInTheDocument();
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "Fetch" }), { key: "Escape" });
    expect(header).toHaveFocus();
  });

  it("a remote folder has a '…' button for its actions", () => {
    renderTree({ remotes: [origin] });
    const actions = screen.getByRole("button", { name: "Actions for remote origin" });
    expect(actions).toHaveAttribute("aria-haspopup", "menu");
    expect(actions).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(actions);
    expect(actions).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("menuitem", { name: "Manage credentials" })).toBeInTheDocument();
  });

  it("'…' and '+' report whether their menu is open", () => {
    renderTree();
    const actions = screen.getByRole("button", { name: "Actions for feat/foo" });
    const add = screen.getByRole("button", { name: "Add" });
    expect(actions).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(add);
    expect(add).toHaveAttribute("aria-expanded", "true");
    expect(actions).toHaveAttribute("aria-expanded", "false");
  });

  it("the '+' menu opens under the button, not at (0,0) (regression, PR #72)", () => {
    vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue(rect(180, 40, 20, 20));
    renderTree();
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    expect(screen.getByRole("menu")).toHaveStyle({ left: "180px", top: "60px" });
  });
});
```

Note: `ContextMenu` clamps its position in a layout effect using its own rect, which the stub also returns; with the values above (`window.innerWidth` 1024 / `innerHeight` 768 in jsdom) no clamping occurs. If a stub value triggers clamping, adjust the numbers rather than the assertion's intent.

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/components/BranchTree`
Expected: FAIL on the new tests.

- [ ] **Step 3: Implement**

- Local rows: replace `onContextMenu` with `onOpenMenu={isRenaming ? undefined : (anchor) => openRowMenu({ kind: "local-branch", name: branch.name, ...anchor })}`.
- Remote-branch rows: `onOpenMenu={(anchor) => openRowMenu({ kind: "remote-branch", remoteName, branchName, ...anchor })}`. Update the stale comment above their "…" button (the row itself now handles the keys).
- Remote folder header `<button>`: `onKeyDown={(event) => { if (isMenuKey(event)) { event.preventDefault(); openRowMenu({ kind: "remote-folder", name: remote.name, ...rectAnchor(event.currentTarget) }); } }}`, `onContextMenu` → `menuAnchor`. Wrap the header and a new sibling "…" button in a `div className={styles.folderRow}`; the button is `aria-label={`Actions for remote ${remote.name}`}`, `aria-haspopup="menu"`, `aria-expanded`, opens at its rect. CSS: reuse `.iconButton` and the hover/focus-within reveal the branch rows use (`.treeRow:hover .iconButton, .treeRow:focus-within .iconButton`) for `.folderRow`.
- `aria-expanded` on every "…" and on "+": `rowMenu !== null && rowMenu.kind === … && <same name>`.
- `commands.ts`'s `expandRemoteBranches` finds the remote header by `button[aria-expanded]` with `textContent === remoteName`; the new "…" buttons have no text, so it still matches only the header. Its comment says the rows are "real keyboard-focusable buttons, each with a '…' affordance" — still true.

- [ ] **Step 4: Run tests and lint**

Run: `cd frontend && pnpm test -- --run && pnpm lint`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): open branch, remote branch and remote menus from the keyboard"
```

---

### Task 5: File sections — Menu key from nested controls

**Files:**
- Modify: `frontend/src/components/ReadOnlyFileSection.tsx`, `frontend/src/components/DiffPane.tsx`
- Test: `frontend/src/components/ReadOnlyFileSection.test.tsx`, `frontend/src/components/DiffPane.test.tsx`

- [ ] **Step 1: Write the failing tests**

`ReadOnlyFileSection.test.tsx`:

```tsx
it("opens the file menu with the Menu key from a header button and returns focus there on Escape", () => {
  const externalTools = { openInEditor: vi.fn(), reveal: vi.fn(), openDiffTool: vi.fn() } as unknown as ExternalToolsApi;
  renderSection({ externalTools, toolSource: { kind: "Commit", commitId: "abc" } as DiffSource });
  const blame = screen.getByRole("button", { name: "Blame" });
  blame.focus();
  fireEvent.keyDown(blame, { key: "F10", shiftKey: true });
  expect(screen.getByRole("menuitem", { name: "Open in editor" })).toHaveFocus();
  fireEvent.keyDown(screen.getByRole("menuitem", { name: "Open in editor" }), { key: "Escape" });
  expect(blame).toHaveFocus();
});

it("has no menu key handling without external tools", () => {
  renderSection();
  fireEvent.keyDown(screen.getByRole("button", { name: "Blame" }), { key: "ContextMenu" });
  expect(screen.queryByRole("menu")).toBeNull();
});
```

(Use the real `ExternalToolsApi` / `DiffSource` shapes: `grep -n "ExternalToolsApi\|DiffSource" frontend/src/components/ReadOnlyFileSection.test.tsx frontend/src/ipc/RepoClient.ts` and copy an existing fixture if one exists.)

`DiffPane.test.tsx`, next to "opens the menu for the selected row with Shift+F10":

```tsx
it("opens the menu of the row holding focus, not the selected row", () => {
  render(); // fixture with src/a.ts and src/b.ts unstaged; adapt to the file's existing helper
  fireEvent.click(screen.getByText("src/a.ts (Modified)"));
  const other = screen.getByRole("button", { name: "Stage src/b.ts" });
  other.focus();
  fireEvent.keyDown(other, { key: "ContextMenu" });
  fireEvent.click(screen.getByRole("menuitem", { name: "Discard changes…" }));
  expect(screen.getByText(/Discard changes to src\/b\.ts/)).toBeInTheDocument();
});
```

If the existing `render()` fixture has only one unstaged file, add a second entry for this test.

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/components/ReadOnlyFileSection src/components/DiffPane`
Expected: FAIL.

- [ ] **Step 3: Implement**

- `ReadOnlyFileSection`: on the header `div`, `onKeyDown={tools === null ? undefined : (event) => { if (!isMenuKey(event)) return; event.preventDefault(); setMenu(rectAnchor(event.currentTarget)); }}`. Focus returns to the focused header button through Task 2.
- `DiffPane.handleGroupKeyDown`: for `isMenuKey(event)`, first look for a trigger in the row containing the event target: `(event.target as HTMLElement).closest('[role="option"]')?.querySelector<HTMLButtonElement>('button[aria-haspopup="menu"]')`; fall back to the selected row's trigger as today. Update the comment ("conflicted and staged rows have none" is stale: with external tools, conflicted rows have one; staged rows have "Stash file…").

- [ ] **Step 4: Run tests and lint**

Run: `cd frontend && pnpm test -- --run && pnpm lint`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): file menus open from the Menu key on any header control"
```

---

### Task 6: Remote dialogs in app state

**Files:**
- Create: `frontend/src/components/EditRemoteDialog.tsx`, `EditRemoteDialog.test.tsx`, `RemoteCredentialsDialog.tsx`, `RemoteCredentialsDialog.test.tsx`
- Modify: `frontend/src/components/BranchTree.tsx` (+ test), `frontend/src/state/useRemoteTransferActions.ts`, `frontend/src/state/useAppState.ts` (+ `useAppState.test.ts`), `frontend/src/App.tsx`

**Interfaces:**
- `export type RemoteDialog = { kind: "edit" | "credentials" | "remove"; remoteName: string };` (in `useAppState.ts`, next to the other dialog state types).
- `AppState.remoteDialog: RemoteDialog | null` (initial `null`); `openRemoteDialog(kind: RemoteDialog["kind"], remoteName: string): void`; `closeRemoteDialog(): void`. Neither calls the client.
- `EditRemoteDialog({ remote, onRename, onUpdateUrls, onClose })` where `onRename: (oldName, newName) => Promise<boolean>`, `onUpdateUrls: (name, fetchUrl, pushUrl | null) => Promise<void>`. Field state initialises from `remote` on mount.
- `RemoteCredentialsDialog({ remote, onSetAuthMode, onSaveHttpsCredential, onForgetHttpsCredential, onClose })`, same signatures as `BranchTree`'s props today.
- `BranchTree` new optional props `remoteDialog?: RemoteDialog | null` and `onRemoteDialogChange?: (dialog: RemoteDialog | null) => void`. Controlled when `onRemoteDialogChange` is given, local state otherwise.

- [ ] **Step 1: Write the failing tests**

`useAppState.test.ts`, next to the push/delete-remote-branch dialog test:

```ts
it("openRemoteDialog/closeRemoteDialog toggle remoteDialog", async () => {
  const client = transferClient({});
  const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
  await act(() => result.current.refresh());
  expect(result.current.state.remoteDialog).toBeNull();
  act(() => result.current.openRemoteDialog("credentials", "origin"));
  expect(result.current.state.remoteDialog).toEqual({ kind: "credentials", remoteName: "origin" });
  act(() => result.current.closeRemoteDialog());
  expect(result.current.state.remoteDialog).toBeNull();
});
```

`EditRemoteDialog.test.tsx` / `RemoteCredentialsDialog.test.tsx`: move the dialog-specific assertions from `BranchTree.test.tsx` ("prefilled with the remote's URLs; saving calls onUpdateRemoteUrls", the clipboard copy tests, rename-then-update, failed rename keeps the dialog open, HTTPS save, SSH agent, forget, the token field is cleared) so they render the component directly. Leave one smoke test per dialog in `BranchTree.test.tsx` (menu item → dialog opens) so the wiring stays covered.

`BranchTree.test.tsx`:

```tsx
describe("BranchTree — controlled remote dialogs", () => {
  const origin = { name: "origin", fetchUrl: "git@github.com:user/repo.git", pushUrl: null, authMode: null, authUsername: null };
  beforeEach(() => localStorage.clear());

  it.each([
    ["edit", "Edit origin"],
    ["credentials", "Credentials for origin"],
    ["remove", "Remove remote confirmation"],
  ] as const)("renders the %s dialog from props even while Branches is collapsed", async (kind, name) => {
    localStorage.setItem("sidebar-branches", "closed"); // `persistOpen`'s format: "open" | "closed"
    renderTree({ remotes: [origin], remoteDialog: { kind, remoteName: "origin" }, onRemoteDialogChange: vi.fn() });
    expect(screen.queryByRole("button", { name: "origin" })).toBeNull(); // body not rendered
    expect(await screen.findByRole("dialog", { name, hidden: true })).toBeInTheDocument();
  });

  it("menu items report the dialog through onRemoteDialogChange when controlled", () => {
    const onRemoteDialogChange = vi.fn();
    renderTree({ remotes: [origin], remoteDialog: null, onRemoteDialogChange });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Manage credentials" }));
    expect(onRemoteDialogChange).toHaveBeenCalledWith({ kind: "credentials", remoteName: "origin" });
    expect(screen.queryByRole("dialog", { name: "Credentials for origin" })).toBeNull();
  });

  it("closes a dialog whose remote is gone", () => {
    const onRemoteDialogChange = vi.fn();
    renderTree({ remotes: [], remoteDialog: { kind: "edit", remoteName: "origin" }, onRemoteDialogChange });
    expect(screen.queryByRole("dialog", { name: "Edit origin" })).toBeNull();
  });

  it("asks to clear upstreams based on the upstreams at confirm time", () => {
    renderTree({
      remotes: [origin],
      remoteDialog: { kind: "remove", remoteName: "origin" },
      onRemoteDialogChange: vi.fn(),
      remoteUpstreams: { origin: [{ localBranch: "main", remoteName: "origin", remoteBranch: "main" }] },
    });
    expect(screen.getByText(/clear upstreams for main/)).toBeInTheDocument();
  });
});
```

(Check `UpstreamInfo`'s fields in `RepoClient.ts` and `AccordionSection`'s `storageKey` for Branches, `"sidebar-branches"`, before relying on them.)

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/state/useAppState src/components/BranchTree src/components/EditRemoteDialog src/components/RemoteCredentialsDialog`
Expected: FAIL.

- [ ] **Step 3: Implement**

- `useRemoteTransferActions.ts`: `openRemoteDialog` / `closeRemoteDialog` with `useCallback` + `setState`, exactly like `openDeleteRemoteBranchDialog`. Add `remoteDialog: null` to the initial state and both actions to `UseAppStateResult` and the returned object (both places `openAddRemoteDraft` appears in `useAppState.ts`). Clear `remoteDialog` in `removeRemote`'s success path if it named the removed remote (or rely on BranchTree closing a dialog whose remote is gone; pick one and test it).
- Extract the two `FormDialog`s from `BranchTree` into `EditRemoteDialog` / `RemoteCredentialsDialog` unchanged in markup and copy; their local field state (`editName`, `editFetchUrl`, `editPushUrl`, `credentialMode`, `credentialUsername`, the token ref) moves with them, initialised from `remote`.
- `BranchTree`: replace `editingRemote`, `credentialRemote` and `removeConfirmation` with `const [localRemoteDialog, setLocalRemoteDialog] = useState<RemoteDialog | null>(null); const remoteDialog = onRemoteDialogChange !== undefined ? (remoteDialogProp ?? null) : localRemoteDialog; const setRemoteDialog = onRemoteDialogChange ?? setLocalRemoteDialog;`. `remoteFolderItems` calls `setRemoteDialog({ kind, remoteName: remote.name })`. Resolve `const dialogRemote = remoteDialog === null ? undefined : remotes.find((r) => r.name === remoteDialog.remoteName)`; render nothing when it is `undefined`.
- Return `<>{accordion}{dialogs}</>`: the `AccordionSection` as today, followed by the Remove `ConfirmDialog` (its "clear upstreams" variant computed from `remoteUpstreams[dialogRemote.name]` at render), `EditRemoteDialog` and `RemoteCredentialsDialog`. `FormDialog`/`ConfirmDialog` are modal overlays, so DOM position does not affect layout.
- `App.tsx`: pass `remoteDialog={appState.state.remoteDialog}` and `onRemoteDialogChange={(dialog) => dialog === null ? appState.closeRemoteDialog() : appState.openRemoteDialog(dialog.kind, dialog.remoteName)}` to `BranchTree`. `BranchTree` is rendered per workspace; only the active repo's palette opens a dialog, and every workspace already gates its own overlays the same way the in-tree dialogs did.

- [ ] **Step 4: Run tests and lint**

Run: `cd frontend && pnpm test -- --run && pnpm lint`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "refactor(frontend): remote dialogs in app state, rendered outside the Branches body"
```

---

### Task 7: Palette entries

**Files:**
- Modify: `frontend/src/lib/commands.ts`
- Test: `frontend/src/lib/commands.test.ts`

- [ ] **Step 1: Write the failing tests**

Add `openRemoteDialog: vi.fn()`, `closeRemoteDialog: vi.fn()` and `checkoutCommit: vi.fn()` (if missing) to `makeAppState`'s mock result, and `remoteDialog: null` to its state.

```ts
describe("remote and tag palette entries", () => {
  const remotes = [
    { name: "origin", fetchUrl: "a", pushUrl: null, authMode: null, authUsername: null },
    { name: "fork", fetchUrl: "b", pushUrl: null, authMode: null, authUsername: null },
  ];

  it.each([
    ["edit-remote:fork", "Edit remote fork…", "edit"],
    ["manage-credentials:fork", "Manage credentials for fork…", "credentials"],
    ["remove-remote:fork", "Remove remote fork…", "remove"],
  ] as const)("%s opens the %s dialog for that remote", (id, label, kind) => {
    const appState = makeAppState({ remotes });
    const command = buildCommands(appState).find((c) => c.id === id);
    expect(command?.label).toBe(label);
    command?.run();
    expect(appState.openRemoteDialog).toHaveBeenCalledWith(kind, "fork");
    expect(commandGroup(id)).toBe("Remotes");
  });

  it("emits one entry per remote", () => {
    const ids = buildCommands(makeAppState({ remotes })).map((c) => c.id);
    expect(ids.filter((id) => id.startsWith("edit-remote:"))).toEqual(["edit-remote:origin", "edit-remote:fork"]);
  });

  it("omits remote and tag entries while an operation is in progress", () => {
    const tags = [{ name: "v1", targetId: "c1", annotated: false, message: null, taggerName: null, timestamp: null }];
    const ids = buildCommands(makeAppState({ remotes, tags, pending: true })).map((c) => c.id);
    expect(ids.some((id) => /^(edit-remote|manage-credentials|remove-remote|checkout-tag):/.test(id))).toBe(false);
  });

  it("checks out a tag's target commit, even with the Tags panel hidden", () => {
    const tags = [{ name: "v1", targetId: "c1", annotated: true, message: "m", taggerName: "t", timestamp: 1 }];
    const appState = makeAppState({ tags });
    const hiddenTags = { ...defaultVisibility(), tags: false }; // use the test file's existing visibility helper
    const command = buildCommands(appState, undefined, undefined, undefined, hiddenTags).find((c) => c.id === "checkout-tag:v1");
    expect(command?.label).toBe("Check out tag v1 (detached)");
    command?.run();
    expect(appState.checkoutCommit).toHaveBeenCalledWith("c1");
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/lib/commands`
Expected: FAIL.

- [ ] **Step 3: Implement**

- Inside the existing `for (const remote of state.remotes)` loop (already under `!repositoryOperationDisabled`), after `browse-remote-branches:`:

```ts
commands.push(
  {
    id: `edit-remote:${remote.name}`,
    label: `Edit remote ${remote.name}…`,
    keywords: ["remote", "edit", "url", "rename", remote.name],
    run: () => appState.openRemoteDialog("edit", remote.name),
  },
  {
    id: `manage-credentials:${remote.name}`,
    label: `Manage credentials for ${remote.name}…`,
    keywords: ["remote", "credentials", "token", "ssh", "https", "auth", "password", remote.name],
    run: () => appState.openRemoteDialog("credentials", remote.name),
  },
  {
    // Opens the same confirmation as the remote's menu; never removes directly.
    id: `remove-remote:${remote.name}`,
    label: `Remove remote ${remote.name}…`,
    keywords: ["remote", "remove", "delete", remote.name],
    run: () => appState.openRemoteDialog("remove", remote.name),
  },
);
```

- Under `!repositoryOperationDisabled`, outside the `panelVisibility.tags` block (it does not navigate):

```ts
for (const tag of state.tags) {
  commands.push({
    id: `checkout-tag:${tag.name}`,
    label: `Check out tag ${tag.name} (detached)`,
    keywords: ["tag", "checkout", "detached", tag.name],
    run: () => void appState.checkoutCommit(tag.targetId),
  });
}
```

- `commandGroup`: add `edit-remote:`, `manage-credentials:`, `remove-remote:` prefixes to "Remotes"; add `checkout-tag:` to "Branches" (next to `switch-branch:`; it is a checkout).

(If the user chose D1 B/C or D2 A, adapt per "Decisions assumed".)

- [ ] **Step 4: Run tests and lint**

Run: `cd frontend && pnpm test -- --run && pnpm lint`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): palette entries to edit, remove and manage credentials for a remote, and check out a tag"
```

---

### Task 8: Shortcut sheet

**Files:**
- Modify: `frontend/src/lib/shortcuts.ts`
- Test: `frontend/src/components/ShortcutSheet.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
it("lists the context menu keys", () => {
  render(<ShortcutSheet onClose={vi.fn()} />);
  expect(screen.getByRole("heading", { name: "Context menus" })).toBeInTheDocument();
  expect(screen.getByText("Close the menu and return to where it opened")).toBeInTheDocument();
  expect(screen.getByText("Open the selected file's menu")).toBeInTheDocument();
});
```

- [ ] **Step 2: Run to verify failure** — `cd frontend && pnpm test -- --run src/components/ShortcutSheet` → FAIL.

- [ ] **Step 3: Implement** — add the "Context menus" group (after "Commit history") and the "Changed files" entry, copy exactly as in Global Constraints. Check how `ShortcutSheet` renders group titles (heading or not) and adjust the test query, not the copy.

- [ ] **Step 4: Run tests and lint** — `cd frontend && pnpm test -- --run && pnpm lint` → PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "docs(frontend): context menu keys in the shortcut sheet"
```

---

### Task 9: Tauri E2E — keyboard menu and palette remote dialog

**Files:**
- Create: `e2e/specs/keyboard-context-menus.spec.ts`

Non-destructive: it adds a remote in `before` (removed in `after`) and only opens and closes menus and dialogs. Each spec file gets a freshly cloned fixture repo.

- [ ] **Step 1: Write the spec**

```ts
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { expandSidebarSection } from "../support/sidebar";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const BARE_REMOTE_PATH = path.join(os.tmpdir(), "browsitory-e2e-keyboard-remote.git");

// Same retrying open as `command-palette.spec.ts`'s `openPalette` (Ctrl+K can arrive before the
// app's key handler is attached right after a reload).
async function runPaletteCommand(query: string): Promise<void> {
  const input = await $('input[aria-label="Command palette"]');
  await browser.waitUntil(
    async () => {
      if (await input.isDisplayed()) return true;
      await browser.keys(["Control", "k"]);
      return input.waitForDisplayed({ timeout: 1000 }).then(
        () => true,
        () => false,
      );
    },
    { timeout: 10000, interval: 0, timeoutMsg: "expected Ctrl/Cmd+K to open the command palette" },
  );
  await input.setValue(query);
  await browser.keys(["Enter"]);
  await input.waitForDisplayed({ reverse: true, timeout: 10000 });
}

describe("Browsitory keyboard context menus", () => {
  before(() => {
    fs.rmSync(BARE_REMOTE_PATH, { recursive: true, force: true });
    execFileSync("git", ["init", "--bare", BARE_REMOTE_PATH], { stdio: "inherit" });
    execFileSync("git", ["remote", "add", "kbd-origin", BARE_REMOTE_PATH], { cwd: E2E_REPO_PATH, stdio: "inherit" });
  });

  after(() => {
    execFileSync("git", ["remote", "remove", "kbd-origin"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    fs.rmSync(BARE_REMOTE_PATH, { recursive: true, force: true });
  });

  it("opens a branch menu with Shift+F10 and returns focus on Escape", async () => {
    await expandSidebarSection("Branches");
    const row = await $('//li[@role="button" and contains(., " (current)")]');
    await row.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).focus(), row);

    await browser.keys(["Shift", "F10"]);
    const menu = await $("[role='menu']");
    await menu.waitForExist({ timeout: 10000 });
    // Checkout is disabled on the current branch, so Rename is the first focusable item.
    expect(await browser.execute(() => document.activeElement?.textContent)).toBe("Rename");

    await browser.keys(["ArrowDown"]);
    expect(await browser.execute(() => document.activeElement?.getAttribute("role"))).toBe("menuitem");
    expect(await browser.execute(() => document.activeElement?.textContent)).not.toBe("Rename");

    await browser.keys(["Escape"]);
    await menu.waitForExist({ reverse: true, timeout: 10000 });
    expect(await browser.execute((el) => document.activeElement === el, row)).toBe(true);
    // Exactly one menu: a native contextmenu following the key must not have opened a second.
    expect(await $$("[role='menu']")).toHaveLength(0);
  });

  it("opens Edit remote from the palette", async () => {
    await runPaletteCommand("Refresh");
    await runPaletteCommand("Edit remote kbd-origin…");
    const dialog = await $("dialog[aria-label='Edit kbd-origin']");
    await dialog.waitForExist({ timeout: 10000 });
    await browser.keys(["Escape"]);
    await dialog.waitForExist({ reverse: true, timeout: 10000 });
  });
});
```

The palette ranks label-prefix matches first, so the full label as the query puts the wanted entry on top for Enter. If WebKitGTK's WebDriver does not deliver `["Shift", "F10"]` as `key: "F10", shiftKey: true`, use `["ContextMenu"]` (WebDriver key `` is not standard; prefer Shift+F10) and note it in the spec file.

- [ ] **Step 2: Build and run** (order-sensitive, from repo root, per CLAUDE.md)

```bash
cd frontend && VITE_E2E_REPO_PATH=/tmp/browsitory-e2e-repo pnpm build && cd ..
cargo build --workspace --features tauri-app/custom-protocol,tauri-app/forge-fixture-override
cd e2e && pnpm install && pnpm test -- --spec ./specs/keyboard-context-menus.spec.ts && cd ..
```

Expected: PASS. Then the full E2E suite (`cd e2e && pnpm test`) to make sure the added/removed remote does not leak into other specs.

- [ ] **Step 3: Commit**

```bash
git add e2e/specs/keyboard-context-menus.spec.ts
git commit -m "test(e2e): keyboard context menu and palette Edit remote"
```

---

### Task 10: Docs, manual checks, verification

**Files:**
- Modify: `CHANGELOG.md`, `docs/USER_GUIDE.md`

- [ ] **Step 1: CHANGELOG** — under `## [Unreleased]`, a new `### Keyboard context menus` section:

```markdown
### Keyboard context menus

- Every row with a context menu opens it with the Menu key or Shift+F10: local and remote
  branches, remote folders, changed files and commit/compare file headers (commits and tabs
  already did). The menu opens at the row, not the top-left corner.
- Menus return focus to where they were opened when closed with Esc or by running an item (so a
  dialog an item opens also returns there), close on Tab, and jump to an item by its first letter.
- Remote folders have a "…" actions button, like branches.
- Command palette: **Edit remote …**, **Manage credentials for …** and **Remove remote …** for
  each remote, and **Check out tag … (detached)** for each tag. The remote dialogs also open while
  the Branches section is collapsed.
```

- [ ] **Step 2: USER_GUIDE**
  - "Branches and remotes": after the paragraph listing the menus, add: "With the keyboard, Tab to a branch, a remote branch or a remote folder and press the Menu key or Shift+F10 (or use its **…** button). In any menu, ↑/↓, Home/End or a letter move between items, Enter or Space runs one, and Esc closes the menu and returns to the row." Mention the palette's **Edit remote …**, **Manage credentials for …** and **Remove remote …**.
  - "Tags" (around "The Tags panel remains the keyboard path…"): add "**Check out tag … (detached)** in the command palette checks out a tag's commit."
  - Command palette section (if it lists entries): add the four families.

- [ ] **Step 3: Full verification**

```bash
cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd frontend && pnpm test -- --run && pnpm lint && pnpm build && cd ..
cd extension && pnpm test -- --run && pnpm run compile && cd ..
```

Expected: PASS (Rust untouched; run anyway, CI does).

- [ ] **Step 4: Manual checks** (record results in the PR description)
  - `cargo tauri dev` on Linux: Shift+F10 and the Menu key on a local branch, a remote branch, a remote folder, a commit and a changed file each open one menu at the row; no WebKitGTK native menu appears; Esc returns focus to the row.
  - macOS (if available): Shift+F10 (Fn+Shift+F10) opens the menu (WKWebView never generates `contextmenu` from keys, so this exercises the explicit handler).
  - VSCode extension on Linux (spec §6): same keys on a branch row, a remote folder and a commit; confirm no VSCode webview context menu appears and the workbench does not act on Shift+F10. If it does, note it and open a follow-up issue.
  - Palette: "Edit remote …" with Branches collapsed opens the dialog; closing it returns focus to the palette's previous focus target, not `<body>`.

- [ ] **Step 5: Commit and push**

```bash
git add CHANGELOG.md docs/USER_GUIDE.md
git commit -m "docs: keyboard context menus and remote palette entries"
git push -u origin feat/keyboard-context-menus
```

Open the PR referencing "Closes #44", listing the decisions taken (D1–D6) and the manual check results.

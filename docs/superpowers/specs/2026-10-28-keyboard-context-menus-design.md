# Keyboard context menus and remote palette parity

## Goal

Every action that lives in a context menu can be reached and used from the keyboard, in both
frontends (Tauri app, VSCode extension), and the two remote actions the branch-tree spec promised
for the command palette ("Edit remote", "Manage credentials") exist there. Closes issue #44.

Issue #44 was written in August, right after PR #43 introduced `BranchTree`. Since then PR #72
(UX polish), the 2026-09-05 audit fixes and parity batches 4a–4g closed part of it and added new
menus. This spec starts from the code at `cd68b124`, not from the issue text.

## Current state (surveyed at `cd68b124`)

### `ContextMenu` (`frontend/src/components/primitives/ContextMenu.tsx`)

Already done (AUD-2026-09-05-FE-001):

- `role="menu"` / `menuitem` / `menuitemcheckbox` (with `aria-checked`).
- Focus moves to the first enabled item on open; roving `tabIndex`.
- ArrowDown/ArrowUp (wrapping) and Home/End, skipping disabled items.
- Escape (a `document` keydown listener) and an outside `mousedown` close it.
- Leaving it with the pointer does not close it (there is a test for this).
- Clamped into the viewport after layout.

Missing:

1. **Focus is not returned to the trigger.** Escape, an item click and an outside click unmount
   the menu and focus falls to `<body>`. Only `CommitGraph` refocuses its list in `onClose`.
   Dialogs opened from a menu item (`useRestoreFocus`) then also restore to `<body>`, because
   the trigger was not focused when they mounted.
2. **Tab does nothing special.** Focus leaves the menu and the menu stays open.
3. **Enter/Space** rely on the native `<button>` click. That works in browsers, but no test
   covers it (jsdom does not synthesize the click).
4. **No typeahead.**
5. **A keyboard-generated `contextmenu` event that lands on a menu item** (Windows Menu key fires
   on keyup, after focus has moved into the menu) is not suppressed, so the webview's native menu
   (Tauri dev "Inspect", VSCode's webview menu) can open on top of ours.
6. **A menu with no enabled items** keeps focus on the trigger, so arrow keys and Tab do nothing
   useful and screen readers never enter the menu.
7. **No `:focus-visible` style on items** (only `:hover`), so keyboard focus inside the menu
   relies on the global focus ring.
8. No submenus exist (`ContextMenuItem` has no children). None are added.

### Every `ContextMenu` user and how it is triggered

| Where | Rows / trigger | Right-click | Visible button | Keyboard trigger today |
|---|---|---|---|---|
| `BranchTree` local branch rows | `ListRow` standalone (`li role="button" tabIndex=0`) | yes, `clientX/Y` | "…" (`Actions for <branch>`) | none on the row (Tab to "…", Enter) |
| `BranchTree` remote-tracking branch rows | same `ListRow` standalone | yes, `clientX/Y` | "…" | none on the row (Tab to "…", Enter) |
| `BranchTree` remote folder header (`<button aria-expanded>`) | Fetch, Push current branch here, Edit remote, Manage credentials, Remove remote | yes, `clientX/Y` | **none** | **none** — right-click only |
| `BranchTree` "+" (Add) | New branch…, Add remote…, Show all branches | — | yes, opens at button rect | Enter/Space on the button |
| `CommitGraph` commit rows (`role="option"`, container `listbox`) | single- and multi-select items | yes, `clientX/Y` | — | Enter / Menu / Shift+F10 on the list, at the row rect |
| `CommitGraph` tag badges (`RefBadges`) | Push tag to remote, Checkout (detached), Delete tag… | yes, `clientX/Y` | — | **none** — right-click only (the guide says "the Tags panel remains the keyboard path for pushing and deleting tags"; checkout-a-tag has no keyboard path) |
| `DiffPane` changed-file rows (`role="option"`, container `listbox`) | `fileMenuItems` (discard, stash, ignore, external tools, merge tool) | header `div`, `clientX/Y` | "…" | Menu / Shift+F10 on the group list clicks the *selected* row's "…" |
| `ReadOnlyFileSection` (commit / compare / stash file headers) | `commitFileMenuItems` (editor, reveal, diff tool) | header `div`, `clientX/Y` | "…" | none on the header (Tab to "…", Enter) |
| `RepoTabs` | Open terminal here, Reveal repository | yes, `clientX/Y` | — | Menu / Shift+F10 on the tab, at the tab rect |
| `SplitButton` (SyncBar Fetch, commit dock Stash) | chevron | — | chevron, opens at its rect | Enter/Space on the chevron |

Stash, worktree, submodule, tag, reflog, pull-request, conflict and compare panels have no
context menus.

### What the issue asked for, and where it stands

| Issue #44 item | Status at `cd68b124` |
|---|---|
| Remote-branch rows focusable | **Done.** They are `ListRow` standalone rows (`role="button"`, `tabIndex=0`, Enter = double-click = checkout) with a "…" button. Only a regression test for focusability is missing. |
| `ContextMenu` keyboard model | **Mostly done** (see above). Missing: focus return, Tab, typeahead, Enter/Space tests, native-menu suppression. |
| Shift+F10 / Menu key on branch/remote rows | **Not done.** Branch rows and remote folders rely on the browser turning the key into a `contextmenu` event. WKWebView (macOS Tauri) never does; WebKitGTK and Chromium do, at browser-chosen coordinates. |
| "+" Add menu positioned at `(0,0)` on keyboard activation | **Fixed in PR #72** (opens at the button's rect). No regression test. |
| Palette "Edit remote", "Manage credentials" | **Not done.** Only "Add remote" exists. |

### Right-click-only actions without palette parity

Actions reachable only through a context menu (or its "…" button), with no palette entry and no
always-visible button. "Keyboard after this spec" means reachable by Tab + Menu/Shift+F10.

| Action | Menu | Palette today | Recommendation |
|---|---|---|---|
| Edit remote | remote folder | — | **Add** (issue) |
| Manage credentials | remote folder | — | **Add** (issue) |
| Remove remote | remote folder | — | **Add** (opens the existing confirm, never removes directly) |
| Check out a tag (detached) | tag badge | — (palette checkout needs the commit selected) | **Add** "Check out tag <name> (detached)"; push/delete stay in the Tags panel |
| Rename branch | local branch | — | Defer: needs the inline rename input, which lives in the tree; keyboard path exists after this spec |
| Delete branch | local branch | — | Defer: same, and destructive; keyboard path exists |
| Compare branch with current | local / remote branch | — | Defer: the compare hook is not part of `buildCommands`' inputs; keyboard path exists |
| Isolate branch in graph | local branch | — | Defer (low value) |
| Set as upstream (remote branch) | remote branch | "Browse branches on <remote>" navigates there | Covered |
| Checkout remote branch | remote branch | same | Covered |
| Fetch / Push to a remote | remote folder | `fetch-remote:` / `push-branch:` | Covered |
| Reveal file / Open in merge tool | file menu | editor + diff tool for the current file only | Defer |
| Tab: terminal / reveal | repo tab | `open-terminal`, `reveal-repository` | Covered |
| Commit menu items | commit row | partial (cherry-pick, revert, checkout, reset) | Covered: the graph has a keyboard trigger |

## Design

### 1. Shared helpers: `frontend/src/lib/menuKeys.ts`

```ts
/** True for the keys that open a context menu: the Menu key, or Shift+F10. */
export function isMenuKey(event: { key: string; shiftKey: boolean }): boolean;

/**
 * Where a menu opened by a `contextmenu` event should appear: the pointer when it lies inside
 * `element`'s rect (a real right-click), else the element's bottom-left corner (a keyboard-
 * generated event, whose coordinates are `(0,0)` or browser-chosen).
 */
export function menuAnchor(event: { clientX: number; clientY: number }, element: Element): { x: number; y: number };

/** The element's bottom-left corner, for menus opened by a key or a button. */
export function rectAnchor(element: Element): { x: number; y: number };
```

`RepoTabs`, `CommitGraph` and `DiffPane` switch their inline `event.key === "ContextMenu" ||
(event.shiftKey && event.key === "F10")` checks to `isMenuKey`. Every `onContextMenu` that opens a
`ContextMenu` uses `menuAnchor` instead of raw `clientX/clientY`.

Why "inside the rect" and not `event.button`: a right-click is always inside the element it
targets, while macOS Ctrl+click and keyboard-generated events report `button` inconsistently
across WebKitGTK, WKWebView, WebView2 and Electron.

### 2. `ContextMenu` completes the APG menu pattern

- **Return focus.** The element focused when the menu first renders (`document.activeElement`,
  captured once in a `useState` initializer, before the menu's own focus effect) is the trigger.
  An optional `returnFocusTo?: HTMLElement | null` prop overrides it. Escape, Tab and item
  activation focus it again (when it is still connected) *before* calling `onSelect`/`onClose`,
  so a dialog opened by `onSelect` records the trigger in `useRestoreFocus` and returns there
  when it closes. An outside click does not move focus (the user clicked somewhere else).
- **Escape** is handled in the menu's own `onKeyDown` (with `stopPropagation`, so the graph's
  "Escape collapses a multi-selection" and other ancestor handlers do not also fire). The
  `document` listener stays for the case where focus is outside the menu.
- **Tab / Shift+Tab** close the menu and return focus to the trigger without `preventDefault`,
  so the browser's default Tab action then moves on from the trigger (APG: "Tab closes the menu
  and moves focus to the next element").
- **Enter / Space** are handled explicitly on keydown (`preventDefault`, activate the focused
  enabled item). That makes them testable and stops the native click from firing a second time.
- **Typeahead.** A single printable character (no Ctrl/Meta/Alt) moves focus to the next enabled
  item whose label starts with it, case-insensitively, wrapping. No multi-character buffer.
- **Disabled items** stay skipped by arrows, Home/End, typeahead and initial focus.
- **No enabled items:** the `<ul>` gets `tabIndex={-1}` and receives focus, so Escape and Tab
  still work and assistive tech enters the menu.
- **Native menu suppression.** The `<ul>` handles `onContextMenu` with `preventDefault()` and
  `stopPropagation()`. A Menu-key `contextmenu` that arrives after focus moved into the menu
  then neither opens the webview's native menu nor re-triggers a row's handler. Today's
  right-click handlers rely on the same `preventDefault()` and already keep VSCode's webview menu
  away, so the same is expected here (checked manually, §6).
- **Mouse leave** still does not close the menu, whether focus is inside it or not.
- **Styling.** `.item:focus-visible` gets the same background as `.item:hover`.
- **Submenus:** none exist and none are added. If one is added later it uses ArrowRight to
  open and ArrowLeft/Escape to close, per the APG.

Not changed: the menu stays rendered where its owner renders it (no portal). Moving it to
`document.body` would put it outside a modal `<dialog>`'s top layer (where it would be inert)
for any future menu inside a dialog, and the `role="menu"` inside `CommitGraph`'s `listbox` is
an existing ARIA nesting issue that is out of scope here.

### 3. Keyboard trigger on every row that has a context menu

**`ListRow` gains `onOpenMenu?: (anchor: { x: number; y: number }) => void`.** When set:

- `onContextMenu` → `preventDefault()`, `onOpenMenu(menuAnchor(event, row))`. (The existing
  `onContextMenu` prop stays for callers that need the raw event; passing both is a type error.)
- For a **standalone** row (focusable, `role="button"`): a Menu/Shift+F10 keydown on the row or
  bubbling from a nested button (swatch, "…") → `preventDefault()`, `onOpenMenu(rectAnchor(row))`.
  Keys from `input`, `textarea` and `select` (the rename field) are ignored.
- The row gets `aria-haspopup="menu"` and `aria-keyshortcuts="Shift+F10 ContextMenu"`.
- Container-owned rows (`selected` set) do not get a keyboard handler: their container owns the
  keys, as `CommitGraph` and `DiffPane` already do.

**`BranchTree`:**

- Local and remote-tracking branch rows pass `onOpenMenu` instead of `onContextMenu` (none while
  the row is being renamed).
- The **remote folder header** gets a keydown handler (Menu/Shift+F10 → menu at its rect) and
  `menuAnchor` for right-click. It also gets a sibling "…" button (`Actions for remote <name>`,
  `aria-haspopup="menu"`), shown on hover/focus like the branch rows' "…", so the remote's
  actions are discoverable without knowing Shift+F10 (macOS keyboards have no Menu key).
- Every "…" button and the "+" button get `aria-expanded` (true while their menu is open).
- Escape from any of these menus returns focus to the row, header or button that opened it
  (from §2, no per-call-site code).

**Other menus:**

- `ReadOnlyFileSection`: the header handles Menu/Shift+F10 bubbling from its buttons (Collapse,
  Blame, History, "…") and opens the file menu at the header rect; right-click uses `menuAnchor`.
- `DiffPane`: right-click uses `menuAnchor`. The group's Menu/Shift+F10 handler opens the menu of
  the row that contains the focused element when focus is inside a row, else the selected row
  (today it always uses the selected row, even when Tab put focus on another row's button).
- `CommitGraph`, `RepoTabs`: `isMenuKey` + `menuAnchor`, no behaviour change.
- `CommitGraph` tag badges stay right-click only (they sit inside `role="option"` rows, which
  must not contain focusable widgets); the palette gains "Check out tag" (§5) and the Tags panel
  keeps push/delete.

### 4. Tree focus model: every row stays a Tab stop

The tree keeps today's model: each branch row, its swatch and its "…" are Tab stops, and folder
headers are buttons. A WAI-ARIA tree (`role="tree"`, roving `tabIndex`, arrow keys, Left/Right to
collapse/expand) is a bigger change with its own trade-offs (the nested swatch and "…" controls
need key equivalents) and is left for a follow-up issue. See decision D3.

### 5. Command palette

Remote dialogs move into app state so the palette can open them even while the Branches section
is collapsed (`AccordionSection` does not render its body when closed):

- `AppState.remoteDialog: { kind: "edit" | "credentials" | "remove"; remoteName: string } | null`,
  with `openRemoteDialog(kind, remoteName)` and `closeRemoteDialog()` (no client calls), like
  `addRemoteDraftOpen` / `openAddRemoteDraft`.
- The Edit and Credentials forms move out of `BranchTree`'s body into `EditRemoteDialog.tsx`
  and `RemoteCredentialsDialog.tsx` (their field state initialises from the `RemoteInfo` on
  mount). `BranchTree` renders them, and the Remove confirmation, *after* its
  `AccordionSection` (a fragment), so they show while the section is collapsed.
- `BranchTree` takes optional `remoteDialog` / `onRemoteDialogChange` props: controlled when
  given (App passes app state), local state when omitted (existing tests keep working). Same
  controlled-or-local pattern as `DiffPane`'s `discardRequest` / `onDiscardRequestChange`.
- Remove's "and clear upstreams for …" variant is computed when the dialog renders, from the
  current `remoteUpstreams`, instead of being frozen into a `"clear:<name>"` string when the
  menu item is clicked.

New palette entries, one per remote (like `fetch-remote:<name>`), group "Remotes", omitted while
`repositoryOperationDisabled` (like "Add remote"):

| id | label | keywords | run |
|---|---|---|---|
| `edit-remote:<name>` | `Edit remote <name>…` | remote, edit, url, rename, `<name>` | `openRemoteDialog("edit", name)` |
| `manage-credentials:<name>` | `Manage credentials for <name>…` | remote, credentials, token, ssh, https, auth, `<name>` | `openRemoteDialog("credentials", name)` |
| `remove-remote:<name>` | `Remove remote <name>…` | remote, remove, delete, `<name>` | `openRemoteDialog("remove", name)` |

And one per tag, group "Branches" (next to `switch-branch:`; it is a checkout), omitted while
`repositoryOperationDisabled`, regardless of the Tags panel's visibility (it does not navigate):

| id | label | keywords | run |
|---|---|---|---|
| `checkout-tag:<name>` | `Check out tag <name> (detached)` | tag, checkout, detached, `<name>` | `checkoutCommit(tag.targetId)` |

`TagInfo.targetId` is the annotated tag's target (`tag.target_id()`) or the lightweight tag's
commit, i.e. a commit for every tag the app creates. A tag of a tag would fail in
`checkoutCommit` with its normal error.

### 6. VSCode webview

- Keys: the webview receives Shift+F10 and the Menu key like any page, and its handlers run
  first. VSCode also forwards webview keydowns to the workbench for keybinding dispatch; its
  default Shift+F10 binding (`editor.action.showContextMenu`) is scoped to editor text focus, so
  nothing is expected to fire there. This is an assumption to confirm in the manual check below,
  not something the repo's code shows. No extension change is planned; if the workbench does act
  on the key, the fallback is the visible "…" buttons and the palette, and a follow-up issue.
- Native menus: our handlers `preventDefault()` the `contextmenu` event on rows, headers and the
  menu itself, which is what already keeps VSCode's webview menu away on right-click today.
- Positioning: the webview is an iframe; `getBoundingClientRect()` and `position: fixed` are
  both relative to its viewport, so rect anchors are correct.
- Verified manually (VSCode on Linux, Menu key and Shift+F10, on a branch row, a remote folder
  and a commit). The extension E2E dispatches synthetic `contextmenu` events and cannot exercise
  native key-to-menu behaviour, so no extension E2E is added.

### 7. Shortcut sheet and docs

- `lib/shortcuts.ts`: new group "Context menus": `Menu / Shift+F10` open the focused row's menu
  (branches, remotes, files, commits, tabs); `Up / Down`, `Home / End`, a letter (jump to item),
  `Enter / Space`, `Esc` (close, back to the row), `Tab` (close and move on). "Changed files"
  gains `Menu / Shift+F10`.
- `docs/USER_GUIDE.md`: Branches and remotes (keyboard paragraph; remote "…"), Tags (palette
  checkout), Command palette (new entries).
- `CHANGELOG.md` `[Unreleased]`: "### Keyboard context menus".

## Testing

Frontend: Vitest + Testing Library, mocking `RepoClient` only. `fireEvent` (the repo has no
`user-event`); `getBoundingClientRect` is stubbed with `vi.spyOn(Element.prototype, ...)` where a
test asserts positions.

- `lib/menuKeys.test.ts`: `isMenuKey`, `menuAnchor` inside/outside/`(0,0)`, `rectAnchor`.
- `ContextMenu.test.tsx`: return focus on Escape / Tab / activation; not on outside click;
  disconnected trigger; Tab not `defaultPrevented`; Enter and Space activate once; typeahead
  (match, wrap, skip disabled, ignore modifiers); `contextmenu` on an item is
  `defaultPrevented`; all-disabled menu focuses the `<ul>`; dialog opened from an item restores
  to the trigger.
- `ListRow.test.tsx`: `onOpenMenu` from Menu, Shift+F10, from a nested button, not from an
  input; right-click anchors; attributes; no keyboard handler on container-owned rows.
- `BranchTree.test.tsx`: Shift+F10 / Menu on a local row, a remote-branch row and a remote
  folder open the right menu at the element rect; Escape returns focus there; remote folder "…";
  `aria-expanded` on "…"/"+"; "+" opens at its rect (regression); remote-branch rows focusable
  (regression); controlled `remoteDialog` opens Edit / Credentials / Remove while the section is
  collapsed.
- `EditRemoteDialog.test.tsx`, `RemoteCredentialsDialog.test.tsx`: moved/added form tests.
- `ReadOnlyFileSection.test.tsx`, `DiffPane.test.tsx`: Menu key from a nested button.
- `useAppState.test.ts`: `openRemoteDialog`/`closeRemoteDialog` without client calls.
- `commands.test.ts`: the four new families, their guards, groups.
- `ShortcutSheet.test.tsx`: the new group renders.

Tauri E2E: one new flow, `e2e/specs/keyboard-context-menus.spec.ts` (non-destructive): focus the
current branch row, Shift+F10, first enabled item ("Rename") focused, ArrowDown, Escape, focus back
on the row; then add a remote in `before`, palette "Edit remote origin…", dialog opens, Escape.

No Rust changes. No new dependencies.

## Decisions for the user

Each has a recommendation that the plan implements; the plan notes what changes for the other
options.

- **D1 – How "Edit remote" / "Manage credentials" pick a remote when several exist.**
  A: one entry per remote (`Edit remote origin…`), like Fetch/Push/Browse. B: one entry that
  opens the dialog with a remote picker, like "Delete remote branch…". C: one entry; picker only
  when there is more than one remote. **Recommended: A.**
- **D2 – Extra palette entries.** A: Edit remote + Manage credentials only (the issue). B: A plus
  Remove remote and Check out tag (detached). C: B plus Rename/Delete branch and Compare with
  current branch. **Recommended: B.**
- **D3 – Tree focus model.** A: keep every row a Tab stop (today), add Menu/Shift+F10. B: roving
  `tabIndex` over rows (Up/Down between rows and folder headers, one Tab stop for the tree), with
  swatch and "…" out of the Tab order. C: full WAI-ARIA tree (B plus `role="tree"`/`treeitem`,
  Left/Right collapse/expand, `aria-level`). **Recommended: A now, C as a follow-up issue.**
- **D4 – Typeahead in menus.** A: single-character typeahead. B: none. **Recommended: A.**
- **D5 – Remote folder "…" button.** A: add it (discoverability, macOS). B: keyboard trigger
  only. **Recommended: A.**
- **D6 – Tag badge actions from the keyboard.** A: palette "Check out tag" only; Tags panel keeps
  push/delete. B: add tag items to the commit row's menu when the commit has tags. C: leave as
  is. **Recommended: A** (B makes the commit menu long, with one push item per tag and remote).

## Out of scope

- WAI-ARIA tree semantics for `BranchTree` (D3 B/C).
- The `role="button"` row containing nested buttons (axe `nested-interactive`) and the menu
  nested inside `CommitGraph`'s `listbox`.
- Portalling `ContextMenu`.
- Palette entries for Rename/Delete branch, Compare, Isolate, Reveal file, merge tool.
- Hover moving focus inside the menu.

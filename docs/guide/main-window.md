# The main window

Once a repository is open, the **repository toolbar** runs across the top, and below it the
window has three columns:

- the **sidebar**, with the sections Branches, Stashes, Worktrees, Submodules, Reflog, Tags and
  Pull requests. Branches is open at first; the others start closed.
- the **commit graph**: your history, newest first, with **Uncommitted changes** at the top.
- the **diff pane** on the right: your working tree, or the commit you selected.

**Repository toolbar.** From the left:

- the sidebar button (**Ctrl+Alt+B**, **⌘⌥B** on macOS) hides or shows the sidebar;
- the current branch and the remote branch it tracks ("main → origin/main"). Click it to
  switch branch: type to filter the list, which starts with the branches you used last, then
  the other local branches, then the remote branches that have no local branch yet
  (`origin/feature`), and press Enter. A remote branch is checked out as a new local branch of
  the same name that tracks it. A name that isn't a branch yet offers **Create branch ⟨name⟩
  from ⟨current⟩**, which creates it at your current commit and switches to it in one step (one
  **Undo** takes it back); with nothing typed, **Create branch…** opens the New branch form. The
  command palette's **Switch branch…** opens the same list;
- **Back** and **Forward** through the commits you selected (see [History](history.md));
- **Fetch**, **Pull** and **Push**, with `↑n` (commits to push) and `↓m` (commits to pull) for
  the current branch (see [Branches and remotes](branches-and-remotes.md)). Pull's menu chooses
  how a pull combines diverged branches: **Pull with merge**, **Pull with rebase** or **Pull,
  fast-forward only** (see [Pulling](branches-and-remotes.md#pulling));
- what is running: a fetch, pull or push by name ("Pushing main to origin/main") with a progress
  bar and **Cancel**, or, when another action takes more than a moment, "Working…";
- **Stash** (see [Stashes](stashes.md)), **Apply patch…** (see [Patches](patches.md#apply-a-patch-file)),
  **Undo** and **Redo** (see [Undo and redo](undo-redo.md)), and **Refresh**, which reloads the
  repository's state;
- when the repository was last fetched.

While a repository opens, or after you switch tabs, the graph and the Branches section show grey
placeholder rows until the repository has been read.

**Menus.** A menu (right-click, the **⋯** button, or the Menu key or Shift+F10) is split into
groups by thin lines: in a commit's menu, for example, new branches and tags, applying the commit
elsewhere, moving HEAD, history edits, **Drop**, exports and copying. Destructive items are red
and in a group of their own. Where a key does the same outside the menu, it is shown at the
right of the item, such as `s` for **Stage 3 files** in the file list or **F2** for **Rename
tab…**.

**Dropping files** (desktop app). Drag a folder from your file manager onto the window to open it
as a repository in a new tab, or a `.patch`, `.diff` or `.mbox` file to apply it to the current
repository's working tree, as **Apply patch…** does. While you drag, the window says what a drop
will do. Several items can be dropped at once; patches are applied in order and stop at the
first one that doesn't apply.

Drag the dividers between the columns to resize them. Double-click a divider to collapse the
sidebar, and again to bring it back at its width. With a divider focused, the arrow keys resize
too. The sidebar and the commit graph keep their width when you resize the window; only the
diff pane grows or shrinks.

**Narrow windows.** When the window (or, in VSCode, the editor group) is narrower than 900
pixels, or too narrow for all three columns, the sidebar hides by itself and comes back once
there is room. While it is hidden, the sidebar button, its shortcut, **Show sidebar** in the
command palette, or a double-click on the divider open it as a drawer over the graph. Escape,
the sidebar button again, or a click beside the drawer closes it. Commands that lead to the
sidebar (**Go to Tags**, **New branch…** and the like) open the drawer by themselves.

**Sidebar sections.** Click a section's header to open or close it. The buttons at the top of
the sidebar open every section (**Expand all sections**) or close them all (**Collapse all
sections**). The gear button there lists every section except Branches with a switch, so you can
hide the ones a project doesn't use.

**Header.** The top row holds the repository tabs (see [Multiple repositories](tabs.md)) and, on
the right:

- **Commands** (with **Ctrl+K**, **⌘K** on macOS, beside it) opens the
  [command palette](command-palette.md), the same as its shortcut. It shows once a repository is
  open.
- The moon or sun button switches between the light and dark theme (**Switch to dark theme** /
  **Switch to light theme**). Browsitory remembers your choice. Until you pick one, it follows
  your system's setting; **Theme** under **Appearance** in **Preferences** goes back to that. In
  VSCode the button isn't there: the panel takes the colours and fonts of VSCode's own theme
  (light, dark or high contrast) and changes with it.
- The gear button opens **Preferences**: the commit message settings, **Fetch automatically**
  (see [Branches and remotes](branches-and-remotes.md#fetching-automatically)), diff word wrap
  and diff algorithm (see [Diff options](history.md#diff-options)), your editor, terminal and diff tools (see
  [External tools](external-tools.md)), the **Appearance** settings below and, in the desktop
  app, **Updates** (see below).
- The sparkle button (**What's new**) opens the release notes (see below).
- The question mark button (**Help**) opens a menu: **User guide** opens this guide in your
  browser, **Keyboard shortcuts** shows the list of shortcuts (while a repository is open), and
  **What's new** opens the release notes.

**Appearance.** The Appearance section of Preferences changes how the window and its text look.
Each change shows at once, without a restart:

- **Theme** (desktop app): **System** (the default) follows your system's light or dark setting,
  also when it changes while Browsitory runs; **Light** and **Dark** keep that theme. Both themes
  meet the WCAG 2.2 AA contrast levels. With a high-contrast or forced-colours setting in your
  system (such as a Windows contrast theme), Browsitory uses the system's colours, and the focus
  and the selection stay marked.
- **Code font**: the font for file content and commit IDs: diffs, blame, the file viewer,
  conflicts, and the commit message field while its ruler is shown. Type a font family name, or
  several separated by commas. A font that isn't installed falls back to the default monospace
  font; leave the field empty to use the default.
- **Code font size (px)**: the size of file content in diffs, blame, the file viewer and
  conflicts, from 10 to 24 pixels. Empty keeps each view's built-in size.
- **Interface size**: Small, Normal or Large text for the rest of the window.
- **Tab size**: how many columns a tab character takes in file content, from 1 to 16 (4 at
  first).
- The sparkle button (**What's new**) opens the **Release notes**: what changed in each version. They also open
  by themselves the first time you start a new version. The buttons at the bottom stay in view
  while you scroll: **Close** (or Escape, or a click outside the dialog) closes them, and
  **User guide** opens this guide in your browser.

**Updates (desktop app).** Browsitory checks for a new version when it starts and every six
hours, and downloads it in the background. If a check or download fails (for example while the
download server is down), it tries again after 10 minutes, then waits twice as long each time, up
to 3 hours, until a check succeeds. When it is ready, a banner says "Update v… ready".
Click **Restart to update** to install it, or keep working: nothing changes until you restart.

To stop the automatic checks, clear **Check for updates automatically** under **Updates** in
**Preferences** (it is `check_automatically = false` under `[updates]` in `config.toml`).
Browsitory then contacts the update server only when you ask: click **Check for updates** in
that section, or run **Check for updates** from the command palette. It tells you whether you
are up to date; a new version downloads and shows the same banner.
The VSCode extension updates through VSCode instead and has neither setting nor command.

**Notifications.** A finished action shows a short notification in the bottom-right corner:
green for done, blue for information, amber for a warning, red for an error, each with its own
icon. It closes by itself after 5 to 15 seconds, depending on its length; one with a button
(such as **Undo**) stays at least 10 seconds, and an error stays until you close it. While the
pointer is over a notification or focus is on one of its buttons, none of them close. They never
take focus: reach their buttons with Tab. Screen readers announce them without interrupting.

![The main window: sidebar, commit graph and diff pane](assets/overview.png)

The dark theme:

![The dark theme, during a merge with a conflict](assets/dark-theme.png)

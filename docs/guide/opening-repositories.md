# Open, clone and create repositories

When no repository is open, Browsitory shows the repository picker. **Open folder** opens a
repository from your disk, and the list below it reopens one you used recently. The picker also
has **Clone…**, **New repository…** and your [workspaces](workspaces.md).

The first time you start Browsitory, the picker opens with a welcome: the same buttons to open,
clone or create a repository, the key for the [command palette](command-palette.md), and a
**User guide** button that opens this guide in your browser. **Dismiss** closes the welcome; it
also goes away once you open a repository, and doesn't come back.

**Recent repositories.** Up to ten repositories you opened, newest first. The **×** at the end
of a row takes it off the list (the folder stays where it is); opening the repository again
puts it back. A repository whose folder is gone (moved, deleted, or on a drive that isn't
connected) is marked **Folder not found**. Type in **Filter repositories and workspaces** to
show only the recent repositories and workspaces whose name or path contains the text.

Browsitory reopens the tabs you had open last time. To open another repository next to them,
click **+** at the end of the tab bar. In the desktop app you can also drag a repository's folder
from your file manager onto the window.

## Cloning and creating repositories

In VSCode, **Clone…** and **New repository…** (in the picker and the command palette) run
VSCode's own **Git: Clone** and **Git: Initialize Repository**: VSCode asks for the URL or the
folder and offers to open the result. The rest of this section is about the desktop app.

**Clone…** (repository picker, or the palette's "Clone repository…"):

- **URL** — `https://…`, `ssh://…`, `git@host:org/repo.git`, or a local path / `file://` URL.
- **Parent folder** and **Folder name** — the clone goes into *parent/name*. The name follows
  the URL until you edit it. The parent must be an absolute path (use **Browse…** if unsure). The
  folder must not exist yet, or be empty.
- **Branch** — optional; defaults to the remote's default branch.
- **Include submodules** — on by default. A submodule that can't be fetched doesn't stop the
  clone; the toast says how many failed and the Submodules panel shows them.
- **HTTPS credentials** — for private repositories with an HTTPS (`https://`) URL, enter a
  username and access token. The token is saved to your system keychain, and the remote set to
  use it, **only after the clone succeeds**. SSH URLs use your SSH agent. Don't put the token in
  the URL — Browsitory refuses URLs that contain a password.
  If the keychain write fails, the clone still succeeds and a toast says "Cloned *name*, but the
  token couldn't be saved." — enter the token again later from the remote's **Manage
  credentials** action (right-click the remote in the Branches sidebar).

Progress shows while cloning; **Cancel clone** stops it and removes what was downloaded.

**New repository…** — pick (or create) a folder; Browsitory runs the equivalent of `git init`
there, on your `init.defaultBranch` (or `main`), and opens it. Picking a folder that already
is a repository offers to open it instead.

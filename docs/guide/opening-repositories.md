# Open, clone and create repositories

When no repository is open, Browsitory shows the repository picker. **Open folder** opens a
repository from your disk, and the list below it reopens one you used recently. The picker also
has **Clone…**, **New repository…** and your [workspaces](workspaces.md).

Browsitory reopens the tabs you had open last time. To open another repository next to them,
click **+** at the end of the tab bar. In the desktop app you can also drag a repository's folder
from your file manager onto the window.

## Cloning and creating repositories

Desktop app only — in VSCode, use VSCode's own **Git: Clone** and **Git: Initialize Repository**.

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

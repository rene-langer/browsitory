# Open, clone and create repositories

When no repository is open, Browsitory shows the repository picker. **Open repository…** opens a
repository from your disk, and the list below it reopens one you used recently. The picker also
has **Clone…**, **New repository…** and your [workspaces](workspaces.md).

The first time you start Browsitory, the picker opens with a welcome: the same buttons to open,
clone or create a repository, the key for the [command palette](command-palette.md), and a
**User guide** button that opens this guide in your browser. **Dismiss** closes the welcome; it
also goes away once you open a repository, and doesn't come back.

**Recent repositories.** Up to ten repositories you opened, newest first. The pin button at the
end of a row pins the repository: pinned repositories come first, in the order you pinned them,
stay there however many others you open, and don't count against the ten. Click the pin again
to unpin it; it then goes back on top of the recent ones. The **×** takes a repository off the
list, pinned or not (the folder stays where it is); opening the repository again puts it back.
A repository that leaves the list also loses its recent searches, recent commit messages,
recent branches and **Fetch automatically** / **Prune** choices (see [Privacy](privacy.md)).
A repository whose folder is gone (moved, deleted, or on a drive that isn't connected) is marked
**Folder not found** and doesn't open. **Locate…** in place of the pin lets you pick the folder's
new place, which replaces the entry (pinned again if it was) and opens; **×** removes it. If such
a repository is opened another way, the error asks whether to remove it from the list. Type in
**Filter repositories and workspaces** to show only the recent repositories and workspaces whose
name or path contains the text. The keyboard focus starts in that field, so you can type right
away; with nothing to filter it starts on **Open repository…**.

**A folder that isn't a repository.** Opening a folder that holds no repository itself (no
`.git` folder or file in it) says "This folder isn't a Git repository" and lists the repositories
in its direct subfolders, each with **Open**. Hidden folders and `node_modules` are left out.
When Git refuses a folder that does have a `.git` (one owned by another user, a format it doesn't
support, a broken HEAD), the banner shows Git's message with **Retry**. **Copy details** copies
Git's message in either case.

Browsitory reopens the tabs you had open last time. To open another repository next to them,
click **+** at the end of the tab bar: the picker opens as the **Open repository** dialog (**×**
or Escape closes it). A repository that is already open in a tab is marked **Open in a tab**,
and clicking it switches to that tab. In the desktop app you can also drag a repository's folder
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
  the URL — Browsitory refuses URLs that contain a password. A user name in an `https://` URL
  (`https://alice@bitbucket.org/…`, as Bitbucket copies it) moves to the **Username** field when
  you click **Clone**, so it never lands in the repository's Git config. A user name that looks
  like an access token (`ghp_…`, `github_pat_…`, `glpat-…`, `ATBB…`, or a long random string) is
  taken out of the URL but not shown: the clone doesn't start, and the dialog asks for your
  username and the token in the **Token** field.
  The token goes only to the host in the URL (and its port): when that server sends the clone
  on to another host, the other host gets no token, and the dialog asks for it again. Clone
  from the address the repository moved to instead.
  If the keychain write fails, the clone still succeeds and a toast says "Cloned *name*, but the
  token couldn't be saved." — enter the token again later from the remote's **Manage
  credentials** action (right-click the remote in the Branches sidebar).

Progress shows while cloning; **Cancel clone** stops it and removes what was downloaded. A
clone that gets no answer from the server (still connecting, the server still preparing a big
repository, or the network gone) stops within a few seconds too; what it downloaded is removed
once its connection ends.

**New repository…** — pick (or create) a folder; Browsitory runs the equivalent of `git init`
there, on your `init.defaultBranch` (or `main`), and opens it. Picking a folder that already
is a repository offers to open it instead. Until the first commit, the toolbar's branch button
reads "main (no commits yet)".

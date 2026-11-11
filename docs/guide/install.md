# Install Browsitory

Browsitory comes in two forms with the same features: a desktop app, and an extension that
runs inside VSCode. Neither needs Git installed: Browsitory brings its own Git library. (On
Windows, [hooks](hooks.md) need the shell that comes with Git for Windows.)

## Desktop app

Download the installer for your system from the
[releases page](https://github.com/rene-langer/browsitory/releases/latest):

| System | File |
|---|---|
| Windows (x64) | `Browsitory_<version>_x64-setup.exe` or `Browsitory_<version>_x64_en-US.msi` |
| macOS (Intel and Apple silicon) | `Browsitory_<version>_universal.dmg` |
| Linux (x64) | `.AppImage`, `.deb` (Debian, Ubuntu) or `.rpm` (Fedora, openSUSE) |

On Linux, the app needs glibc 2.39 or newer (for example Ubuntu 24.04, Debian 13, Fedora 40 or
newer) and WebKitGTK 4.1.

The installers are not signed with an Apple or Microsoft certificate yet, so your system may
warn you the first time:

- **macOS:** if macOS says it can't check the app, open **System Settings → Privacy &
  Security** and choose **Open Anyway** next to the message about Browsitory.
- **Windows:** if SmartScreen stops the installer, choose **More info**, then **Run anyway**.

**Updates.** The desktop app checks for new versions by itself and offers to restart when one
is ready (see [The main window](main-window.md)). To check only when you ask, clear **Check for
updates automatically** in **Preferences**. This works for every installer, including
the Linux `.AppImage`, `.deb` and `.rpm`. A `.deb` or `.rpm` update installs the new package for
the whole system, so Linux asks for your password when you restart to update.

## VSCode extension

The extension needs VSCode 1.134 or newer on Linux x64 (with glibc 2.39 or newer, as the app),
macOS (Intel or Apple silicon) or Windows x64. It doesn't run in vscode.dev or in Codespaces in the browser.

- **Visual Studio Marketplace** (VSCode): open the Extensions view and search for
  "Browsitory" and choose **Install**, or run
  `code --install-extension browsitory.browsitory`.
- **Open VSX** (VSCodium, Cursor, Gitpod and other editors that use it): search for
  "Browsitory" in the editor's Extensions view, or see
  <https://open-vsx.org/extension/browsitory/browsitory>.
- **From a `.vsix` file:** each [GitHub release](https://github.com/rene-langer/browsitory/releases)
  has one package per platform. Download the one for your platform and run
  `code --install-extension browsitory-<version>-<platform>.vsix` (or use **Extensions: Install
  from VSIX…**). A `.vsix` install doesn't update itself.

Then run **Browsitory: Open** from VSCode's Command Palette, press Ctrl+K Ctrl+G (Cmd+K Cmd+G
on macOS), or click **Browsitory** in the status bar. It opens the repository of the file in the
active editor. Without one, the first time, it opens your workspace's repositories, each in a
tab, next to the tabs you had open last time: the workspace folders' repositories and those
directly inside a workspace folder (one folder down, also inside another repository). When there
are several, a list asks which ones to open, all ticked. With none of those, pick a repository
folder. Browsitory opens in the active editor group and comes back after you reload the window.

On macOS, Cmd+K clears the terminal while the terminal has the focus, so Cmd+K Cmd+G doesn't
work there. To change the key, open VSCode's **Keyboard Shortcuts** and search for
"Browsitory: Open". To hide the status bar button, turn off the **Browsitory › Status Bar:
Enabled** setting (`browsitory.statusBar.enabled`).

### Browsitory in the VSCode menus

- **Open in Browsitory**: right-click a folder in the Explorer, or choose it in the **…** menu of a
  Git repository in the Source Control view, to open that repository's tab.
- **Show File History in Browsitory**: right-click a file in the Explorer, an editor's tab, or
  inside an editor (also in the Command Palette) to see the file's [history](file-history.md).
- **Show Line History in Browsitory**: select lines in an editor and right-click them to see
  the [history of those lines](file-history.md#history-of-lines). The line numbers count in the
  last commit (HEAD), so in a file with uncommitted changes above the selection the history may
  be of other lines.
- **Show Commit in Browsitory**: right-click a commit in the **Timeline** view (in the Explorer)
  to select that commit in Browsitory's graph. Browsitory loads as much history as it needs, as
  [Go to commit…](history.md) does.

### Keyboard shortcuts in VSCode

VSCode uses some of Browsitory's keys itself (F5 starts debugging, Ctrl+W closes the editor,
Ctrl+K starts a key chord). While Browsitory has the focus, the extension's own keybindings run
those keys in Browsitory instead: the command palette, Refresh, Fetch, F6 between panes, the
sidebar, the tab keys and Back/Forward (see [Keyboard shortcuts](command-palette.md#keyboard-shortcuts)).
Everywhere else they keep doing what VSCode does. To change them, open VSCode's **Keyboard
Shortcuts** and search for "Browsitory".

In VSCode, a few things work the VSCode way: files open in VSCode's editor, diffs in its diff
view, conflicts in its merge editor and terminals in its integrated terminal (see
[External tools](external-tools.md#in-vscode)). To clone or create a repository, use VSCode's
own **Git: Clone** and **Git: Initialize Repository**.

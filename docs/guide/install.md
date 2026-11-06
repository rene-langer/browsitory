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

The installers are not signed with an Apple or Microsoft certificate yet, so your system may
warn you the first time:

- **macOS:** if macOS says it can't check the app, open **System Settings → Privacy &
  Security** and choose **Open Anyway** next to the message about Browsitory.
- **Windows:** if SmartScreen stops the installer, choose **More info**, then **Run anyway**.

**Updates.** The desktop app checks for new versions by itself and offers to restart when one
is ready (see [The main window](main-window.md)). This works for every installer, including
the Linux `.AppImage`, `.deb` and `.rpm`. A `.deb` or `.rpm` update installs the new package for
the whole system, so Linux asks for your password when you restart to update.

## VSCode extension

The extension needs VSCode 1.134 or newer on Linux x64, macOS (Intel or Apple silicon) or
Windows x64. It doesn't run in vscode.dev or in Codespaces in the browser.

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

Then run **Browsitory: Open** from VSCode's Command Palette and pick a repository folder.

In VSCode, a few things work the VSCode way: files open in VSCode's editor, diffs in its diff
view, conflicts in its merge editor and terminals in its integrated terminal (see
[External tools](external-tools.md#in-vscode)). To clone or create a repository, use VSCode's
own **Git: Clone** and **Git: Initialize Repository**.

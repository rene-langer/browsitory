# External tools

## Opening files

Every changed file, every file of a commit and every file in a compare has a "…" button and a
right-click menu with **Open in editor**, **Reveal in file manager** and **Open in diff tool**
(**Open in merge tool** for a conflicted file). The palette has "Open current file in editor"
and "Open current file in diff tool".

Set your editor in **Preferences → External tools** (the gear button in the header, or
"Preferences…" in the palette). The command is split into words the way a shell would, but never run through a
shell, and these placeholders are replaced inside each word:

- `{file}`: the file's full path (added at the end when the command has no `{file}`)
- `{line}`: the first changed line, or 1
- `{dir}`: the repository's root folder

Examples: `code --goto {file}:{line}`, `subl {file}:{line}`, or on Windows
`"C:\Program Files\Sublime Text\subl.exe" {file}:{line}`.

Without an editor setting, the file opens with your system's default app. Files the system
might run instead of open are refused on every platform (a Windows installer can still run on
Linux or macOS through Wine): programs and installers (`.exe`, `.msi`, …), scripts (`.sh`,
`.py`, `.bat`, `.ps1`, …), shortcuts and launchers (`.lnk`, `.desktop`, `.app`, …), disk images
that mount when opened (`.iso`, `.vhd`, `.dmg`, …), also when the name ends in a dot or space
(`setup.exe.`), files with the execute bit on macOS and Linux, and files whose contents start like
a script, program or installer, or, on Linux, like a desktop launcher (unless the name ends in a
document extension such as `.txt` or `.md`). Set an editor to open those.

Don't wrap an editor or terminal command in `sh -c '…'` or `cmd /c …`: Browsitory deliberately
passes the file path as one plain argument, and a shell would read shell syntax in file names
(a repository can contain a file called `$(rm -rf ~).txt`). Commands are checked when you save
them, and one with an unclosed quote is refused.

Apps started from the macOS Dock (or a Linux desktop launcher) may not see your shell's `PATH`.
If an editor, terminal or diff tool isn't found, use its absolute path, for example
`/usr/local/bin/code --goto {file}:{line}` or `git config --global difftool.meld.path
/opt/homebrew/bin/meld`.

## Terminal

Right-click a repository tab (or focus it and press Shift+F10 or the Menu key) for **Open
terminal here** and **Reveal repository**; both are also in the palette. The terminal starts
in the repository's root folder. Without a terminal setting Browsitory uses:

- macOS: Terminal.app
- Windows: Windows Terminal (`wt`), else a `cmd` window
- Linux: `$TERMINAL`, else the first of `x-terminal-emulator`, GNOME Terminal, Konsole, Xfce
  Terminal, Alacritty, kitty and xterm that is installed

A terminal command in Preferences can use `{dir}`, for example `kitty --directory {dir}` or
`wezterm start --cwd {dir}`.

## .gitignore and .gitattributes

The same tab menu has **Open .gitignore** and **Open .gitattributes**, which open the file at
the repository's root folder in your editor; both are also in the palette. When the file doesn't
exist yet, the item reads **Create .gitignore** (or **Create .gitattributes**): Browsitory
creates an empty file at the root and opens it; it never overwrites an existing file. A bare
repository has no work tree, so neither item is offered there. In VSCode the file opens in
VSCode.

## Diff and merge tools

Browsitory uses the diff and merge tools from your git config, like `git difftool` and `git
mergetool`:

```sh
git config --global diff.tool meld
git config --global merge.tool meld
```

Without `diff.tool`, the diff tool falls back to `merge.tool`. These work without more setup:
`meld`, `kdiff3`, `bc` / `bc3` / `bc4` (Beyond Compare), `p4merge`, `vscode` and `winmerge`.
Point one at a program that is not on your `PATH` with `difftool.<name>.path` (or
`mergetool.<name>.path`). Any other tool needs a command, which runs through `sh` with the
`$LOCAL`, `$REMOTE`, `$MERGED` and `$BASE` variables, exactly as in git:

```sh
git config --global difftool.mytool.cmd 'mytool "$LOCAL" "$REMOTE"'
```

The command must wait until its window closes (for example `code --wait`), as with `git
difftool`. Terminal tools such as `vimdiff` are not supported, since Browsitory has no terminal
to run them in; give them a `cmd` that opens a terminal window if you want them anyway.
**Preferences → External tools** shows which tools Browsitory found and where they were set.

Browsitory only reads tool settings from your own git config (global, XDG or system), never
from a repository's `.git/config`: a repository you downloaded or share with others can't make
Browsitory run a command when you click a menu item. If a repository sets one, Preferences and
the error message say so. Hooks are different: they keep git's behaviour, including a
`core.hooksPath` set in the repository's own config (see [Hooks](hooks.md)), because running a
repository's hooks is what git itself does on every commit and push.

What the tool compares:

- an unstaged change: the staged (index) version against the file in your working tree, which
  you can edit in the tool
- a staged change: the last commit against the staged version (the old name for a staged
  rename)
- a commit's file: the commit's first parent against the commit (the old name for a rename)
- a compare: the left side against the right side (or the working tree file)

A missing side (a new or deleted file) is an empty file. The other sides are temporary copies,
removed when the tool closes. They hold the file as `git checkout` would write it: with the
line-ending conversion of `core.autocrlf` and the `text`/`eol` attributes, and `ident`
expanded, so a side matches the working tree file line for line. Filter drivers such as Git LFS
are not run, so an LFS file shows its pointer text. The VSCode diff and merge editors show the
same contents.

## Resolving with a merge tool

**Open in merge tool** on a conflicted file opens your tool with your version (`LOCAL`), the
incoming version (`REMOTE`), their common ancestor (`BASE`) and the working tree file
(`MERGED`) to save the result to. When both sides added the file (there is no ancestor),
`BASE` is empty, except for `p4merge`, which gets git's virtual base as with `git mergetool`:
your version without the lines the incoming version doesn't share (empty when less than half of
it is shared). Other actions keep working while the tool is open. When it
closes, the file shows "The merge tool finished." or, when the tool reported no resolution, "The
merge tool closed without resolving the conflict." With `mergetool.<name>.trustExitCode` set,
the tool's exit code decides; otherwise any change to the file counts as finished. Both notices
offer **Mark resolved**. **Mark resolved**
stages the file; if it still contains `<<<<<<<` or `>>>>>>>` lines, Browsitory asks first.
No `.orig` backup is written.

## In VSCode

In the VSCode extension, everything opens in VSCode itself: **Open in editor** opens the file
at the line, **Open in diff tool** opens VSCode's diff view, **Open in merge tool** opens
VSCode's merge editor (Browsitory can't tell when you are done there, so it shows no
**Mark resolved** notice: save the result, then stage the file from VSCode's merge editor or
Source Control view; Browsitory picks the change up by itself within a few seconds), **Open terminal here** opens an integrated terminal, and
**Reveal** uses "Reveal in File Explorer/Finder". The editor and terminal settings and your git
tools are not used there.

# Troubleshooting

An error says what failed and on what, then Git's reason, then what to do next: "Couldn't
switch to feature/x: your local changes to src/map.js would be overwritten" and below it "Commit
or stash the changes to those files first, or use Stash and switch." It stays at the top of the
window until you dismiss it or start another change (stage, commit, switch branches, …);
refreshing doesn't clear it. **Details** opens the
[operation log](operation-log.md) on the failed operation: what it was given, its full error and
the output of the hooks it ran. When reading the repository failed, there is no operation to
show: the banner has **Copy details** instead, which copies the full error.

The desktop app writes a log file, `browsitory.log`, to your system's log folder, so a bug
report doesn't require a live dev session to reproduce. When the file reaches 40 KB, it is
renamed with the date and time (`browsitory_2026-11-06_14-30-00.log`) and a new one starts; the
5 most recent old files are kept. Attach `browsitory.log` (and the newest old file, if the
problem was earlier) to a bug report:

- **Linux:** `~/.local/share/com.browsitory.desktop/logs` (or
  `$XDG_DATA_HOME/com.browsitory.desktop/logs` if you set `XDG_DATA_HOME`)
- **macOS:** `~/Library/Logs/com.browsitory.desktop`
- **Windows:** `%LOCALAPPDATA%\com.browsitory.desktop\logs`

Up to version 0.3.x these folders were named `com.browsitory.app`. The first start of 0.4.0 or
later renames them, and the log file then names each folder it moved. If a folder could not be
moved, the log says why and the old folder is left as it was.

Report problems on [GitHub](https://github.com/rene-langer/browsitory/issues).

## Another Git program is using the repository

git writes the index, a branch or the config by first creating a lock file next to it
(`.git/index.lock`, `.git/refs/heads/main.lock`, `.git/config.lock`) and removing it when done.
While another Git program (a terminal command, your editor's Git integration, another Git GUI)
holds one, Browsitory can't write that file, and the error says "Another Git program is using the
repository". Wait until the other program finishes, then try again. A program that crashed can
leave its lock file behind: if no Git program is running, delete the file the error names (for
the index, `index.lock` in the `.git` folder), then try again. A fetch, pull or push that stopped
on a lock says the same; **Details** shows the file.

## "Couldn't show …"

If a bug stops Browsitory from drawing a repository's tab, the tab shows "Couldn't show" with the
repository's name and the error instead of going blank. The other tabs and the rest of the window
keep working. **Reload tab** reads the repository again (your commit message draft stays),
**Close tab** closes it, and **Copy details** copies the version and the error for a bug report.
If the window itself can't be drawn, it shows "Browsitory couldn't show this window" with
**Reload**, which opens your repositories again (your commit message drafts stay too). Either way the error also goes to the log.

## An invalid `core.ignorecase`

If the repository's `core.ignorecase` setting holds something other than true or false,
Browsitory can't tell whether names that differ only in case are the same file. Actions that
write files from a commit then stop before they change anything: switching branches, merging,
pulling, cherry-picking, rebasing, saving, applying or popping a stash, resetting, discarding
changes, Abort, undo/redo, cloning, adding a worktree and updating a submodule. Dropping a
stash still works. The error names the setting, for example "Can't read core.ignorecase from
the Git config: failed to parse 'maybe' as a boolean." Set it to true or false, or remove it
(`git config --unset core.ignorecase`),
then try again.

## Files whose names differ only in case

On Linux, with `core.ignorecase` on, Git keeps only one of two files whose names differ only in
case, such as `A` and `a`. Browsitory therefore refuses, before it changes anything, the actions
that would lose one of them: switching to a branch or checking out a commit, tag or remote branch
that has such a pair the current commit doesn't, adding such a worktree, a merge, pull, rebase
step, cherry-pick, revert or stash apply that brings one in, a hard reset, starting a rebase and
discarding one file of the pair. The error names both files. Rename one of them in a commit, then
try again. Aborting a rebase, cherry-pick or revert always works, even with such a pair.

## Git LFS, git-crypt and other filter drivers

A `.gitattributes` line such as `*.psd filter=lfs` or `secrets/** filter=git-crypt` hands those
files to a program that git runs when it stores or writes them: Git LFS stores a small pointer
instead of the file, git-crypt stores it encrypted. Browsitory can't run these programs. So that
it never commits a secret in plain text or writes an LFS pointer over your file, it refuses, before
it changes anything, every action that would store or write such a file: staging it (also with
**Stage all**, hunks and lines), discarding its changes, stashing it or applying a stash that
holds it, marking its conflict resolved or taking a side, and switching branches, checking out a
commit, resetting, merging, pulling, cherry-picking, reverting, rebasing, applying a patch or
undoing when that would write it. The error names the file and the filter, for example
"'secrets/prod.env' uses the 'git-crypt' filter (for example Git LFS or git-crypt), which
Browsitory can't run, so it refused this change. Use git on the command line for it instead."
Actions that don't touch such a file work as usual.

When the repository uses such a filter, the **Changes** list says so at the top ("This repository
uses the 'lfs' filter, which Browsitory can't run.") until you dismiss it with ✕. A changed file
under the filter is marked with the filter's name ("'git-crypt' filter"), its Stage button is
off, and **Stage all** and **Stage modified** leave it out. Its diff compares the file on disk
with the version Git stored (the encrypted bytes, the LFS pointer), so the difference may be only
the filter's; a note above the diff says so.

??? note "Details"

    A filter counts when the Git config (of the repository, or your global one: `git lfs install`
    writes there) sets a command for it (`filter.<name>.clean`, `.smudge` or `.process`) or
    marks it `required`. A `filter=` attribute without such a setting is ignored, as Git ignores
    it, and a symbolic link is never filtered. Git decides whether such a file changed by running
    the filter on it; Browsitory can't, so an encrypted file can show as changed although it
    isn't (after a `touch`, or a checkout by another tool). Git LFS is the exception: the stored
    pointer names the file's size and SHA-256, so a Git LFS file whose content matches it isn't
    listed (only for a filter whose command is `git-lfs`, as `git lfs install` sets it up). The
    notice lists the filters named in the repository's `.gitattributes` files,
    `.git/info/attributes` and the file `core.attributesFile` names, when the Git config sets them
    up: a global `git lfs install` alone shows no notice. A fresh clone or a new worktree writes such files as they are stored (the LFS pointer,
    the encrypted bytes), as Git does without the program installed; run `git lfs pull` or
    `git-crypt unlock` there. Side files for external tools show the stored text too.

## Files with a working-tree-encoding (UTF-16 and others)

A `.gitattributes` line such as `*.ps1 text working-tree-encoding=UTF-16LE-BOM` tells Git to keep
those files as UTF-8 in the repository and to convert them to the named encoding on disk (usual
for PowerShell scripts, `.rc` and `.reg` files and SQL Server scripts). Browsitory can't convert
them: it would commit the raw UTF-16 bytes (Git then sees a binary file) and write UTF-8 where
your tools expect UTF-16. So it treats such a file like one under a filter driver (see above): it
refuses every action that would store or write it, with an error such as "'setup.ps1' uses
working-tree-encoding=UTF-16LE-BOM, which Browsitory can't convert, so it refused this change. Use
git on the command line for it instead." The notice at the top of **Changes** names the encodings
too, a changed file is marked with its encoding ("UTF-16LE-BOM encoding") and its Stage button is
off, and **Stage all** and **Stage modified** leave it out. Its diff compares the file on disk with
the UTF-8 version Git stored, so the difference may be only the encoding's.

??? note "Details"

    `working-tree-encoding=UTF-8` (or `utf8`, any case) converts nothing, so such files work as
    usual. A `working-tree-encoding` without a value makes Git itself fail on the file; Browsitory
    doesn't refuse it. The notice reads the same attribute files as for filters.

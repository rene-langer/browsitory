# Stashes

![The Stashes section](assets/stashes.png)

## Stashing your changes

**Stash** in the repository toolbar sets every change aside, untracked files included. Its
chevron opens **Stash all** and **Stash…**, a dialog with an optional message, **Keep staged
changes** (stash them but also leave them staged) and **Include untracked files** (on by
default).

**Stash file…** in a changed file's menu (staged or not) opens the same dialog for just that
file; every other change stays where it is, and **Apply** or **Pop** later restages only that
file. To stash several files, select them in the change list (see [Selecting several
files](staging-and-committing.md#selecting-several-files)) and use **Stash N files…**.

## Getting a stash back

Expand **Stashes** to see everything set aside. Each stash shows its name and, below it, the
branch it was made on, how long ago, and how many files it holds ("On main · 2 days ago · 3
files"). The name is the message you gave it, or, for a stash without one, "WIP:" and the commit
it was made on. Hover over a stash to read its full message and the date.

Click a stash to see what it holds: **Changes** (diffed against the commit it was made on) and,
if it took any, **Untracked** files. From there or from the panel you can **Apply** it (keep the
stash), **Pop** it (apply, then drop it) or drop it (**Drop…**). In the panel these are menu
items in the stash's menu: open it with the **…** button that shows when you point at or tab to
the stash, by right-clicking the stash, or with the Menu key or Shift+F10.

**Apply** and **Pop** bring back work that was staged as staged (`git stash apply --index`). A
stash that holds staged changes also offers **Apply as unstaged** and **Pop as unstaged**, which
bring everything back unstaged (`git stash apply`). For other stashes the two would do the same,
so only Apply and Pop are shown.

A stash that conflicts with your current files applies anyway and opens the files in the
conflict pane; a conflicting **Pop** keeps the stash, so drop it yourself once the files are
resolved. Dropping asks for confirmation first, since it can't be undone, and stash operations
are not part of **Undo**.

??? note "Details"
    Apply and Pop need nothing staged and no unresolved conflicts; until then the menu items are
    disabled with the reason, and the stash view's buttons show the reason below them while you
    hover or focus them (a screen reader reads it as the button's description). If the stash
    list changed since you looked (another tool stashed or dropped), Apply, Pop and Drop stop
    and ask you to refresh. You can drop a stash while a merge, rebase, cherry-pick or revert is
    paused; while another action or a fetch, pull or push is running, **Drop…** and its
    confirmation are disabled, and the confirmation says why under its question. The command
    palette's **Apply stash**, **Pop stash** and **Drop stash** commands (and **Apply stash as
    unstaged** and **Pop stash as unstaged** for a stash with staged changes) follow the same
    rules.

## When stashing refuses

Stashing refuses, and changes nothing, when it would commit the wrong result or lose work. The
message names the file and what to do. The cases below are rare; open them when a message sends
you here.

??? note "Details: paused operations and conflicts"
    While a rebase or a cherry-pick or revert is paused, stashing waits: the Stash button, the
    dialog's **Stash** and **Stash file…** are disabled with the reason, because continuing after
    stashing the paused step's changes away would commit the wrong result.

    While a merge or another operation has left conflicts, stashing refuses and changes nothing:
    "Resolve the current conflicts before stashing." This applies to Stash all and Stash file…
    alike, as with git.

??? note "Details: one file on its own"
    For an untracked file, untracked files are always included. For a staged rename **Stash
    file…** stashes both names: the new file and the old name's deletion. If you made a new file
    at the old name since, it refuses ("Can't stash 'new' on its own: it was renamed from 'old',
    and 'old' exists again. Stash 'old' too, or move it first."); **Stash file…** on the old
    name stashes both. A file inside a symlinked folder can't be stashed on its own.

??? note "Details: names that differ only in case"
    On macOS, Windows or with `core.ignorecase`, **Stash file…** and **Stash all** refuse a
    staged rename that changes only the case of a name (`Old` to `old`: "Can't stash 'old': it
    differs from 'Old' only in case."); commit it or undo it first. On macOS this includes names
    the drive treats as the same (`Straße` and `Strasse`, `Café` and `CAFÉ`), on Windows `Café`
    and `CAFÉ`. If the repository holds two files whose names differ only in case (`A` and `a`,
    committed on a case-sensitive drive), **Stash all** refuses there too, and so does **Stash
    file…** on either of them ("Can't stash: 'a' and 'A' differ only in case, and stashing would
    remove one of them…"); stashing other files on their own still works.

    On macOS and Windows, where `D` and `d` name the same file, a commit that holds a symbolic
    link and a folder whose names differ only in case (`D` and `d/x`) can't be checked out
    safely: switching to it, merging, pulling, cherry-picking, rebasing, applying a stash,
    resetting, undo/redo, cloning, adding a worktree or updating a submodule refuses ("'D' is a
    symbolic link and 'd' a folder whose names differ only in case…"). Rename one of them in the
    repository (on Linux, or with git). On Linux this only applies when `core.ignorecase` is
    set, or, when adding a worktree, when the worktree's drive or folder ignores case (a USB
    stick, a case-insensitive folder). `ẞ`, `ß` and `ss` count as the same name, and so do names
    that differ only in how an accent is encoded (`é` as one or two code points).

??? note "Details: symbolic links in place of files or folders"
    If a tracked file was replaced by a symbolic link, stashing keeps the link in the stash and
    puts the file back; while a symbolic link replaces a tracked folder, **Stash all** refuses
    (move the link away first). So do switching branches, merging, cherry-picking, reverting,
    rebasing, applying a stash and pulling, which could otherwise create files through the link
    — also when nothing tracked the link's name but the operation would create files below it.
    The message names the link: "'d' is a symbolic link where the repository has a folder.
    Remove or rename it first." Staging a file below such a link stages its deletion (git sees
    no file there; nothing is read through the link), and **Stage all** stages the link itself
    in place of the folder, in any order. Unstaging the link or any of the folder's files puts
    the link and the folder's files back together, since git's index can't hold half of the
    swap. The other way round works the same: when a folder replaced a committed file or link,
    **Stage all** stages that file's deletion and the folder's files in any order, and unstaging
    any of them, the folder's name included, puts the file or link back. Unstaging a staged
    rename whose old path is part of such a swap leaves the swap staged, as `git restore
    --staged` does. While a merge has left conflicts in such a swap, unstaging it refuses and
    changes nothing ("Can't unstage 'd': the merge left conflicts in it. Resolve them first.").
    Discarding a file below the link still refuses ("'d/f' is beyond a symbolic link."), like
    git.

??? note "Details: a file where a folder was"
    When a file replaced a tracked folder (`d` where `d/f` was), **Stash file…** on `d` stashes
    the file together with the folder's deletion and puts the folder back, as `git stash push -u
    -- d` does. Apply and Pop bring the file back: they remove the folder first, then write the
    file, the order git uses. If the folder now holds something the stash doesn't know (a new or
    ignored file), they refuse and keep the stash ("The untracked file 'd' in this stash would
    overwrite the folder 'd'; move the folder first."); a local edit to one of its files is
    protected like any other. If a commit changed one of the folder's files since, they refuse
    too ("This stash can't be applied here: its untracked file 'd' needs the folder 'd' gone,
    but the folder still holds tracked files…"). If a folder was created where a stash holds an
    untracked file, Apply and Pop refuse too ("The untracked file 'd' in this stash would
    overwrite the folder 'd'; move the folder first."). With **Include untracked files** off,
    stashing would delete an untracked file that is in the way of a stashed folder or file (with
    **Keep staged changes** also one in the way of a staged one), so it refuses instead
    ("Stashing would delete the untracked file 'd', which is in the way of 'd/f'. Turn on
    Include untracked files, or move 'd' first."). Where names ignore case (macOS, Windows, or
    `core.ignorecase` on Linux), `docs` counts as in the way of `Docs/f`, and so does a name that
    differs only in Unicode form (`ss` in the way of `ß/f`).

??? note "Details: a folder where a file was"
    When a folder replaced a tracked file (`d/x` where the file `d` was), **Stash all** with
    untracked files stashes `d/x` and the deletion of `d`, and puts the file `d` back. **Stash
    file…** on `d` does the same and takes the files in the folder with it. Apply and Pop delete
    the file `d` first and then bring the folder back. `git stash pop` can't do that with such a
    stash: it writes the untracked files first and stops at the file `d`.

    This differs from git in two more ways. `git stash push -u -- d` stashes only the folder's
    files, not the deletion of `d`, so its own pop fails later; Browsitory records the deletion.
    And when the folder holds anything the stash can't take (an ignored file, a repository, or
    untracked files with **Include untracked files** off), git stashes and then stops half-way,
    with the index changed. Browsitory refuses before stashing anything: "Can't stash 'd': it was
    a file and is now a directory. Commit or move its contents first." The same applies to a
    folder that replaced a symbolic link or a submodule, and to **Stash all** when it works like
    **Stash file…** on every change (see the next two sections).

    Apply and Pop still refuse, and keep the stash, when the file `d` is there for another
    reason: a commit changed it, or added it after the stash was made. They say "This stash
    can't be applied here: its untracked file 'd/x' needs a folder where 'd' is tracked. Commit
    the deletion of 'd' first, then apply it; the stash is kept." A local edit to `d` is
    protected like any other ("Applying this stash would overwrite local changes…").

??? note "Details: ignored files and nested repositories"
    Ignored files and repositories nested in the worktree are never stashed. If one is in the
    way of a file or folder that stashing puts back, **Stash all** would delete it, as git does.
    So it refuses, with or without **Include untracked files**, and stashes nothing: "Stashing
    would delete the ignored 'd', which is in the way of the tracked 'd/f'. Move 'd' first." or
    "Stashing would delete the repository in 'd', which is in the way of the tracked 'd'. Move
    'd' first." Ignored files and repositories elsewhere don't matter. A repository inside an
    untracked folder (`vendor/lib` in a new `vendor/`) stays where it is, with its history, as
    with `git stash -u`: **Stash all** sets aside the folder's other untracked files only.
    Ignored files in an untracked folder stay too. In that case Stash all works like **Stash
    file…** on every change, so it also refuses what that refuses (for a folder that replaced a
    tracked file, see "A folder where a file was").

    A file or folder you removed from the index and then ignored (`.env` after `git rm --cached
    .env`, or `build/` after `git rm -r --cached build`) isn't stashed, and stashing would
    overwrite it with the committed version. So **Stash all** and **Stash file…** refuse, as git
    does: "Stashing would overwrite the ignored '.env' with its committed version. Commit the
    removal of '.env' first, or move it."

    A repository you staged inside this one (`git add lib`, where `lib` has its own `.git`)
    can't be stashed. **Stash all**, and **Stash file…** on `lib`, refuse and change nothing:
    "'lib' is a repository inside this one. Add it as a submodule or remove it from the index
    first." A submodule you added but haven't committed yet gets "Can't stash the new submodule
    'lib'. Commit it first, or remove it from the index." If `lib` is gone, or has no `.git` any
    more, the message asks you to remove `lib` from the index first. **Stash file…** on other
    files still works.

??? note "Details: files removed from the index"
    If you removed a file or folder from the index without ignoring it (`git rm -r --cached d`)
    and then edited it, **Stash all** stashes your edit and puts the committed version back.
    **Pop** brings the edit back, and the removal from the index comes back too (**Pop as
    unstaged** leaves it out). This is the same with **Stash file…**, and for a removal without
    an edit: the file is back in the index afterwards, so Pop isn't refused. With **Keep staged
    changes** the file is deleted, as with git; the edit is still in the stash. Pop works once
    you unstage that deletion and discard it. This applies to any stash made with **Keep staged
    changes**: the staged changes are still staged, so Pop says "Commit or unstage your staged
    changes before applying a stash." until you commit or unstage them. In this case **Stash
    all** works like **Stash file…** on every change, so it also refuses what that refuses (for
    a folder that replaced a tracked file, see "A folder where a file was").

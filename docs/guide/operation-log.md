# Operation log

The operation log lists what Browsitory ran in the repository, newest first: every commit,
checkout, merge, rebase, stash, fetch, pull, push and the rest, and each run of
[your own commands](user-commands.md), with the time it started, how long it took and how it
ended. Reading the repository (opening a commit, a diff, the history) is not listed.

Open it from:

- the command palette: **Show operation log**;
- the **Help** menu (the question mark at the top right): **Operation log**;
- an error at the top of the window: **Show details** opens the log on the newest failed
  operation.

Each row shows the operation's name and what it ran on (a branch, a remote, a file or a commit
message), the time, the duration and an icon for the result:

| Icon | Result |
|---|---|
| … | Still running |
| ✓ | Succeeded |
| ‖ | Stopped part way, as Git does: on conflicts, or a pull whose branch and upstream diverged |
| ✕ | Failed |
| ⊘ | Cancelled |

Select a row to see its details: what it was given (for a push, the remote and whether hooks were
skipped), the error, for a fetch, pull or push what the transfer reported, for a Fetch all the
remotes it couldn't fetch, and every hook and signing program it ran, with its command line and
the end of its output. **Copy** copies a program's output, for example for a bug report.

**Failed only** lists only the failed operations. **Hide automatic fetches** leaves out the
fetches **Fetch automatically** runs in the background (see
[Branches and remotes](branches-and-remotes.md)). The log follows along while it is open: a long
push shows as running and then gets its result. **Load older operations** shows earlier ones.

## Run again

A failed fetch, Fetch all, pull or push shows **Run again**. It runs the operation again with the
same remote, branches, tags and hook choice, as if you had started it from the toolbar or a menu:
progress, errors and notifications show as usual, and the new run is a new row in the log. A
fetch prunes as the repository's **Prune** setting says now.

Run again is not offered for local operations such as a commit, checkout, merge, rebase or
cherry-pick: run them again yourself once you have looked at what went wrong. It is greyed out,
with the reason, while another operation runs or when the remote or branch the operation named
is gone. A force push is not offered again either.

A failed run of [your own commands](user-commands.md) shows **Run again** too: it runs the same
command on the same commit, branch or files, asking first and showing its output as the command
is set up now. It is greyed out once you have deleted the command, and in VS Code in a workspace
you don't trust.

??? note "Details"
    The log is kept in memory only, per repository, for as long as the app runs: up to the last
    500 operations, and up to 16 KB of output for each program. Closing a tab keeps it, quitting
    the app clears it. Nothing is written to disk.

    Nothing secret goes into the log: a URL loses its user name and password
    (`https://***@example.com/repo.git`), an `Authorization` header its value, and saving a token
    records that it was saved, never the token. See [Privacy](privacy.md).

    Cloning and creating a repository are not logged: until they succeed there is no repository
    to keep a log for. Opening files in an editor, terminal or diff tool isn't logged either.

    In the VS Code extension the log works the same way. Each VS Code window has its own log.

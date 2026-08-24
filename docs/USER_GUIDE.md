# Browsitory User Guide

A tour of Browsitory's UI and the day-to-day Git workflows it covers.

## Opening a repository

On launch, Browsitory shows a repo picker: pick a folder, or reopen one of your
recently used repositories.

## Multiple repositories

Every repo you open lands in its own tab at the top of the window, and each
tab keeps its own selection, panel state, and in-flight operations
independent of the others — switch freely without losing your place. Open
another via **+** at the end of the tab bar. A tab with an operation still
running (a push, fetch, or similar) can't be closed until it finishes.
Reopening Browsitory restores every tab you had open, workspace groupings
included.

## Workspaces

A **workspace** is a saved group of repositories under a common root
folder — the multi-repo case most git GUIs don't have an answer for: a
project split across several repos that you always want open together.
From the repo picker, **Open Workspace Root** scans a folder for git repos
and lets you name and save the group; **Open All** reopens every member as
tabs at once, visually grouped in the tab bar under the workspace's name
with a single button to close the whole group together. **Edit** re-scans
the root — so a repo added to the folder later shows up, unchecked, ready
to add — and lets you change which repos are members; **Delete** removes
the saved workspace without touching the repos themselves.

## Overview

Once a repository is open, the window is split into three columns: the
**sidebar** (branches, worktrees, submodules, reflog, remotes, tags, pull
requests — each collapsed by default), the **commit graph** (your history,
newest first), and the **working-directory / diff pane** on the right.

![Overview: sidebar, commit graph, and the working-directory pane](assets/overview.png)

## Staging and committing

The right-hand pane defaults to your working directory, split into
**Changes** and **Staged** groups — each with a bulk **Stage all**/**Unstage
all** action. Hover a file to reveal its stage/unstage toggle, or use
**Blame** for per-line authorship. Click a file to preview its diff, type a
commit message, and hit **Commit**. **Stash** sets the current changes
aside.

![Staging a change, with the diff and commit message visible](assets/staging.png)

## Browsing history and diffs

Click any commit in the graph to see the files it touched; click a file to
view its diff, or **Blame** to see per-line authorship. Commits from every
local branch are shown, each tagged with the branches that point at it.

![Viewing a past commit's diff](assets/commit-diff.png)

## Branches

Expand **Branches** to switch, create, rename, delete, or merge — the
button always shows the current branch. Creating a branch from a specific
commit, or starting a rebase onto one, is available from that commit's row
in the graph.

![The branch switcher, open over the commit graph](assets/branches.png)

## Tags

Expand **Tags** to create lightweight or annotated tags, delete local ones,
and push a selection (or all of them) to a remote.

![The Tags panel: create, delete, and push tags](assets/tags.png)

## Remotes

Expand **Remotes** to add or edit remotes, fetch, push the current branch,
manage saved credentials, and set or clear the current branch's upstream.

![The Remotes panel: origin's URLs and actions](assets/remotes.png)

## Worktrees and submodules

**Worktrees** creates and removes linked worktrees, and opens one directly
in Browsitory. **Submodules** initializes and updates submodules, including
recursively.

## Reflog

**Reflog** lists recent HEAD movements per reference and can restore a
branch to an earlier entry — useful for recovering from a bad reset or
rebase.

## Rebase

Starting a rebase (from a branch or a specific commit) opens the rebase
planner as an overlay, where you can reorder/pick/squash/drop commits
before running it. Conflicts, once they happen, are resolved inline in the
diff pane; a paused rebase shows its progress and lets you continue or
abort.

## Pull requests

For GitHub and Bitbucket remotes, expand **Pull Requests** to save a
personal access token, list existing pull requests, and open a create form
targeting a source/target branch.

## Command palette

Press **Ctrl+K** (or **Cmd+K** on macOS) anywhere to open the command
palette: a fuzzy-searchable list of every action above, ranked by recent
use.

![The command palette, filtered to branch-related commands](assets/command-palette.png)

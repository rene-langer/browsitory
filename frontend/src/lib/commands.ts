import { MAX_CONTEXT_LINES, type DiffSettings, type GraphCommit } from "../ipc/RepoClient";
import { historyEditChecks, type HistoryEditCheck, type HistoryEditRequest } from "./historyEdits";
import type { PaletteAction } from "./actions/action";
import { branchActions } from "./actions/branchActions";
import { commitActions } from "./actions/commitActions";
import { addRemoteAction, remoteActions } from "./actions/remoteActions";
import { stashActions, stashBlockers } from "./actions/stashActions";
import { pruneWorktreesAction, worktreeActions } from "./actions/worktreeActions";
import { busyReason, operationBlockers, stashBlockReason } from "./operationStatus";
import { stageModifiedPaths, untrackedPaths } from "./stagePaths";
import type { UseAppStateResult } from "../state/useAppState";
import type { OpenRepo } from "../state/useOpenRepos";
import { SIDEBAR_PANEL_IDS, type SidebarPanelId } from "../state/useSidebarPanelVisibility";

export interface AppCommands {
  /** Absent where the host can't clone/init (VSCode). */
  onCloneRepository?: () => void;
  onNewRepository?: () => void;
  /** Opens the Preferences dialog ("Preferences…"). */
  onOpenPreferences?: () => void;
  /** Opens the third-party licences dialog ("Third-party licences"). */
  onOpenThirdPartyLicences?: () => void;
  /** Opens the user guide site in the browser ("Open user guide"). */
  onOpenUserGuide?: () => void;
}

/**
 * External tools for the active repository. All read-only, so they stay available while an
 * operation runs.
 */
export interface ToolCommands {
  onOpenTerminal(): void;
  onRevealRepository(): void;
  /** Only offered while `DiffCommands.currentFile` is set. */
  onOpenCurrentFileInEditor(): void;
  onOpenCurrentFileInDiffTool(): void;
}

export interface SearchCommands {
  /** Focuses the commit search field (closing file history first). */
  onSearchCommits(): void;
}

export interface UndoCommands {
  undoLabel: string | null;
  redoLabel: string | null;
  onUndo: () => void;
  onRedo: () => void;
}

export interface Command {
  id: string;
  label: string;
  keywords: string[];
  run: () => void;
  /** Set for a picker ("Go to commit…"): choosing the command lists these items instead of
   *  running `run`. */
  picker?: CommandPicker;
}

/** A second step of the palette: the items to choose from for the typed query (#441). */
export interface CommandPicker {
  placeholder: string;
  /** Shown when `items` is empty. */
  emptyText: string;
  /** The items for `query`, already filtered and in display order. */
  items(query: string): Command[];
}

/**
 * "Go to commit / branch / tag / stash…" (#441): select and scroll to the target without
 * touching the working tree. Read-only, so offered while an operation runs.
 */
export interface GoToCommands {
  onGoToCommit(revision: string): void;
  /** Opens the commit search with `query`, for message text beyond the loaded history. */
  onSearchHistory(query: string): void;
  onGoToBranch(kind: "local" | "remote", name: string): void;
  onGoToTag(name: string, commitId: string | null): void;
  onGoToStash(commitId: string): void;
}

const RECENT_KEY = "command-palette-recent";
const RECENT_LIMIT = 10;
const RESULT_LIMIT = 50;

const SIDEBAR_SECTIONS = [
  "Branches",
  "Stashes",
  "Worktrees",
  "Submodules",
  "Reflog",
  "Tags",
  "Pull requests",
] as const;

// Command ids are stored in the palette's recent list (RECENT_KEY), so a section's go-to id
// must not change when its title does. "Pull requests" was "Pull Requests" until #320.
const GO_TO_COMMAND_IDS: Record<(typeof SIDEBAR_SECTIONS)[number], string> = {
  Branches: "go-to:Branches",
  Stashes: "go-to:Stashes",
  Worktrees: "go-to:Worktrees",
  Submodules: "go-to:Submodules",
  Reflog: "go-to:Reflog",
  Tags: "go-to:Tags",
  "Pull requests": "go-to:Pull Requests",
};

function defaultPanelVisibility(): Record<SidebarPanelId, boolean> {
  return Object.fromEntries(SIDEBAR_PANEL_IDS.map((id) => [id, true])) as Record<SidebarPanelId, boolean>;
}

// "Branches" has no visibility toggle (it's always shown), so it maps to `null` here —
// `SIDEBAR_SECTIONS`' loop below treats `null` as "never hidden".
function sidebarSectionPanelId(title: (typeof SIDEBAR_SECTIONS)[number]): SidebarPanelId | null {
  switch (title) {
    case "Stashes":
      return "stash";
    case "Worktrees":
      return "worktree";
    case "Submodules":
      return "submodule";
    case "Reflog":
      return "reflog";
    case "Tags":
      return "tags";
    case "Pull requests":
      return "pullRequests";
    case "Branches":
      return null;
  }
}

// Remote-tracking branches (Checkout / Set as upstream, from BranchTree's `remoteBranchItems`)
// have no keyboard path at all: their rows only render once a remote's folder is expanded, and
// the folder is never expanded until then, so there's nothing in the DOM yet for a command to act
// on directly (see AUD-2026-09-05-FE-001). Navigating to where the real UI lives rather than
// reimplementing it, this expands the named
// remote's folder inside the Branches section so its branch rows (now real keyboard-focusable
// buttons, each with a "…" affordance) become reachable by Tab immediately after running the
// command.
function expandRemoteBranches(remoteName: string): void {
  const workspace = document.querySelector(`[data-active-repo="true"]`);
  if (workspace === null) return;
  const branchesSection = workspace.querySelector<HTMLElement>(`section[aria-label="Branches"]`);
  if (branchesSection === null) return;

  // First toggle in document order is the "Branches" AccordionSection's own header — expand it
  // before looking for the remote's folder header, which only renders once this is open.
  const sectionToggle = branchesSection.querySelector<HTMLButtonElement>("button[aria-expanded]");
  if (sectionToggle !== null && sectionToggle.getAttribute("aria-expanded") === "false") {
    sectionToggle.click();
  }

  const remoteToggle = Array.from(branchesSection.querySelectorAll<HTMLButtonElement>("button[aria-expanded]")).find(
    (button) => button.textContent?.trim() === remoteName,
  );
  if (remoteToggle === undefined) return;
  if (remoteToggle.getAttribute("aria-expanded") === "false") {
    remoteToggle.click();
  }
  remoteToggle.scrollIntoView({ behavior: scrollBehavior(), block: "nearest" });
}

function goToSidebarSection(title: string): void {
  // Scoped to the active tab's workspace, not the whole document: every open repo's
  // `RepoWorkspace` stays mounted simultaneously (inactive ones are only `display: none`), so a
  // document-wide lookup always resolves to whichever tab is first in document order. On any
  // other tab that silently expanded a hidden tab's accordion and scrolled an invisible element
  // into view — a no-op from the user's side. `RepoWorkspace.tsx` marks the visible workspace with
  // `data-active-repo="true"`.
  const workspace = document.querySelector(`[data-active-repo="true"]`);
  if (workspace === null) return;
  const button = workspace.querySelector<HTMLButtonElement>(
    `section[aria-label="${title}"] button[aria-expanded]`,
  );
  if (button === null) return;
  if (button.getAttribute("aria-expanded") === "false") {
    button.click();
  }
  button.closest("section")?.scrollIntoView({ behavior: scrollBehavior(), block: "nearest" });
}

/** Smooth scrolling unless the user asked the OS for reduced motion. */
function scrollBehavior(): ScrollBehavior {
  return typeof window.matchMedia === "function" && window.matchMedia("(prefers-reduced-motion: reduce)").matches
    ? "auto"
    : "smooth";
}

export interface DiffCommands {
  settings: DiffSettings;
  update(partial: Partial<DiffSettings>): void;
  /** DiffPane's current unstaged file, if any. */
  currentFile: string | null;
  /** Opens the same confirm dialog as the file menu; never discards directly. */
  requestDiscardCurrentFile(): void;
  /** Opens the discard-all confirm dialog; never discards directly. */
  requestDiscardAllUnstaged(): void;
  /** Opens the "Delete untracked…" confirm dialog (#622); never deletes directly. */
  requestDeleteUntracked?(): void;
  /** Opens the current file's history in place of the commit graph. */
  openCurrentFileHistory(): void;
  /** DiffPane's selection of two or more changed files (#449), if any. */
  selection?: { staged: boolean; paths: string[] } | null;
  /** Opens the discard confirm dialog for the selected unstaged files; never discards directly. */
  requestDiscardSelection?(): void;
  /**
   * The current diff's own toggles (#469): the diff last clicked or focused, or the working tree's
   * current file. Absent while no diff is shown.
   */
  currentDiff?: {
    path: string | null;
    canShowWholeFile: boolean;
    toggleWrap(): void;
    toggleWholeFile(): void;
  } | null;
}

function selectedFiles(n: number): string {
  return `${n} selected ${n === 1 ? "file" : "files"}`;
}

/**
 * The commit menu's Cherry-pick, Revert and Reset for the selected commit (#507): the same action
 * functions as the menu (toasts, outcome and sequence handling), and the parent picker and reset
 * dialog opened directly, so they work while the graph row isn't rendered (search results, file
 * history). Without these, the palette leaves the three out.
 */
export interface CommitCommands {
  onCherryPick(commitId: string): void;
  onRevert(commitId: string): void;
  /** A merge commit's pick or revert: opens the parent picker. */
  onChooseParent(action: "Cherry-pick" | "Revert", commitId: string): void;
  /** Opens the reset mode dialog. */
  onReset(commitId: string): void;
}

export function buildCommands(
  appState: UseAppStateResult,
  onOpenRepoTab: (path: string) => void = () => {},
  otherOpenRepos: OpenRepo[] = [],
  onSwitchRepoTab: (path: string) => void = () => {},
  panelVisibility: Record<SidebarPanelId, boolean> = defaultPanelVisibility(),
  onShowShortcuts?: () => void,
  // Closes the palette's own (active) repo tab. The palette is a keyboard path that no host can
  // intercept — unlike Ctrl/Cmd+W, which Tauri's default macOS menu binds to "Close Window" and a
  // VSCode webview may hand to the workbench before the page ever sees it.
  onCloseActiveRepoTab?: () => void,
  diffCommands?: DiffCommands,
  appCommands?: AppCommands,
  undoCommands?: UndoCommands,
  searchCommands?: SearchCommands,
  toolCommands?: ToolCommands,
  goToCommands?: GoToCommands,
  commitCommands?: CommitCommands,
): Command[] {
  const { state } = appState;
  const commands: Command[] = [];

  if (onShowShortcuts !== undefined) {
    commands.push({
      id: "show-shortcuts",
      label: "Show keyboard shortcuts",
      keywords: ["keyboard", "shortcuts", "help", "keys"],
      run: onShowShortcuts,
    });
  }

  if (appCommands !== undefined) {
    if (appCommands.onCloneRepository !== undefined) {
      commands.push({
        id: "clone-repository",
        label: "Clone repository…",
        keywords: ["clone", "repository", "url", "remote"],
        run: appCommands.onCloneRepository,
      });
    }
    if (appCommands.onNewRepository !== undefined) {
      commands.push({
        id: "new-repository",
        label: "New repository…",
        keywords: ["new", "init", "create", "repository"],
        run: appCommands.onNewRepository,
      });
    }
    if (appCommands.onOpenPreferences !== undefined) {
      commands.push({
        id: "preferences",
        label: "Preferences…",
        keywords: ["preferences", "settings", "options", "editor", "terminal", "tools"],
        run: appCommands.onOpenPreferences,
      });
    }
    if (appCommands.onOpenUserGuide !== undefined) {
      commands.push({
        id: "open-user-guide",
        label: "Open user guide",
        keywords: ["help", "user guide", "documentation", "docs", "manual"],
        run: appCommands.onOpenUserGuide,
      });
    }
    if (appCommands.onOpenThirdPartyLicences !== undefined) {
      commands.push({
        id: "third-party-licences",
        label: "Third-party licences",
        keywords: ["licences", "licenses", "notices", "copyright", "about", "open source", "legal"],
        run: appCommands.onOpenThirdPartyLicences,
      });
    }
  }

  // The same rule RepoWorkspace gates every menu and sidebar mutation button behind
  // (`operationBlockers`, #495). Mutating command families must be omitted (not merely disabled)
  // while it blocks; navigation and the in-progress escape hatches
  // (merge-abort/rebase-continue/rebase-abort/refresh) stay.
  const blockers = operationBlockers(state);
  const repositoryOperationDisabled = blockers.blocked;
  // A shared action (`lib/actions`, #495, #509), under its palette label: offered exactly where
  // its menu or panel enables it.
  const offer = (
    id: string,
    action: Pick<PaletteAction, "paletteLabel" | "enabled">,
    keywords: string[],
    run: () => void,
  ) => {
    if (action.enabled) commands.push({ id, label: action.paletteLabel, keywords, run });
  };

  // Read-only, so available even while an operation is running.
  if (searchCommands !== undefined) {
    commands.push({
      id: "search-commits",
      label: "Search commits",
      keywords: ["search", "find", "filter", "author", "grep", "log"],
      run: searchCommands.onSearchCommits,
    });
  }

  if (toolCommands !== undefined) {
    commands.push(
      {
        id: "open-terminal",
        label: "Open terminal here",
        keywords: ["terminal", "shell", "console", "cmd"],
        run: toolCommands.onOpenTerminal,
      },
      {
        id: "reveal-repository",
        label: "Reveal repository",
        keywords: ["reveal", "finder", "explorer", "file manager", "folder"],
        run: toolCommands.onRevealRepository,
      },
    );
    if (diffCommands?.currentFile != null) {
      commands.push(
        {
          id: "open-current-file-in-editor",
          label: "Open current file in editor",
          keywords: ["editor", "open", "edit", "file"],
          run: toolCommands.onOpenCurrentFileInEditor,
        },
        {
          id: "open-current-file-in-diff-tool",
          label: "Open current file in diff tool",
          keywords: ["diff", "tool", "difftool", "compare"],
          run: toolCommands.onOpenCurrentFileInDiffTool,
        },
      );
    }
  }

  if (!repositoryOperationDisabled && undoCommands !== undefined) {
    if (undoCommands.undoLabel !== null) {
      commands.push({
        id: "undo",
        label: `Undo ${undoCommands.undoLabel}`,
        keywords: ["undo", "revert", "back"],
        run: undoCommands.onUndo,
      });
    }
    if (undoCommands.redoLabel !== null) {
      commands.push({
        id: "redo",
        label: `Redo ${undoCommands.redoLabel}`,
        keywords: ["redo", "again"],
        run: undoCommands.onRedo,
      });
    }
  }

  if (!repositoryOperationDisabled && state.upstream !== null) {
    commands.push({
      id: "pull",
      label: "Pull",
      keywords: ["pull", "sync"],
      run: () => void appState.pullCurrentUpstream(),
    });
  }
  // #448: a patch file applied to the working tree (recorded, so behind the repository guard);
  // saving the staged changes only reads.
  if (!repositoryOperationDisabled) {
    commands.push({
      id: "apply-patch",
      label: "Apply patch…",
      keywords: ["apply", "patch", "diff", "mbox", "file", "import"],
      run: () => void appState.applyPatchFile(),
    });
  }
  if (state.status.some((entry) => entry.staged)) {
    commands.push({
      id: "save-staged-patch",
      label: "Save staged changes as patch…",
      keywords: ["save", "patch", "staged", "diff", "export"],
      run: () => void appState.savePatch({ kind: "Staged" }, "staged.patch"),
    });
  }
  // Stashing has its own, narrower rule (`stashBlockReason`): the same one DiffPane's Stash
  // button and the file menu's "Stash file…" follow. It allows stashing during a merge, so these
  // commands are not behind the full repository guard.
  const canStash = stashBlockReason(state) === null;
  if (canStash) {
    // The id stays "save-stash" so recents recorded under the old label keep working.
    commands.push({
      id: "save-stash",
      label: "Stash all changes",
      keywords: ["stash", "save", "all"],
      run: () => void appState.saveStash(),
    });
    commands.push({
      id: "stash-with-options",
      label: "Stash…",
      keywords: ["stash", "save", "message", "untracked", "keep", "index", "options"],
      run: () => appState.openDialog({ kind: "stash", paths: [] }),
    });
  }
  const stashFile = diffCommands?.currentFile ?? null;
  if (canStash && stashFile !== null) {
    commands.push({
      id: "stash-current-file",
      label: `Stash ${stashFile}…`,
      keywords: ["stash", "file", "save", stashFile],
      run: () => appState.openDialog({ kind: "stash", paths: [stashFile] }),
    });
  }
  const fileSelection = diffCommands?.selection ?? null;
  if (canStash && fileSelection !== null) {
    commands.push({
      id: "stash-selected-files",
      label: `Stash ${selectedFiles(fileSelection.paths.length)}…`,
      keywords: ["stash", "selected", "files", "save"],
      run: () => appState.openDialog({ kind: "stash", paths: fileSelection.paths }),
    });
  }
  const prune = pruneWorktreesAction(blockers);
  offer("prune-worktrees", { paletteLabel: prune.label, enabled: prune.enabled }, ["worktree", "prune"], () =>
    void appState.pruneWorktrees(),
  );
  if (state.rebaseProgress !== null) {
    // An interrupted rebase (changed outside Browsitory) can only be aborted.
    if (state.headInfo.rebase?.health.kind !== "Diverged") {
      commands.push({
        id: "rebase-continue",
        label: "Continue rebase",
        keywords: ["rebase", "continue"],
        run: () => void appState.rebaseContinue(),
      });
    }
    commands.push({
      id: "rebase-abort",
      label: "Abort rebase",
      keywords: ["rebase", "abort", "cancel"],
      run: () => void appState.abortRebase(),
    });
  }
  const sequence = state.headInfo.sequence;
  if (sequence !== null) {
    const noun = sequence.kind === "CherryPick" ? "cherry-pick" : "revert";
    commands.push({
      id: "continue-sequence",
      label: `Continue ${noun}`,
      keywords: ["continue", "cherry", "pick", "revert", "sequence"],
      run: () => void appState.continueSequence(),
    });
    commands.push({
      id: "abort-sequence",
      label: `Abort ${noun}`,
      keywords: ["abort", "cancel", "cherry", "pick", "revert", "sequence"],
      run: () => void appState.abortSequence(),
    });
  } else if (state.headInfo.terminalSequence !== null && state.headInfo.operation !== "Merge") {
    // A cherry-pick started in a terminal (#258): Abort ends it like `git cherry-pick --abort`
    // (#253); it is continued in the terminal.
    const noun = state.headInfo.terminalSequence.kind === "CherryPick" ? "cherry-pick" : "revert";
    commands.push({
      id: "abort-sequence",
      label: `Abort ${noun}`,
      keywords: ["abort", "cancel", "cherry", "pick", "revert", "sequence"],
      run: () => void appState.abortSequence(),
    });
  } else if (state.mergeMessage !== null && state.headInfo.terminalRebase === null) {
    // A terminal rebase's conflicted step writes MERGE_MSG too; it is aborted in the terminal
    // (#263).
    commands.push({
      id: "merge-abort",
      label: "Abort merge",
      keywords: ["merge", "abort", "cancel"],
      run: () => void appState.abortMerge(),
    });
  }
  commands.push({
    id: "refresh",
    label: "Refresh",
    keywords: ["refresh", "reload"],
    run: () => void appState.refresh(),
  });

  const selected = state.selectedRow === "uncommitted" ? null : state.selectedRow.commitId;
  const selectedCommit = selected === null ? undefined : state.commits.find((c) => c.id === selected);
  const currentBranch = state.branches.find((branch) => branch.isCurrent)?.name ?? null;
  if (selectedCommit !== undefined) {
    // The commit menu's actions, with the same availability (`commitActions`, #495): the palette
    // leaves out what the menu disables.
    const actions = commitActions({ blockers, headId: state.headInfo.headId, currentBranch }, selectedCommit);
    const commitId = selectedCommit.id;
    // The menu's own actions (#507); a merge commit's pick and revert ask for the parent first.
    if (commitCommands !== undefined) {
      offer("cherry-pick-selected", actions.cherryPick, ["cherry", "pick", "apply", "commit"], () =>
        actions.cherryPick.needsParent
          ? commitCommands.onChooseParent("Cherry-pick", commitId)
          : commitCommands.onCherryPick(commitId),
      );
      offer("revert-selected", actions.revert, ["revert", "undo", "commit"], () =>
        actions.revert.needsParent ? commitCommands.onChooseParent("Revert", commitId) : commitCommands.onRevert(commitId),
      );
    }
    offer("checkout-selected", actions.checkout, ["checkout", "detach", "commit"], () =>
      void appState.checkoutCommit(commitId),
    );
    // The keyboard path for the commit menu's "Create tag…" (#438). Only opens the dialog, whose
    // Create button carries the busy rule, so it is offered while an operation runs.
    offer("create-tag-selected", actions.createTag, ["tag", "create", "new", "release", "commit"], () =>
      appState.openDialog({ kind: "createTag", commitId }),
    );
    // Reset asks for the mode in its dialog.
    if (commitCommands !== undefined) {
      offer("reset-selected", actions.reset, ["reset", "soft", "mixed", "hard"], () => commitCommands.onReset(commitId));
    }
  }
  if (state.headInfo.headId !== null) {
    const headId = state.headInfo.headId;
    commands.push({
      id: "create-tag-head",
      label: "Create tag at HEAD…",
      keywords: ["tag", "create", "new", "release", "head"],
      run: () => appState.openDialog({ kind: "createTag", commitId: headId }),
    });
  }

  if (!repositoryOperationDisabled) {
    if (selectedCommit !== undefined) {
      const isHead = state.headInfo.headId === selectedCommit.id;
      // The commit menu's one-click history edits (#442), only where the menu enables them.
      // HEAD's Reword… is "Amend last commit" below.
      const commitId = selectedCommit.id;
      const short = selectedCommit.shortId;
      const checks = historyEditChecks(
        {
          commits: state.commits,
          headInfo: state.headInfo,
          currentBranch,
          status: state.status,
        },
        commitId,
      );
      const edit = (request: HistoryEditRequest) => (onto: string) => void appState.editHistory(commitId, onto, request);
      const edits: [string, string, HistoryEditCheck, string[], (onto: string) => void][] = [
        ["reword-selected", `Reword ${short}…`, isHead ? { ok: false, reason: "" } : checks.Reword, ["reword", "message"],
          (onto) => appState.openDialog({ kind: "reword", commitId, onto })],
        ["squash-selected", `Squash ${short} with parent`, checks.Squash, ["squash", "combine", "parent"], edit({ kind: "Squash" })],
        ["fixup-selected", `Fixup ${short} into parent`, checks.Fixup, ["fixup", "squash", "parent"], edit({ kind: "Fixup" })],
        ["move-up-selected", `Move ${short} up`, checks.MoveUp, ["move", "up", "reorder"], edit({ kind: "MoveUp" })],
        ["move-down-selected", `Move ${short} down`, checks.MoveDown, ["move", "down", "reorder"], edit({ kind: "MoveDown" })],
        ["drop-selected", `Drop ${short}`, checks.Drop, ["drop", "remove"], edit({ kind: "Drop" })],
      ];
      for (const [id, label, check, keywords, run] of edits) {
        if (!check.ok) continue;
        const { onto } = check;
        commands.push({ id, label, keywords: [...keywords, "edit", "rebase", "commit"], run: () => run(onto) });
      }
    }
    // The keyboard path for a tag badge's "Delete from remote…" (#439); the dialog confirms.
    if (state.remotes.length > 0) {
      for (const tag of state.tags) {
        commands.push({
          id: `delete-remote-tag:${tag.name}`,
          label: `Delete tag ${tag.name} from remote…`,
          keywords: ["tag", "delete", "remove", "remote", tag.name],
          run: () => appState.openDialog({ kind: "deleteRemoteTag", tagName: tag.name }),
        });
      }
    }
    if (state.headInfo.headId !== null) {
      commands.push({
        id: "amend-last-commit",
        label: "Amend last commit",
        keywords: ["amend", "edit", "message", "commit"],
        run: () => {
          appState.selectRow("uncommitted");
          appState.setAmendMode(true);
        },
      });
    }
    if (state.upstream !== null && state.headInfo.detachedAt === null) {
      const remoteName = state.upstream.remoteName;
      commands.push({
        id: "force-push",
        label: "Force push current branch…",
        keywords: ["force", "push", "lease"],
        run: () => appState.openDialog({ kind: "forcePush", remoteName }),
      });
    }
  }

  // Changes no repository state (only what the next commit passes), so it is offered even while
  // other operations are blocked.
  if (state.headInfo.commitHooks.length > 0) {
    commands.push({
      id: "toggle-skip-hooks",
      label: state.skipHooks ? "Run hooks for the next commit" : "Skip hooks for the next commit",
      keywords: ["hooks", "skip", "no-verify", "bypass", "pre-commit", "commit-msg"],
      run: () => {
        appState.selectRow("uncommitted");
        appState.setSkipHooks(!state.skipHooks);
      },
    });
  }

  // The branch menu's Checkout and Merge (`branchActions`, #509): every branch but the current one.
  const branchContext = { blockers, remoteCount: state.remotes.length, upstream: state.upstream };
  for (const branch of state.branches) {
    const actions = branchActions(branchContext, branch);
    offer(`switch-branch:${branch.name}`, actions.checkout, ["branch", "switch", "checkout", branch.name], () =>
      void appState.switchBranch(branch.name),
    );
    offer(`merge-branch:${branch.name}`, actions.merge, ["branch", "merge", "squash", branch.name], () =>
      appState.openDialog({ kind: "merge", branchName: branch.name }),
    );
  }

  if (state.remotes.length > 0) {
    if (!repositoryOperationDisabled) {
      commands.push({
        id: "fetch-all-remotes",
        label: "Fetch all remotes",
        keywords: ["fetch", "all", "remotes", "prune"],
        run: () => void appState.fetchAllRemotes(),
      });
    }
    const current = state.branches.find((branch) => branch.isCurrent);
    if (current !== undefined && state.headInfo.detachedAt === null) {
      offer(
        "push-branch-to",
        branchActions(branchContext, current).pushTo,
        ["push", "publish", "upstream", "remote", "branch"],
        () => appState.openDialog({ kind: "push", localBranch: current.name }),
      );
    }
    if (!repositoryOperationDisabled) {
      commands.push({
        id: "delete-remote-branch",
        label: "Delete remote branch…",
        keywords: ["delete", "remove", "remote", "branch"],
        run: () => appState.openDialog({ kind: "deleteRemoteBranch", remoteName: null, branchName: null }),
      });
    }
  }

  for (const remote of state.remotes) {
    // The remote folder menu's actions (`remoteActions`, #509). App state owns their dialogs, so
    // they open even while the Branches section is collapsed.
    const actions = remoteActions(blockers, remote.name);
    offer(`fetch-remote:${remote.name}`, actions.fetch, ["fetch", "remote", remote.name], () =>
      void appState.fetchRemote(remote.name),
    );
    offer(`push-branch:${remote.name}`, actions.pushCurrentBranch, ["push", "remote", remote.name], () =>
      void appState.pushCurrentBranch(remote.name),
    );
    if (!repositoryOperationDisabled) {
      commands.push({
        id: `push-tags:${remote.name}`,
        label: `Push all tags to ${remote.name}`,
        keywords: ["push", "tags", "remote", remote.name],
        run: () => void appState.pushTags(remote.name, []),
      });
      // Checkout/Set-as-upstream for a specific remote-tracking branch (BranchTree's
      // `remoteBranchItems`) have no keyboard path at all otherwise — see
      // AUD-2026-09-05-FE-001 and `expandRemoteBranches` above.
      commands.push({
        id: `browse-remote-branches:${remote.name}`,
        label: `Browse branches on ${remote.name}`,
        keywords: ["remote", "branch", "checkout", "upstream", "browse", remote.name],
        run: () => expandRemoteBranches(remote.name),
      });
    }
    offer(`edit-remote:${remote.name}`, actions.edit, ["remote", "edit", "url", "rename", remote.name], () =>
      appState.openDialog({ kind: "remote", action: "edit", remoteName: remote.name }),
    );
    offer(
      `manage-credentials:${remote.name}`,
      actions.credentials,
      ["remote", "credentials", "token", "ssh", "https", "auth", "password", remote.name],
      () => appState.openDialog({ kind: "remote", action: "credentials", remoteName: remote.name }),
    );
    // Opens the same confirmation as the remote's menu; never removes directly.
    offer(`remove-remote:${remote.name}`, actions.remove, ["remote", "remove", "delete", remote.name], () =>
      appState.openDialog({ kind: "remote", action: "remove", remoteName: remote.name }),
    );
  }

  // Only opens the form, whose Add remote button carries the busy rule (`addRemoteAction`).
  offer("add-remote", addRemoteAction(), ["remote", "add"], () => appState.openAddRemoteDraft());

  // Every loop below is also skipped while its sidebar panel is hidden (the new sidebar
  // panel-visibility toggles fully unmount hidden panels, not just CSS-hide them) — otherwise
  // these commands either navigate to a section that no longer renders (Tags/Worktrees'
  // `goToSidebarSection` calls) or silently mutate state the user can't see the result of
  // (stash/submodule). "Branches" has no toggle, so it's never filtered.

  if (!repositoryOperationDisabled) {
    // The keyboard path for the tag badge menu's "Checkout (detached)". Not tied to the Tags
    // panel's visibility: it doesn't navigate there.
    for (const tag of state.tags) {
      const commitId = tag.commitId;
      if (commitId === null) continue; // a tag of a tree or blob has nothing to check out
      commands.push({
        id: `checkout-tag:${tag.name}`,
        label: `Check out tag ${tag.name} (detached)`,
        keywords: ["tag", "checkout", "detached", tag.name],
        run: () => void appState.checkoutCommit(commitId),
      });
    }

    // Deleting a tag has a real confirmation dialog in TagPanel — the palette
    // must not skip it. This stays discoverable by tag name but only navigates
    // to the Tags section; the actual mutation happens behind the confirmation.
    if (panelVisibility.tags) {
      for (const tag of state.tags) {
        commands.push({
          id: `delete-tag:${tag.name}`,
          label: `Delete tag ${tag.name}`,
          keywords: ["tag", "delete", tag.name],
          run: () => goToSidebarSection("Tags"),
        });
      }
    }
  }

  if (panelVisibility.stash) {
    // The Stashes panel's Apply, Pop and Drop… (`stashActions`, #509): Drop stays available while
    // resolving a paused step (#521). Showing a stash only selects it, so it is always offered.
    const storedStashBlockers = stashBlockers(state);
    for (const stash of state.stashes) {
      const actions = stashActions(storedStashBlockers, stash);
      commands.push({
        id: `show-stash:${stash.index}`,
        label: `Show stash: ${stash.message}`,
        keywords: ["stash", "show", "view", "diff"],
        run: () => appState.selectRow({ commitId: stash.commitId }),
      });
      offer(`apply-stash:${stash.index}`, actions.apply, ["stash", "apply"], () => void appState.applyStash(stash.index));
      offer(`pop-stash:${stash.index}`, actions.pop, ["stash", "pop", "apply"], () => void appState.popStash(stash.index));
      // Dropping has a confirmation in the Stashes panel and the stash view; like "Delete tag", the
      // palette only shows the stash and opens the section, never drops directly.
      offer(`drop-stash:${stash.index}`, actions.drop, ["stash", "drop", "delete"], () => {
        appState.selectRow({ commitId: stash.commitId });
        goToSidebarSection("Stashes");
      });
    }
  }

  if (panelVisibility.worktree) {
    for (const worktree of state.worktrees) {
      // The main worktree's remove is refused (`MAIN_WORKTREE`), so only its open is offered (#561).
      const actions = worktreeActions(blockers, worktree);
      offer(`open-worktree:${worktree.path}`, actions.open, ["worktree", "open", worktree.name], () =>
        onOpenRepoTab(worktree.path),
      );
      // Removing a worktree has a real confirmation dialog in WorktreePanel — navigate there
      // instead of removing directly from the palette.
      offer(`remove-worktree:${worktree.name}`, actions.remove, ["worktree", "remove", "delete", worktree.name], () =>
        goToSidebarSection("Worktrees"),
      );
    }
  }

  if (!repositoryOperationDisabled) {
    if (panelVisibility.submodule) {
      for (const submodule of state.submodules) {
        if (submodule.initialized) {
          commands.push({
            id: `update-submodule:${submodule.path}`,
            label: `Update submodule ${submodule.path}`,
            keywords: ["submodule", "update", submodule.path],
            run: () => void appState.updateSubmodule(submodule.path, false),
          });
        } else {
          commands.push({
            id: `init-submodule:${submodule.path}`,
            label: `Initialize submodule ${submodule.path}`,
            keywords: ["submodule", "init", "initialize", submodule.path],
            run: () => void appState.initSubmodule(submodule.path),
          });
        }
        // Like "Open worktree": a tab of its own (#445). Only a checked-out submodule has a
        // folder to open; for the others the commands above offer Initialize / Update.
        const workdir = submodule.workdir;
        if (workdir !== null) {
          commands.push({
            id: `open-submodule:${submodule.path}`,
            label: `Open submodule ${submodule.path} in new tab`,
            keywords: ["submodule", "open", "tab", submodule.path],
            run: () => onOpenRepoTab(workdir),
          });
        }
      }
    }

    // Restoring a reflog entry has a real confirmation dialog in ReflogPanel,
    // and state.reflog is unbounded (a full, uncapped reflog read for the
    // selected reference) — so this is not a per-entry loop. The existing
    // "Go to Reflog" command below covers navigating there; no separate
    // restore-reflog command is emitted.
  }

  if (diffCommands !== undefined) {
    const { settings, update } = diffCommands;
    commands.push(
      {
        id: "toggle-ignore-whitespace",
        label: "Toggle ignore whitespace",
        keywords: ["diff", "whitespace", "ignore"],
        run: () => update({ ignoreWhitespace: !settings.ignoreWhitespace }),
      },
      {
        id: "increase-context-lines",
        label: "Increase context lines",
        keywords: ["diff", "context", "lines", "more"],
        run: () => update({ contextLines: Math.min(settings.contextLines + 1, MAX_CONTEXT_LINES) }),
      },
      {
        id: "decrease-context-lines",
        label: "Decrease context lines",
        keywords: ["diff", "context", "lines", "fewer", "less"],
        run: () => update({ contextLines: Math.max(settings.contextLines - 1, 0) }),
      },
      {
        id: "toggle-syntax-highlighting",
        label: "Toggle syntax highlighting",
        keywords: ["diff", "syntax", "highlight", "colors"],
        run: () => update({ syntaxHighlight: !settings.syntaxHighlight }),
      },
    );
    const currentDiff = diffCommands.currentDiff ?? null;
    if (currentDiff !== null) {
      const of = currentDiff.path === null ? "" : ` (${currentDiff.path})`;
      const fileKeywords = currentDiff.path === null ? [] : [currentDiff.path];
      commands.push({
        id: "toggle-wrap-lines",
        label: `Toggle wrap lines${of}`,
        keywords: ["diff", "wrap", "lines", "word", "long", ...fileKeywords],
        run: currentDiff.toggleWrap,
      });
      if (currentDiff.canShowWholeFile) {
        commands.push({
          id: "toggle-show-whole-file",
          label: `Toggle show whole file${of}`,
          keywords: ["diff", "whole", "file", "full", "context", ...fileKeywords],
          run: currentDiff.toggleWholeFile,
        });
      }
    }
    if (diffCommands.currentFile !== null) {
      commands.push({
        id: "file-history-current-file",
        label: "File history of current file",
        keywords: ["history", "log", "file", "follow", "rename"],
        run: diffCommands.openCurrentFileHistory,
      });
    }
    // A selection of several files (#449): the same actions as its selection bar. Staging is not
    // behind the full repository guard (it is how a merge's resolutions get staged), only behind
    // another action in flight, as the selection bar is.
    const selection = diffCommands.selection ?? null;
    if (selection !== null && !state.pending) {
      const paths = selection.paths;
      commands.push(
        selection.staged
          ? {
              id: "unstage-selected-files",
              label: `Unstage ${selectedFiles(paths.length)}`,
              keywords: ["unstage", "selected", "files", "reset"],
              run: () => void appState.unstageAllFiles(paths),
            }
          : {
              id: "stage-selected-files",
              label: `Stage ${selectedFiles(paths.length)}`,
              keywords: ["stage", "add", "selected", "files"],
              run: () => void appState.stageAllFiles(paths),
            },
      );
    }
    // The Changes heading's "Stage modified" (#621): tracked files' changes only, no untracked
    // files and no conflicts; one undo step, like "Stage all".
    const modifiedPaths = stageModifiedPaths(state.status);
    if (modifiedPaths.length > 0 && !state.pending) {
      commands.push({
        id: "stage-modified-files",
        label: "Stage modified files (tracked only)",
        keywords: ["stage", "add", "modified", "tracked", "update", "changes"],
        run: () => void appState.stageAllFiles(modifiedPaths),
      });
    }
    // These only open the confirm dialogs; staged and conflicted entries have no discard action.
    // Like the diff pane's discard, allowed while resolving a merge or a paused step: only an
    // action in flight or a transfer blocks it (`busyReason`, #521).
    if (busyReason(state) === null) {
      if (selection !== null && !selection.staged && diffCommands.requestDiscardSelection !== undefined) {
        commands.push({
          id: "discard-selected-files",
          label: `Discard ${selectedFiles(selection.paths.length)}…`,
          keywords: ["discard", "revert", "selected", "files"],
          run: diffCommands.requestDiscardSelection,
        });
      }
      if (diffCommands.currentFile !== null) {
        commands.push({
          id: "discard-current-file",
          label: "Discard changes in current file",
          keywords: ["discard", "revert", "file"],
          run: diffCommands.requestDiscardCurrentFile,
        });
      }
      if (state.status.some((entry) => !entry.staged && entry.kind !== "Conflicted")) {
        commands.push({
          id: "discard-all-unstaged",
          label: "Discard all unstaged changes",
          keywords: ["discard", "revert", "all", "unstaged"],
          run: diffCommands.requestDiscardAllUnstaged,
        });
      }
      if (untrackedPaths(state.status).length > 0 && diffCommands.requestDeleteUntracked !== undefined) {
        commands.push({
          id: "delete-untracked-files",
          label: "Delete untracked files…",
          keywords: ["delete", "remove", "clean", "untracked", "new", "files"],
          run: diffCommands.requestDeleteUntracked,
        });
      }
    }
  }

  if (goToCommands !== undefined) commands.push(...goToPickers(appState, goToCommands));

  for (const title of SIDEBAR_SECTIONS) {
    const panelId = sidebarSectionPanelId(title);
    if (panelId !== null && !panelVisibility[panelId]) continue;
    commands.push({
      id: GO_TO_COMMAND_IDS[title],
      label: `Go to ${title}`,
      keywords: ["go", "navigate", title.toLowerCase()],
      run: () => goToSidebarSection(title),
    });
  }

  // Omitted (not disabled) while an operation is in flight, the same rule the tab's own close
  // button and Ctrl/Cmd+W follow — closing would orphan the operation mid-way.
  if (onCloseActiveRepoTab !== undefined && !repositoryOperationDisabled) {
    commands.push({
      id: "close-tab",
      label: "Close tab",
      keywords: ["close", "tab", "repo", "repository"],
      run: onCloseActiveRepoTab,
    });
  }

  for (const repo of otherOpenRepos) {
    commands.push({
      id: `switch-repo:${repo.path}`,
      label: `Switch to ${repo.displayName}`,
      keywords: ["repo", "switch", "tab", repo.displayName],
      run: () => onSwitchRepoTab(repo.path),
    });
  }

  return commands;
}

const PICKER_KEYWORDS = ["go", "go to", "jump", "find", "show", "reveal", "select", "navigate"];

function goToPickers(appState: UseAppStateResult, goTo: GoToCommands): Command[] {
  const { state } = appState;
  const pickers: Command[] = [];
  const noop = () => {};

  pickers.push({
    id: "go-to-commit",
    label: "Go to commit…",
    keywords: [...PICKER_KEYWORDS, "commit", "sha", "hash", "message", "revision"],
    run: noop,
    picker: {
      placeholder: "SHA, message text, or any revision…",
      emptyText: "No loaded commit matches",
      items: (query) => commitPickerItems(state.commits, query, goTo),
    },
  });

  const remoteNames = new Set<string>();
  for (const branch of state.branches) if (branch.upstream != null) remoteNames.add(branch.upstream);
  for (const name of state.graphSelection.remote ?? []) remoteNames.add(name);
  for (const commit of state.commits) for (const name of commit.remoteBranchRefs) remoteNames.add(name);
  const branchItems: Command[] = [
    ...state.branches.map((branch) => ({
      id: `branch:${branch.name}`,
      label: branch.isCurrent ? `${branch.name} (current)` : branch.name,
      keywords: [branch.name],
      run: () => goTo.onGoToBranch("local", branch.name),
    })),
    ...Array.from(remoteNames)
      .sort()
      .map((name) => ({
        id: `remote-branch:${name}`,
        label: name,
        keywords: [name, "remote"],
        run: () => goTo.onGoToBranch("remote", name),
      })),
  ];
  if (branchItems.length > 0) {
    pickers.push({
      id: "go-to-branch",
      label: "Go to branch…",
      keywords: [...PICKER_KEYWORDS, "branch", "tip"],
      run: noop,
      picker: {
        placeholder: "Branch name…",
        emptyText: "No matching branches",
        items: (query) => filterPickerItems(branchItems, query),
      },
    });
  }

  if (state.tags.length > 0) {
    const tagItems = state.tags.map((tag) => ({
      id: `tag:${tag.name}`,
      label: tag.name,
      keywords: [tag.name],
      run: () => goTo.onGoToTag(tag.name, tag.commitId),
    }));
    pickers.push({
      id: "go-to-tag",
      label: "Go to tag…",
      keywords: [...PICKER_KEYWORDS, "tag", "release", "version"],
      run: noop,
      picker: {
        placeholder: "Tag name…",
        emptyText: "No matching tags",
        items: (query) => filterPickerItems(tagItems, query),
      },
    });
  }

  if (state.stashes.length > 0) {
    const stashItems = state.stashes.map((stash) => ({
      id: `stash:${stash.commitId}`,
      label: stash.message,
      keywords: [stash.message, `stash@{${stash.index}}`],
      run: () => goTo.onGoToStash(stash.commitId),
    }));
    pickers.push({
      id: "go-to-stash",
      label: "Go to stash…",
      keywords: [...PICKER_KEYWORDS, "stash"],
      run: noop,
      picker: {
        placeholder: "Stash message…",
        emptyText: "No matching stashes",
        items: (query) => filterPickerItems(stashItems, query),
      },
    });
  }
  return pickers;
}

/**
 * Loaded commits whose SHA starts with the query or whose summary contains it, newest first.
 * A typed query also offers a lookup in the whole repository (a SHA or revision that isn't
 * loaded, or is on a hidden branch) and the commit search (message text in older history).
 */
export function commitPickerItems(commits: GraphCommit[], query: string, goTo: GoToCommands): Command[] {
  const q = query.trim();
  const needle = q.toLowerCase();
  const matches =
    needle === ""
      ? commits
      : commits.filter((commit) => commit.id.startsWith(needle) || commit.summary.toLowerCase().includes(needle));
  const items: Command[] = matches.slice(0, RESULT_LIMIT).map((commit) => ({
    id: `commit:${commit.id}`,
    label: `${commit.shortId} ${commit.summary}`,
    keywords: [],
    run: () => goTo.onGoToCommit(commit.id),
  }));
  if (q === "") return items;
  // After the loaded matches (first when there are none). A revision has no spaces; one that a
  // loaded SHA already starts with needs no lookup.
  if (!/\s/.test(q) && !commits.some((commit) => commit.id.startsWith(needle))) {
    items.push({
      id: "lookup-revision",
      label: `Look up “${q}” in the whole repository`,
      keywords: [],
      run: () => goTo.onGoToCommit(q),
    });
  }
  items.push({
    id: "search-history",
    label: `Search all history for “${q}”`,
    keywords: [],
    run: () => goTo.onSearchHistory(q),
  });
  return items;
}

/** Picker items matching `query` (best match first, else in the given order). */
export function filterPickerItems(items: Command[], query: string): Command[] {
  const scored: { item: Command; score: number; index: number }[] = [];
  items.forEach((item, index) => {
    const score = scoreCommand(item, query);
    if (score !== null) scored.push({ item, score, index });
  });
  scored.sort((a, b) => b.score - a.score || a.index - b.index);
  return scored.slice(0, RESULT_LIMIT).map((entry) => entry.item);
}

// Heading order used when the palette shows its unfiltered, grouped view.
export const COMMAND_GROUPS = ["Branches", "Remotes", "Stash", "Diff", "Go to", "Repositories", "General"] as const;
export type CommandGroup = (typeof COMMAND_GROUPS)[number];

/** Derives a command's display group from its id; unknown ids fall into "General". */
export function commandGroup(id: string): CommandGroup {
  if (id.startsWith("switch-branch:") || id.startsWith("merge-branch:") || id.startsWith("checkout-tag:")) {
    return "Branches";
  }
  if (
    id.startsWith("fetch-remote:") ||
    id.startsWith("push-branch:") ||
    id.startsWith("push-tags:") ||
    id.startsWith("browse-remote-branches:") ||
    id.startsWith("edit-remote:") ||
    id.startsWith("manage-credentials:") ||
    id.startsWith("remove-remote:") ||
    id.startsWith("delete-remote-tag:") ||
    id === "add-remote" ||
    id === "force-push"
  ) {
    return "Remotes";
  }
  if (
    id === "save-stash" ||
    id === "stash-with-options" ||
    id === "stash-current-file" ||
    id === "stash-selected-files" ||
    id.startsWith("show-stash:") ||
    id.startsWith("apply-stash:") ||
    id.startsWith("pop-stash:") ||
    id.startsWith("drop-stash:")
  ) {
    return "Stash";
  }
  if (
    id === "toggle-ignore-whitespace" ||
    id === "increase-context-lines" ||
    id === "decrease-context-lines" ||
    id === "toggle-syntax-highlighting" ||
    id === "discard-current-file" ||
    id === "discard-all-unstaged" ||
    id === "delete-untracked-files" ||
    id === "stage-selected-files" ||
    id === "stage-modified-files" ||
    id === "unstage-selected-files" ||
    id === "discard-selected-files" ||
    id === "open-current-file-in-editor" ||
    id === "open-current-file-in-diff-tool"
  ) {
    return "Diff";
  }
  if (id.startsWith("go-to:") || id.startsWith("go-to-")) return "Go to";
  if (id.startsWith("switch-repo:") || id === "close-tab" || id === "open-terminal" || id === "reveal-repository") {
    return "Repositories";
  }
  return "General";
}

function scoreCommand(command: Command, query: string): number | null {
  const q = query.trim().toLowerCase();
  if (q === "") return 0;
  const label = command.label.toLowerCase();
  // The whole label typed ("go to branch" for "Go to branch…") beats a longer label that merely
  // starts with it ("Go to Branches").
  if (label === q || label.replace(/…$/, "") === q) return 4;
  if (label.startsWith(q)) return 3;
  if (command.keywords.some((keyword) => keyword.toLowerCase().startsWith(q))) return 2;
  if (label.includes(q) || command.keywords.some((keyword) => keyword.toLowerCase().includes(q))) return 1;
  return null;
}

export function filterAndSortCommands(commands: Command[], query: string): Command[] {
  const recent = loadRecentCommandIds();
  const scored: { command: Command; score: number }[] = [];
  for (const command of commands) {
    const score = scoreCommand(command, query);
    if (score !== null) scored.push({ command, score });
  }
  scored.sort((a, b) => {
    if (a.score !== b.score) return b.score - a.score;
    const aRecent = recent.indexOf(a.command.id);
    const bRecent = recent.indexOf(b.command.id);
    if (aRecent !== bRecent) {
      if (aRecent === -1) return 1;
      if (bRecent === -1) return -1;
      return aRecent - bRecent;
    }
    return a.command.label.localeCompare(b.command.label);
  });
  return scored.slice(0, RESULT_LIMIT).map((entry) => entry.command);
}

export function loadRecentCommandIds(): string[] {
  const stored = localStorage.getItem(RECENT_KEY);
  if (stored === null) return [];
  try {
    const parsed: unknown = JSON.parse(stored);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((id): id is string => typeof id === "string");
  } catch {
    return [];
  }
}

export function recordCommandUsed(id: string): void {
  const recent = loadRecentCommandIds().filter((existing) => existing !== id);
  recent.unshift(id);
  localStorage.setItem(RECENT_KEY, JSON.stringify(recent.slice(0, RECENT_LIMIT)));
}

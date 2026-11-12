import { MAX_CONTEXT_LINES, type DiffSettings, type GraphCommit, type PullStrategy } from "../ipc/RepoClient";
import { historyEditChecks, type HistoryEditCheck, type HistoryEditRequest } from "./historyEdits";
import { UNAVAILABLE_REASON, type PaletteAction } from "./actions/action";
import { branchActions, rebaseOntoAction } from "./actions/branchActions";
import { commitActions } from "./actions/commitActions";
import { addRemoteAction, remoteActions } from "./actions/remoteActions";
import { stashActions, stashBlockers, stashTitle } from "./actions/stashActions";
import { pruneWorktreesAction, worktreeActions } from "./actions/worktreeActions";
import { busyReason, operationBlockers, stashBlockReason, type OperationBlockers } from "./operationStatus";
import { stageAllPaths, stageModifiedPaths, untrackedPaths } from "./stagePaths";
import type { BlockChoice } from "./conflictMerge";
import { mergeEditorShortcut, mergeEditorShortcutLabel, type MergeEditorShortcutId, type ShortcutId } from "./keybindings";
import type { AppState, UseAppStateResult } from "../state/useAppState";
import type { OpenRepo } from "../state/useOpenRepos";
import { matchItem } from "./fuzzyMatch";
import { branchNameProblem } from "./refName";
import { repoNameOf } from "./repoName";
import type { RootFileItem } from "./rootFiles";
import type { SidebarPanelId } from "../state/useSidebarPanelVisibility";
import type { SidebarSectionId } from "../state/useSidebarReveal";

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
  /** Reopens the most recently closed tab ("Reopen closed tab"); absent when none was closed. */
  onReopenClosedTab?: () => void;
  /** Opens the active tab's rename dialog ("Rename tab…", #625). */
  onRenameTab?: () => void;
  /** Checks for a new version now ("Check for updates", #630); desktop app only. */
  onCheckForUpdates?: () => void;
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
  /** "Open .gitignore" / "Create .gitignore" and the same for `.gitattributes` (#624); none when bare. */
  rootFiles?: RootFileItem[];
  onOpenRootFile?(item: RootFileItem): void;
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

/**
 * The repository tabs: open a folder (a worktree, a checked-out submodule) in a tab of its own,
 * switch to another open tab, or close this one.
 */
export interface TabCommands {
  onOpenRepoTab(path: string): void;
  /** The open tabs other than this one, for "Switch to …". */
  otherOpenRepos: OpenRepo[];
  onSwitchRepoTab(path: string): void;
  /**
   * Closes the palette's own (active) repo tab. The palette is a keyboard path that no host can
   * intercept — unlike Ctrl/Cmd+W, which Tauri's default macOS menu binds to "Close Window" and a
   * VSCode webview may hand to the workbench before the page ever sees it.
   */
  onCloseActiveRepoTab(): void;
}

/**
 * Where the sidebar commands lead (#494): requests the owning components handle
 * (`useSidebarReveal`), never a lookup in the rendered sidebar.
 */
export interface SidebarCommands {
  /** Which sidebar panels are shown. A hidden panel is unmounted, so its commands are left out. */
  panelVisibility: Record<SidebarPanelId, boolean>;
  /** Expands the section and scrolls it into view. */
  revealSection(section: SidebarSectionId): void;
  /** Expands the remote's folder in the Branches section and scrolls it into view. */
  revealRemote(remoteName: string): void;
  /** Shows or hides the sidebar, as a drawer in a narrow layout (#635); `shown` names the command. */
  toggle?: { shown: boolean; run(): void };
}

/** The toolbar's branch switcher (#638), the "Switch branch…" picker. */
export interface BranchSwitchCommands {
  /** Local branches switched to lately, most recent first. */
  recentBranches: readonly string[];
  onSwitchBranch(name: string): void;
  /** Creates the branch at HEAD and switches to it. */
  onCreateAndSwitch(name: string): void;
  /**
   * The remotes' branches (`listRemoteBranches`, loaded while the palette is open, #726); those
   * without a local branch of the same name are listed after the local ones.
   */
  remoteBranches: readonly { remoteName: string; branchName: string }[];
  /** Creates the local branch from `remoteName/branchName`, tracking it, and switches to it. */
  onCheckoutRemoteBranch(remoteName: string, branchName: string): void;
}

/** The id of the "Switch branch…" picker, which the toolbar's branch chip opens directly. */
export const SWITCH_BRANCH_PICKER_ID = "switch-branch";

/** A command's heading in the palette's unfiltered view, in display order. */
export const COMMAND_GROUPS = ["Branches", "Remotes", "Stash", "Diff", "Go to", "Repositories", "General"] as const;
export type CommandGroup = (typeof COMMAND_GROUPS)[number];

/** An item to run: a picker's choice ("Go to branch…" → main), and the base of every command. */
export interface PickerItem {
  /** Stored in the palette's recent list, so it must not change when the label does. */
  id: string;
  label: string;
  keywords: string[];
  run: () => void;
}

export interface Command extends PickerItem {
  /** Set by the family builder that makes the command. */
  group: CommandGroup;
  /**
   * Why the command can't run now (#644, #664): set only on the unavailable commands
   * `buildPaletteCommands` adds, which a search lists greyed out with this reason.
   */
  disabledReason?: string;
  /** Set for a picker ("Go to commit…"): choosing the command lists these items instead of
   *  running `run`. */
  picker?: CommandPicker;
  /**
   * The global shortcut that runs this command (#646): the palette shows its keys, and
   * `useWorkspaceShortcuts` runs the command when they are pressed. Set by `buildAllCommands`.
   */
  shortcut?: ShortcutId;
  /**
   * Keys its pane binds while it has the focus (#764): shown like `shortcut`, but the palette and
   * `useWorkspaceShortcuts` bind nothing for them.
   */
  keys?: string;
}

/** A second step of the palette: the items to choose from for the typed query (#441). */
export interface CommandPicker {
  placeholder: string;
  /** Shown when `items` is empty. */
  emptyText: string;
  /** The items for `query`, already filtered and in display order. */
  items(query: string): PickerItem[];
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
  /** Back/Forward through the visited commits (#618); absent at that end. */
  onGoBack?(): void;
  onGoForward?(): void;
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
  /** The open merge editor (#764), what the palette can do in it; absent while none is shown. */
  mergeEditor?: MergeEditorCommands | null;
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

/** The merge editor's moves and choices (#764), for its current conflict. */
export interface MergeEditorCommands {
  path: string;
  canGoPrevious: boolean;
  canGoNext: boolean;
  previous(): void;
  next(): void;
  /** Takes `choice` for the current conflict (the first, before one is current) and focuses it. */
  choose(choice: BlockChoice): void;
}

const MERGE_EDITOR_CHOICES: [MergeEditorShortcutId, BlockChoice][] = [
  ["conflict-ours", "ours"],
  ["conflict-theirs", "theirs"],
  ["conflict-ours-then-theirs", "oursThenTheirs"],
  ["conflict-theirs-then-ours", "theirsThenOurs"],
  ["conflict-base", "base"],
];

/**
 * The commit menu's Cherry-pick, Revert and Reset for the selected commit (#507): the same action
 * functions as the menu (toasts, outcome and sequence handling), and the parent picker and reset
 * dialog opened directly, so they work while the graph row isn't rendered (search results, file
 * history). Without these, the palette leaves the three out.
 */
export interface CommitCommands {
  /**
   * The commits the open search results or file history list (#515). A selected commit there
   * that the loaded graph lacks still gets the commit menu's actions, from its own data.
   */
  listedCommits?: readonly GraphCommit[];
  onCherryPick(commitId: string): void;
  onRevert(commitId: string): void;
  /** A merge commit's pick or revert: opens the parent picker. */
  onChooseParent(action: "Cherry-pick" | "Revert", commitId: string): void;
  /** Opens the reset mode dialog. */
  onReset(commitId: string): void;
}

/**
 * Everything the palette's commands act on (#494). Only `appState` is required: each other
 * family is offered only when its handlers are given.
 */
export interface CommandContext {
  appState: UseAppStateResult;
  onShowShortcuts?: () => void;
  /** Shows the working tree and focuses the commit message (#646). */
  onFocusCommitMessage?: () => void;
  app?: AppCommands;
  tabs?: TabCommands;
  sidebar?: SidebarCommands;
  diff?: DiffCommands;
  undo?: UndoCommands;
  search?: SearchCommands;
  tools?: ToolCommands;
  goTo?: GoToCommands;
  commit?: CommitCommands;
  branchSwitch?: BranchSwitchCommands;
}

/** `CommandContext` plus what several families derive from the repository state. */
interface FamilyContext extends CommandContext {
  state: AppState;
  /** The rule every menu and sidebar mutation button follows (`operationBlockers`, #495). */
  blockers: OperationBlockers;
  /**
   * Mutating command families are omitted (not merely disabled) while it is set; navigation and
   * the in-progress escape hatches (merge-abort/rebase-continue/rebase-abort/refresh) stay.
   */
  blocked: boolean;
  panelVisible(panel: SidebarPanelId): boolean;
}

const RECENT_KEY = "command-palette-recent";
const RECENT_LIMIT = 10;
const RESULT_LIMIT = 50;

/**
 * The sidebar's sections in display order. Command ids are stored in the palette's recent list
 * (RECENT_KEY), so a section's go-to id must not change when its title does: "Pull requests"
 * was "Pull Requests" until #320.
 */
const SIDEBAR_SECTIONS: { section: SidebarSectionId; title: string; id: string }[] = [
  { section: "branches", title: "Branches", id: "go-to:Branches" },
  { section: "stash", title: "Stashes", id: "go-to:Stashes" },
  { section: "worktree", title: "Worktrees", id: "go-to:Worktrees" },
  { section: "submodule", title: "Submodules", id: "go-to:Submodules" },
  { section: "reflog", title: "Reflog", id: "go-to:Reflog" },
  { section: "tags", title: "Tags", id: "go-to:Tags" },
  { section: "pullRequests", title: "Pull requests", id: "go-to:Pull Requests" },
];

type CommandSpec = Omit<Command, "group">;

/** Collects one family's commands, stamping each with the family's group. */
function family(group: CommandGroup) {
  const commands: Command[] = [];
  return {
    commands,
    add(spec: CommandSpec, inGroup: CommandGroup = group) {
      commands.push({ ...spec, group: inGroup });
    },
    /**
     * A shared action (`lib/actions`, #495, #509), under its palette label: offered exactly where
     * its menu or panel enables it.
     */
    offer(
      id: string,
      action: Pick<PaletteAction, "paletteLabel" | "enabled"> & { reason?: string },
      keywords: string[],
      run: () => void,
      inGroup: CommandGroup = group,
    ) {
      // A disabled action is kept with its reason; `buildCommands` leaves it out and
      // `buildPaletteCommands` lists it as unavailable (#644).
      commands.push({
        id,
        label: action.paletteLabel,
        keywords,
        run,
        group: inGroup,
        ...(action.enabled ? {} : { disabledReason: action.reason ?? UNAVAILABLE_REASON }),
      });
    },
  };
}

function selectedFiles(n: number): string {
  return `${n} selected ${n === 1 ? "file" : "files"}`;
}

/** The palette's commands for the current state, each family's from its own builder. */
export function buildCommands(context: CommandContext): Command[] {
  return buildAllCommands(context, operationBlockers(context.appState.state)).filter(
    (command) => command.disabledReason === undefined,
  );
}

/**
 * `buildCommands` plus the commands that can't run now (#644), each with its `disabledReason`:
 * actions their rule disables, and the commands left out while an operation blocks the
 * repository (`operationBlockers`), with the blocker's reason. The palette lists them only for a
 * typed query, after the runnable matches, so a search for "Push" says why there is none.
 */
export function buildPaletteCommands(context: CommandContext): Command[] {
  const blockers = operationBlockers(context.appState.state);
  const commands = buildAllCommands(context, blockers);
  const available = commands.filter((command) => command.disabledReason === undefined);
  const availableIds = new Set(available.map((command) => command.id));
  const unavailable = new Map<string, Command>();
  const addUnavailable = (command: Command, reason: string) => {
    if (!availableIds.has(command.id) && !unavailable.has(command.id)) {
      unavailable.set(command.id, { ...command, disabledReason: reason });
    }
  };
  for (const command of commands) {
    if (command.disabledReason !== undefined) addUnavailable(command, command.disabledReason);
  }
  if (blockers.blocked) {
    // The same families as if nothing blocked: what the real list lacks, the blocker keeps out.
    const blockedReason = blockers.reason ?? UNAVAILABLE_REASON;
    for (const command of buildAllCommands(context, { blocked: false, reason: null })) {
      addUnavailable(command, blockedReason);
    }
  }
  return [...available, ...unavailable.values()];
}

function buildAllCommands(context: CommandContext, blockers: OperationBlockers): Command[] {
  const { state } = context.appState;
  const visibility = context.sidebar?.panelVisibility;
  const ctx: FamilyContext = {
    ...context,
    state,
    blockers,
    blocked: blockers.blocked,
    panelVisible: (panel) => visibility?.[panel] ?? true,
  };
  return withShortcuts(state, [
    ...appCommands(ctx),
    ...undoCommands(ctx),
    ...workingTreeCommands(ctx),
    ...operationCommands(ctx),
    ...historyCommands(ctx),
    ...branchCommands(ctx),
    ...remoteCommands(ctx),
    ...tagCommands(ctx),
    ...stashCommands(ctx),
    ...worktreeCommands(ctx),
    ...submoduleCommands(ctx),
    ...diffCommands(ctx),
    ...goToCommands(ctx),
    ...repositoryCommands(ctx),
  ]);
}

/**
 * Marks the commands the global shortcuts run (#646). Fetch and Push are the toolbar's: Fetch
 * from the upstream's remote (else the first remote), Push to the upstream, or the Push dialog
 * for a branch without one.
 */
function withShortcuts(state: AppState, commands: Command[]): Command[] {
  const fetchRemote = state.upstream?.remoteName ?? state.remotes[0]?.name;
  const ids = new Map<string, ShortcutId>([
    ["show-shortcuts", "show-shortcuts"],
    ["refresh", "refresh"],
    ["pull", "pull"],
    [state.upstream !== null ? `push-branch:${state.upstream.remoteName}` : "push-branch-to", "push"],
    ["stage-all-files", "stage-all"],
    ["focus-commit-message", "focus-commit-message"],
    ["undo", "undo"],
    ["redo", "redo"],
    ["toggle-sidebar", "toggle-sidebar"],
    ["close-tab", "close-tab"],
    ["reopen-closed-tab", "reopen-closed-tab"],
    ["go-back", "go-back"],
    ["go-forward", "go-forward"],
  ]);
  if (fetchRemote !== undefined) ids.set(`fetch-remote:${fetchRemote}`, "fetch");
  return commands.map((command) => {
    const shortcut = ids.get(command.id);
    return shortcut === undefined ? command : { ...command, shortcut };
  });
}

/** App-wide commands: help, dialogs and the commit search. */
function appCommands({ onShowShortcuts, onFocusCommitMessage, app, search, appState, state }: FamilyContext): Command[] {
  const { commands, add } = family("General");
  if (onShowShortcuts !== undefined) {
    add({
      id: "show-shortcuts",
      label: "Show keyboard shortcuts",
      keywords: ["keyboard", "shortcuts", "help", "keys"],
      run: onShowShortcuts,
    });
  }
  if (app?.onCloneRepository !== undefined) {
    add({
      id: "clone-repository",
      label: "Clone repository…",
      keywords: ["clone", "repository", "url", "remote"],
      run: app.onCloneRepository,
    });
  }
  if (app?.onNewRepository !== undefined) {
    add({
      id: "new-repository",
      label: "New repository…",
      keywords: ["new", "init", "create", "repository"],
      run: app.onNewRepository,
    });
  }
  if (app?.onOpenPreferences !== undefined) {
    add({
      id: "preferences",
      label: "Preferences…",
      keywords: ["preferences", "settings", "options", "editor", "terminal", "tools"],
      run: app.onOpenPreferences,
    });
  }
  if (app?.onOpenUserGuide !== undefined) {
    add({
      id: "open-user-guide",
      label: "Open user guide",
      keywords: ["help", "user guide", "documentation", "docs", "manual"],
      run: app.onOpenUserGuide,
    });
  }
  if (app?.onCheckForUpdates !== undefined) {
    add({
      id: "check-for-updates",
      label: "Check for updates",
      keywords: ["update", "upgrade", "new version", "release", "download"],
      run: app.onCheckForUpdates,
    });
  }
  if (app?.onOpenThirdPartyLicences !== undefined) {
    add({
      id: "third-party-licences",
      label: "Third-party licences",
      keywords: ["licences", "licenses", "notices", "copyright", "about", "open source", "legal"],
      run: app.onOpenThirdPartyLicences,
    });
  }
  // Read-only, so available even while an operation is running.
  if (search !== undefined) {
    add({
      id: "search-commits",
      label: "Search commits",
      keywords: ["search", "find", "filter", "author", "grep", "log"],
      run: search.onSearchCommits,
    });
  }
  // Only moves the focus, so offered while an operation runs.
  if (onFocusCommitMessage !== undefined) {
    add(
      {
        id: "focus-commit-message",
        label: "Go to commit message",
        keywords: ["commit", "message", "write", "focus", "working tree", "uncommitted"],
        run: onFocusCommitMessage,
      },
      "Go to",
    );
  }
  // The graph's "Collapse merges" toggle (#617); a view setting, so never blocked.
  add({
    id: "toggle-collapse-merges",
    label: state.collapseMerges ? "Show every commit of merged branches" : "Collapse merges",
    keywords: ["collapse", "expand", "merges", "first parent", "graph", "history", "side branches"],
    run: () => void appState.setCollapseMerges(!state.collapseMerges),
  });
  return commands;
}

function undoCommands({ undo, blocked }: FamilyContext): Command[] {
  const { commands, add } = family("General");
  if (blocked || undo === undefined) return commands;
  if (undo.undoLabel !== null) {
    add({ id: "undo", label: `Undo ${undo.undoLabel}`, keywords: ["undo", "revert", "back"], run: undo.onUndo });
  }
  if (undo.redoLabel !== null) {
    add({ id: "redo", label: `Redo ${undo.redoLabel}`, keywords: ["redo", "again"], run: undo.onRedo });
  }
  return commands;
}

/**
 * Pull and patches: what changes or exports the working tree as a whole. The patches are listed
 * under "Diff", Pull under "Remotes" with fetch and push (#592).
 */
function workingTreeCommands({ appState, state, blocked }: FamilyContext): Command[] {
  const { commands, add } = family("Diff");
  if (!blocked && state.upstream !== null) {
    add(
      { id: "pull", label: "Pull", keywords: ["pull", "sync"], run: () => void appState.pullCurrentUpstream() },
      "Remotes",
    );
    // The Pull menu's strategies (#652): each is remembered for this repository, then pulls.
    const strategies: [PullStrategy, string, string[]][] = [
      ["Merge", "Pull with merge", ["merge"]],
      ["Rebase", "Pull with rebase", ["rebase"]],
      ["FastForwardOnly", "Pull, fast-forward only", ["fast-forward", "ff", "only"]],
    ];
    for (const [strategy, label, keywords] of strategies) {
      add(
        {
          id: `pull-strategy:${strategy}`,
          label,
          keywords: ["pull", "sync", "strategy", ...keywords],
          run: () =>
            void (async () => {
              await appState.setPullStrategy(strategy);
              await appState.pullCurrentUpstream();
            })(),
        },
        "Remotes",
      );
    }
  }
  // #448: a patch file applied to the working tree (recorded, so behind the repository guard);
  // saving the staged changes only reads.
  if (!blocked) {
    add({
      id: "apply-patch",
      label: "Apply patch…",
      keywords: ["apply", "patch", "diff", "mbox", "file", "import"],
      run: () => void appState.applyPatchFile(),
    });
  }
  if (state.status.some((entry) => entry.staged)) {
    add({
      id: "save-staged-patch",
      label: "Save staged changes as patch…",
      keywords: ["save", "patch", "staged", "diff", "export"],
      run: () => void appState.savePatch({ kind: "Staged" }, "staged.patch"),
    });
  }
  return commands;
}

/** The escape hatches of an operation in progress, and Refresh: never behind the guard. */
function operationCommands({ appState, state }: FamilyContext): Command[] {
  const { commands, add } = family("General");
  if (state.rebaseProgress !== null) {
    // An interrupted rebase (changed outside Browsitory) can only be aborted.
    if (state.headInfo.rebase?.health.kind !== "Diverged") {
      add({
        id: "rebase-continue",
        label: "Continue rebase",
        keywords: ["rebase", "continue"],
        run: () => void appState.rebaseContinue(),
      });
    }
    add({
      id: "rebase-abort",
      label: "Abort rebase",
      keywords: ["rebase", "abort", "cancel"],
      run: () => void appState.abortRebase(),
    });
  }
  const abortSequence = (noun: string) =>
    add({
      id: "abort-sequence",
      label: `Abort ${noun}`,
      keywords: ["abort", "cancel", "cherry", "pick", "revert", "sequence"],
      run: () => void appState.abortSequence(),
    });
  const sequence = state.headInfo.sequence;
  if (sequence !== null) {
    const noun = sequence.kind === "CherryPick" ? "cherry-pick" : "revert";
    add({
      id: "continue-sequence",
      label: `Continue ${noun}`,
      keywords: ["continue", "cherry", "pick", "revert", "sequence"],
      run: () => void appState.continueSequence(),
    });
    abortSequence(noun);
  } else if (state.headInfo.terminalSequence !== null && state.headInfo.operation !== "Merge") {
    // A cherry-pick started in a terminal (#258): Abort ends it like `git cherry-pick --abort`
    // (#253); it is continued in the terminal.
    abortSequence(state.headInfo.terminalSequence.kind === "CherryPick" ? "cherry-pick" : "revert");
  } else if (state.mergeMessage !== null && state.headInfo.terminalRebase === null) {
    // A terminal rebase's conflicted step writes MERGE_MSG too; it is aborted in the terminal
    // (#263).
    add({
      id: "merge-abort",
      label: "Abort merge",
      keywords: ["merge", "abort", "cancel"],
      run: () => void appState.abortMerge(),
    });
  }
  add({ id: "refresh", label: "Refresh", keywords: ["refresh", "reload"], run: () => void appState.refresh({ full: true }) });
  return commands;
}

/** The selected commit's menu, HEAD's tag and amend, and the next commit's hooks. */
function historyCommands({ appState, state, blockers, blocked, commit }: FamilyContext): Command[] {
  const { commands, add, offer } = family("General");
  const selected = state.selectedRow === "uncommitted" ? null : state.selectedRow.commitId;
  const graphCommit = selected === null ? undefined : state.commits.find((c) => c.id === selected);
  // A search result or file history commit outside the loaded graph (#515) gets the commit menu's
  // actions too; the history edits below need its ancestry in the graph, so they stay graph-only.
  const selectedCommit =
    graphCommit ?? (selected === null ? undefined : commit?.listedCommits?.find((c) => c.id === selected));
  const currentBranch = state.branches.find((branch) => branch.isCurrent)?.name ?? null;
  if (selectedCommit !== undefined) {
    // The commit menu's actions, with the same availability (`commitActions`, #495): the palette
    // leaves out what the menu disables.
    const actions = commitActions({ blockers, headId: state.headInfo.headId, currentBranch }, selectedCommit);
    const commitId = selectedCommit.id;
    // The menu's own actions (#507); a merge commit's pick and revert ask for the parent first.
    if (commit !== undefined) {
      offer("cherry-pick-selected", actions.cherryPick, ["cherry", "pick", "apply", "commit"], () =>
        actions.cherryPick.needsParent ? commit.onChooseParent("Cherry-pick", commitId) : commit.onCherryPick(commitId),
      );
      offer("revert-selected", actions.revert, ["revert", "undo", "commit"], () =>
        actions.revert.needsParent ? commit.onChooseParent("Revert", commitId) : commit.onRevert(commitId),
      );
    }
    offer("checkout-selected", actions.checkout, ["checkout", "detach", "commit"], () =>
      void appState.checkoutCommit(commitId),
    );
    // The keyboard path for the commit menu's "Create tag…" (#438). Only opens the dialog, whose
    // Create button carries the busy rule, so it is offered while an operation runs. Like every
    // local tag command, it is listed under "Branches" (#583).
    offer(
      "create-tag-selected",
      actions.createTag,
      ["tag", "create", "new", "release", "commit"],
      () => appState.openDialog({ kind: "createTag", commitId }),
      "Branches",
    );
    // Reset asks for the mode in its dialog.
    if (commit !== undefined) {
      offer("reset-selected", actions.reset, ["reset", "soft", "mixed", "hard"], () => commit.onReset(commitId));
    }
  }
  const headId = state.headInfo.headId;
  if (headId !== null) {
    add(
      {
        id: "create-tag-head",
        label: "Create tag at HEAD…",
        keywords: ["tag", "create", "new", "release", "head"],
        run: () => appState.openDialog({ kind: "createTag", commitId: headId }),
      },
      "Branches",
    );
  }

  if (!blocked && graphCommit !== undefined) {
    const isHead = headId === graphCommit.id;
    // The commit menu's one-click history edits (#442), only where the menu enables them.
    // HEAD's Reword… is "Amend last commit" below.
    const commitId = graphCommit.id;
    const short = graphCommit.shortId;
    const checks = historyEditChecks(
      { commits: state.commits, headInfo: state.headInfo, currentBranch, status: state.status },
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
      add({ id, label, keywords: [...keywords, "edit", "rebase", "commit"], run: () => run(onto) });
    }
  }
  if (!blocked && headId !== null) {
    add({
      id: "amend-last-commit",
      label: "Amend last commit",
      keywords: ["amend", "edit", "message", "commit"],
      run: () => {
        appState.selectRow("uncommitted");
        appState.setAmendMode(true);
      },
    });
  }

  // Changes no repository state (only what the next commit passes), so it is offered even while
  // other operations are blocked.
  if (state.headInfo.commitHooks.length > 0) {
    add({
      id: "toggle-skip-hooks",
      label: state.skipHooks ? "Run hooks for the next commit" : "Skip hooks for the next commit",
      keywords: ["hooks", "skip", "no-verify", "bypass", "pre-commit", "commit-msg"],
      run: () => {
        appState.selectRow("uncommitted");
        appState.setSkipHooks(!state.skipHooks);
      },
    });
  }
  return commands;
}

/**
 * "Switch branch…" (#638): the recent branches first, then the other local ones, then the remote
 * branches without a local one (#726), and "Create branch <name> from <current>" for a new name. Always listed, so the toolbar's branch chip opens
 * it; while the repository is busy it lists nothing and says why.
 */
function switchBranchPicker({ appState, state, blockers }: FamilyContext, branchSwitch: BranchSwitchCommands): Command {
  const current = state.branches.find((branch) => branch.isCurrent)?.name ?? null;
  const others = state.branches.filter((branch) => !branch.isCurrent).map((branch) => branch.name);
  const recent = branchSwitch.recentBranches.filter((name) => others.includes(name));
  const ordered = [...recent, ...others.filter((name) => !recent.includes(name))];
  const local: PickerItem[] = ordered.map((name) => ({
    id: `switch-to-branch:${name}`,
    label: name,
    keywords: [name],
    run: () => branchSwitch.onSwitchBranch(name),
  }));
  // A remote branch without a local one (#726): checked out as a new tracking branch, as the
  // Branches tree's remote Checkout does. `origin/HEAD` names no branch of its own.
  const localNames = new Set(state.branches.map((branch) => branch.name));
  const remote: PickerItem[] = branchSwitch.remoteBranches
    .filter(({ branchName }) => branchName !== "HEAD" && !localNames.has(branchName))
    .map(({ remoteName, branchName }) => ({
      id: `checkout-remote-branch:${remoteName}/${branchName}`,
      label: `${remoteName}/${branchName}`,
      keywords: [branchName, remoteName, "remote", "track"],
      run: () => branchSwitch.onCheckoutRemoteBranch(remoteName, branchName),
    }));
  const items = [...local, ...remote];
  return {
    id: SWITCH_BRANCH_PICKER_ID,
    group: "Branches",
    label: "Switch branch…",
    keywords: ["branch", "switch", "checkout", "change", "create", "new", "recent"],
    run: () => {},
    picker: {
      placeholder: "Branch name, or a new one to create…",
      emptyText: blockers.blocked ? (blockers.reason ?? "Another operation is in progress.") : "No other branches",
      items: (query) => {
        if (blockers.blocked) return [];
        const matches = filterPickerItems(items, query);
        const name = query.trim();
        // The New branch form, for another start point or to stay on this branch (#656).
        if (name === "")
          return [
            ...matches,
            {
              id: "create-branch-form",
              label: "Create branch…",
              keywords: ["create", "new"],
              run: () => appState.openCreateBranchDraft("HEAD"),
            },
          ];
        const taken = state.branches.some((branch) => branch.name === name);
        if (taken || branchNameProblem(name) !== null) return matches;
        return [
          ...matches,
          {
            id: `create-branch:${name}`,
            label: `Create branch ${name} from ${current ?? state.headInfo.detachedAt ?? "HEAD"}`,
            keywords: [name],
            run: () => branchSwitch.onCreateAndSwitch(name),
          },
        ];
      },
    },
  };
}

/** The branch menu's Checkout and Merge (`branchActions`, #509), and checking out a tag. */
function branchCommands(ctx: FamilyContext): Command[] {
  const { appState, state, blockers, blocked } = ctx;
  const { commands, add, offer } = family("Branches");
  if (ctx.branchSwitch !== undefined) commands.push(switchBranchPicker(ctx, ctx.branchSwitch));
  // Opens the sidebar's New branch form (a drawer when narrow, #656); its Create carries the
  // busy rule.
  add({
    id: "create-branch",
    label: "Create branch…",
    keywords: ["branch", "create", "new"],
    run: () => appState.openCreateBranchDraft("HEAD"),
  });
  const branchContext = { blockers, remoteCount: state.remotes.length, upstream: state.upstream };
  const currentBranch = state.branches.find((branch) => branch.isCurrent)?.name ?? null;
  // Every branch but the current one.
  for (const branch of state.branches) {
    const actions = branchActions(branchContext, branch);
    offer(`switch-branch:${branch.name}`, actions.checkout, ["branch", "switch", "checkout", branch.name], () =>
      void appState.switchBranch(branch.name),
    );
    offer(`merge-branch:${branch.name}`, actions.merge, ["branch", "merge", "squash", branch.name], () =>
      appState.openDialog({ kind: "merge", branchName: branch.name }),
    );
    // #619; `null` on a detached HEAD.
    const rebase = rebaseOntoAction(blockers, currentBranch, { kind: "local", name: branch.name });
    if (rebase !== null) {
      offer(`rebase-onto-branch:${branch.name}`, rebase, ["branch", "rebase", "onto", branch.name], () =>
        appState.openRebasePlanner(rebase.onto),
      );
    }
  }
  // The keyboard path for the tag badge menu's "Switch to (detached)".
  if (!blocked) {
    for (const tag of state.tags) {
      const commitId = tag.commitId;
      if (commitId === null) continue; // a tag of a tree or blob has nothing to check out
      add({
        id: `checkout-tag:${tag.name}`,
        label: `Switch to tag ${tag.name} (detached)`,
        keywords: ["tag", "checkout", "detached", tag.name],
        run: () => void appState.checkoutCommit(commitId),
      });
    }
  }
  return commands;
}

/** The remote folder menus (`remoteActions`, #509), pushing and fetching. */
function remoteCommands({ appState, state, blockers, blocked, sidebar }: FamilyContext): Command[] {
  const { commands, add, offer } = family("Remotes");
  // The keyboard path for a tag badge's "Delete from remote…" (#439); the dialog confirms.
  if (!blocked && state.remotes.length > 0) {
    for (const tag of state.tags) {
      add({
        id: `delete-remote-tag:${tag.name}`,
        label: `Delete tag ${tag.name} from remote…`,
        keywords: ["tag", "delete", "remove", "remote", tag.name],
        run: () => appState.openDialog({ kind: "deleteRemoteTag", tagName: tag.name }),
      });
    }
  }
  if (!blocked && state.upstream !== null && state.headInfo.detachedAt === null) {
    const remoteName = state.upstream.remoteName;
    add({
      id: "force-push",
      label: "Force push current branch…",
      keywords: ["force", "push", "lease"],
      run: () => appState.openDialog({ kind: "forcePush", remoteName }),
    });
  }

  if (state.remotes.length > 0) {
    if (!blocked) {
      add({
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
        branchActions({ blockers, remoteCount: state.remotes.length, upstream: state.upstream }, current).pushTo,
        ["push", "publish", "upstream", "remote", "branch"],
        () => appState.openDialog({ kind: "push", localBranch: current.name }),
      );
    }
    if (!blocked) {
      add({
        id: "delete-remote-branch",
        label: "Delete remote branch…",
        keywords: ["delete", "remove", "remote", "branch"],
        run: () => appState.openDialog({ kind: "deleteRemoteBranch", remoteName: null, branchName: null }),
      });
    }
  }

  for (const remote of state.remotes) {
    // App state owns their dialogs, so they open even while the Branches section is collapsed.
    const actions = remoteActions(blockers, remote.name);
    offer(`fetch-remote:${remote.name}`, actions.fetch, ["fetch", "remote", remote.name], () =>
      void appState.fetchRemote(remote.name),
    );
    offer(`push-branch:${remote.name}`, actions.pushCurrentBranch, ["push", "remote", remote.name], () =>
      void appState.pushCurrentBranch(remote.name),
    );
    if (!blocked) {
      add({
        id: `push-tags:${remote.name}`,
        label: `Push all tags to ${remote.name}`,
        keywords: ["push", "tags", "remote", remote.name],
        run: () => void appState.pushTags(remote.name, []),
      });
    }
    // Checkout/Set-as-upstream for a specific remote-tracking branch (BranchTree's
    // `remoteBranchItems`) have no keyboard path otherwise (AUD-2026-09-05-FE-001): their rows
    // only render once the remote's folder is expanded. This expands it, so its branch rows (each
    // with a "…" affordance) are reachable by Tab right after.
    if (!blocked && sidebar !== undefined) {
      add({
        id: `browse-remote-branches:${remote.name}`,
        label: `Browse branches on ${remote.name}`,
        keywords: ["remote", "branch", "checkout", "upstream", "browse", remote.name],
        run: () => sidebar.revealRemote(remote.name),
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
  return commands;
}

// The families below that lead into a sidebar panel are also left out while that panel is hidden
// (hidden panels are unmounted, not just CSS-hidden): otherwise they would reveal a section that
// doesn't render, or silently change state the user can't see. "Branches" is never hidden.

/**
 * Deleting a tag has a real confirmation dialog in TagPanel, which the palette must not skip:
 * the command stays discoverable by tag name but only reveals the Tags section.
 *
 * The palette has no "Tags" group: local tag commands (create, check out, delete) are listed under
 * "Branches" with the other local refs, a remote tag's delete under "Remotes" (#583).
 */
function tagCommands({ state, blocked, sidebar, panelVisible }: FamilyContext): Command[] {
  const { commands, add } = family("Branches");
  if (blocked || sidebar === undefined || !panelVisible("tags")) return commands;
  for (const tag of state.tags) {
    add({
      id: `delete-tag:${tag.name}`,
      label: `Delete tag ${tag.name}`,
      keywords: ["tag", "delete", tag.name],
      run: () => sidebar.revealSection("tags"),
    });
  }
  return commands;
}

function stashCommands({ appState, state, diff, sidebar, panelVisible }: FamilyContext): Command[] {
  const { commands, add, offer } = family("Stash");
  // Stashing has its own, narrower rule (`stashBlockReason`): the same one DiffPane's Stash
  // button and the file menu's "Stash file…" follow. It allows stashing during a merge, so these
  // commands are not behind the full repository guard.
  if (stashBlockReason(state) === null) {
    // The id stays "save-stash" so recents recorded under the old label keep working.
    add({ id: "save-stash", label: "Stash all changes", keywords: ["stash", "save", "all"], run: () => void appState.saveStash() });
    add({
      id: "stash-with-options",
      label: "Stash…",
      keywords: ["stash", "save", "message", "untracked", "keep", "index", "options"],
      run: () => appState.openDialog({ kind: "stash", paths: [] }),
    });
    const stashFile = diff?.currentFile ?? null;
    if (stashFile !== null) {
      add({
        id: "stash-current-file",
        label: `Stash ${stashFile}…`,
        keywords: ["stash", "file", "save", stashFile],
        run: () => appState.openDialog({ kind: "stash", paths: [stashFile] }),
      });
    }
    const fileSelection = diff?.selection ?? null;
    if (fileSelection !== null) {
      add({
        id: "stash-selected-files",
        label: `Stash ${selectedFiles(fileSelection.paths.length)}…`,
        keywords: ["stash", "selected", "files", "save"],
        run: () => appState.openDialog({ kind: "stash", paths: fileSelection.paths }),
      });
    }
  }

  if (!panelVisible("stash")) return commands;
  // The Stashes panel's Apply, Pop and Drop… (`stashActions`, #509): Drop stays available while
  // resolving a paused step (#521). Showing a stash only selects it, so it is always offered.
  const storedStashBlockers = stashBlockers(state);
  for (const stash of state.stashes) {
    const actions = stashActions(storedStashBlockers, stash);
    add({
      id: `show-stash:${stash.index}`,
      label: `Show stash: ${stashTitle(stash)}`,
      keywords: ["stash", "show", "view", "diff"],
      run: () => appState.selectRow({ commitId: stash.commitId }),
    });
    offer(`apply-stash:${stash.index}`, actions.apply, ["stash", "apply"], () => void appState.applyStash(stash.index, true));
    if (actions.applyUnstaged !== null) {
      offer(`apply-stash-unstaged:${stash.index}`, actions.applyUnstaged, ["stash", "apply", "unstaged"], () =>
        void appState.applyStash(stash.index, false),
      );
    }
    offer(`pop-stash:${stash.index}`, actions.pop, ["stash", "pop", "apply"], () => void appState.popStash(stash.index, true));
    if (actions.popUnstaged !== null) {
      offer(`pop-stash-unstaged:${stash.index}`, actions.popUnstaged, ["stash", "pop", "apply", "unstaged"], () =>
        void appState.popStash(stash.index, false),
      );
    }
    // Dropping has a confirmation in the Stashes panel and the stash view; like "Delete tag", the
    // palette only shows the stash and reveals the section, never drops directly.
    if (sidebar !== undefined) {
      offer(`drop-stash:${stash.index}`, actions.drop, ["stash", "drop", "delete"], () => {
        appState.selectRow({ commitId: stash.commitId });
        sidebar.revealSection("stash");
      });
    }
  }
  return commands;
}

function worktreeCommands({ appState, state, blockers, tabs, sidebar, panelVisible }: FamilyContext): Command[] {
  const { commands, offer } = family("General");
  const prune = pruneWorktreesAction(blockers);
  offer("prune-worktrees", { paletteLabel: prune.label, enabled: prune.enabled, reason: prune.reason }, ["worktree", "prune"], () =>
    void appState.pruneWorktrees(),
  );
  if (!panelVisible("worktree")) return commands;
  for (const worktree of state.worktrees) {
    // The main worktree's remove is refused (`MAIN_WORKTREE`), so only its open is offered (#561).
    const actions = worktreeActions(blockers, worktree);
    if (tabs !== undefined) {
      offer(`open-worktree:${worktree.path}`, actions.open, ["worktree", "open", worktree.name], () =>
        tabs.onOpenRepoTab(worktree.path),
      );
    }
    // Removing a worktree has a real confirmation dialog in WorktreePanel: reveal it instead of
    // removing directly from the palette.
    if (sidebar !== undefined) {
      offer(`remove-worktree:${worktree.name}`, actions.remove, ["worktree", "remove", "delete", worktree.name], () =>
        sidebar.revealSection("worktree"),
      );
    }
  }
  return commands;
}

function submoduleCommands({ appState, state, blocked, tabs, panelVisible }: FamilyContext): Command[] {
  const { commands, add } = family("General");
  if (blocked || !panelVisible("submodule")) return commands;
  for (const submodule of state.submodules) {
    if (submodule.initialized) {
      add({
        id: `update-submodule:${submodule.path}`,
        label: `Update submodule ${submodule.path}`,
        keywords: ["submodule", "update", submodule.path],
        run: () => void appState.updateSubmodule(submodule.path, false),
      });
    } else {
      add({
        id: `init-submodule:${submodule.path}`,
        label: `Initialize submodule ${submodule.path}`,
        keywords: ["submodule", "init", "initialize", submodule.path],
        run: () => void appState.initSubmodule(submodule.path),
      });
    }
    // Like "Open worktree": a tab of its own (#445). Only a checked-out submodule has a
    // folder to open; for the others the commands above offer Initialize / Update.
    const workdir = submodule.workdir;
    if (workdir !== null && tabs !== undefined) {
      add({
        id: `open-submodule:${submodule.path}`,
        label: `Open submodule ${submodule.path} in new tab`,
        keywords: ["submodule", "open", "tab", submodule.path],
        run: () => tabs.onOpenRepoTab(workdir),
      });
    }
  }
  // Restoring a reflog entry has a real confirmation dialog in ReflogPanel, and state.reflog is
  // unbounded (a full, uncapped reflog read for the selected reference), so there is no
  // per-entry command: "Go to Reflog" leads there.
  return commands;
}

/** The diff settings, the current diff's toggles, and the current file and selection. */
function diffCommands({ appState, state, diff, tools }: FamilyContext): Command[] {
  const { commands, add } = family("Diff");
  // Read-only external tools, so offered while an operation runs.
  if (tools !== undefined && diff?.currentFile != null) {
    add({
      id: "open-current-file-in-editor",
      label: "Open current file in editor",
      keywords: ["editor", "open", "edit", "file"],
      run: tools.onOpenCurrentFileInEditor,
    });
    add({
      id: "open-current-file-in-diff-tool",
      label: "Open current file in diff tool",
      keywords: ["diff", "tool", "difftool", "compare"],
      run: tools.onOpenCurrentFileInDiffTool,
    });
  }
  if (diff === undefined) return commands;
  const { settings, update } = diff;
  add({
    id: "toggle-ignore-whitespace",
    label: "Toggle ignore whitespace",
    keywords: ["diff", "whitespace", "ignore"],
    run: () => update({ ignoreWhitespace: !settings.ignoreWhitespace }),
  });
  add({
    id: "toggle-show-whitespace",
    label: "Toggle whitespace",
    keywords: ["diff", "whitespace", "show", "spaces", "tabs", "markers", "trailing"],
    run: () => update({ showWhitespace: !settings.showWhitespace }),
  });
  add({
    id: "increase-context-lines",
    label: "Increase context lines",
    keywords: ["diff", "context", "lines", "more"],
    run: () => update({ contextLines: Math.min(settings.contextLines + 1, MAX_CONTEXT_LINES) }),
  });
  add({
    id: "decrease-context-lines",
    label: "Decrease context lines",
    keywords: ["diff", "context", "lines", "fewer", "less"],
    run: () => update({ contextLines: Math.max(settings.contextLines - 1, 0) }),
  });
  add({
    id: "toggle-syntax-highlighting",
    label: "Toggle syntax highlighting",
    keywords: ["diff", "syntax", "highlight", "colors"],
    run: () => update({ syntaxHighlight: !settings.syntaxHighlight }),
  });
  const currentDiff = diff.currentDiff ?? null;
  if (currentDiff !== null) {
    const of = currentDiff.path === null ? "" : ` (${currentDiff.path})`;
    const fileKeywords = currentDiff.path === null ? [] : [currentDiff.path];
    add({
      id: "toggle-wrap-lines",
      label: `Toggle wrap lines${of}`,
      keywords: ["diff", "wrap", "lines", "word", "long", ...fileKeywords],
      run: currentDiff.toggleWrap,
    });
    if (currentDiff.canShowWholeFile) {
      add({
        id: "toggle-show-whole-file",
        label: `Toggle show whole file${of}`,
        keywords: ["diff", "whole", "file", "full", "context", ...fileKeywords],
        run: currentDiff.toggleWholeFile,
      });
    }
  }
  const mergeEditor = diff.mergeEditor ?? null;
  if (mergeEditor !== null) {
    const keywords = ["conflict", "merge", "resolve", mergeEditor.path];
    const move = (id: MergeEditorShortcutId, run: () => void) =>
      add({
        id,
        label: `${mergeEditorShortcut(id).description} (${mergeEditor.path})`,
        keywords: [...keywords, "go", "jump"],
        run,
        keys: mergeEditorShortcutLabel(id),
      });
    if (mergeEditor.canGoPrevious) move("previous-conflict", mergeEditor.previous);
    if (mergeEditor.canGoNext) move("next-conflict", mergeEditor.next);
    for (const [id, choice] of MERGE_EDITOR_CHOICES) {
      add({
        id,
        label: `${mergeEditorShortcut(id).description} (current conflict)`,
        keywords: [...keywords, "accept", "take", "choose", "side"],
        run: () => mergeEditor.choose(choice),
        keys: mergeEditorShortcutLabel(id),
      });
    }
  }
  if (diff.currentFile !== null) {
    add({
      id: "file-history-current-file",
      label: "File history of current file",
      keywords: ["history", "log", "file", "follow", "rename"],
      run: diff.openCurrentFileHistory,
    });
  }
  // A selection of several files (#449): the same actions as its selection bar. Staging is not
  // behind the full repository guard (it is how a merge's resolutions get staged), only behind
  // another action in flight, as the selection bar is.
  const selection = diff.selection ?? null;
  if (selection !== null && !state.pending) {
    const paths = selection.paths;
    add(
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
  // The Changes heading's "Stage all" (#646): every change but conflicts, untracked files
  // included, in one undo step.
  const allPaths = stageAllPaths(state.status);
  if (allPaths.length > 0 && !state.pending) {
    add({
      id: "stage-all-files",
      label: "Stage all changes",
      keywords: ["stage", "add", "all", "changes", "untracked"],
      run: () => void appState.stageAllFiles(allPaths),
    });
  }
  // The Changes heading's "Stage modified" (#621): tracked files' changes only, no untracked
  // files and no conflicts; one undo step, like "Stage all".
  const modifiedPaths = stageModifiedPaths(state.status);
  if (modifiedPaths.length > 0 && !state.pending) {
    add({
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
    if (selection !== null && !selection.staged && diff.requestDiscardSelection !== undefined) {
      add({
        id: "discard-selected-files",
        label: `Discard ${selectedFiles(selection.paths.length)}…`,
        keywords: ["discard", "revert", "selected", "files"],
        run: diff.requestDiscardSelection,
      });
    }
    if (diff.currentFile !== null) {
      add({
        id: "discard-current-file",
        label: "Discard changes in current file",
        keywords: ["discard", "revert", "file"],
        run: diff.requestDiscardCurrentFile,
      });
    }
    if (state.status.some((entry) => !entry.staged && entry.kind !== "Conflicted")) {
      add({
        id: "discard-all-unstaged",
        label: "Discard all unstaged changes",
        keywords: ["discard", "revert", "all", "unstaged"],
        run: diff.requestDiscardAllUnstaged,
      });
    }
    if (untrackedPaths(state.status).length > 0 && diff.requestDeleteUntracked !== undefined) {
      add({
        id: "delete-untracked-files",
        label: "Delete untracked files…",
        keywords: ["delete", "remove", "clean", "untracked", "new", "files"],
        run: diff.requestDeleteUntracked,
      });
    }
  }
  return commands;
}

/** The "Go to …" pickers (#441) and the sidebar sections. Read-only, so never behind the guard. */
function goToCommands({ appState, goTo, sidebar, panelVisible }: FamilyContext): Command[] {
  const commands = goTo === undefined ? [] : goToPickers(appState, goTo);
  if (sidebar === undefined) return commands;
  const { commands: sections, add } = family("Go to");
  const toggle = sidebar.toggle;
  if (toggle !== undefined) {
    add({
      id: "toggle-sidebar",
      label: toggle.shown ? "Hide sidebar" : "Show sidebar",
      keywords: ["sidebar", "toggle", "panel", "drawer", "branches", "show", "hide"],
      run: toggle.run,
    });
  }
  for (const { section, title, id } of SIDEBAR_SECTIONS) {
    if (section !== "branches" && !panelVisible(section)) continue;
    add({
      id,
      label: `Go to ${title}`,
      keywords: ["go", "navigate", title.toLowerCase()],
      run: () => sidebar.revealSection(section),
    });
  }
  return [...commands, ...sections];
}

/** The repository's tab and folder: external tools, closing and switching tabs. */
function repositoryCommands({ app, tools, tabs, blocked }: FamilyContext): Command[] {
  const { commands, add } = family("Repositories");
  if (tools !== undefined) {
    add({
      id: "open-terminal",
      label: "Open terminal here",
      keywords: ["terminal", "shell", "console", "cmd"],
      run: tools.onOpenTerminal,
    });
    add({
      id: "reveal-repository",
      label: "Reveal repository",
      keywords: ["reveal", "finder", "explorer", "file manager", "folder"],
      run: tools.onRevealRepository,
    });
    for (const item of tools.rootFiles ?? []) {
      // One id for both labels, so the recent list keeps it once the file exists.
      add({
        id: `open-root-file:${item.name}`,
        label: item.label,
        keywords: ["open", "create", "edit", "editor", "file", item.name, item.name.slice(1)],
        run: () => tools.onOpenRootFile?.(item),
      });
    }
  }
  // Omitted (not disabled) while an operation is in flight, the same rule the tab's own close
  // button and Ctrl/Cmd+W follow — closing would orphan the operation mid-way.
  if (tabs !== undefined && !blocked) {
    add({
      id: "close-tab",
      label: "Close tab",
      keywords: ["close", "tab", "repo", "repository"],
      run: tabs.onCloseActiveRepoTab,
    });
  }
  if (app?.onReopenClosedTab !== undefined) {
    add({
      id: "reopen-closed-tab",
      label: "Reopen closed tab",
      keywords: ["reopen", "restore", "closed", "tab", "repo", "repository", "undo close"],
      run: app.onReopenClosedTab,
    });
  }
  if (app?.onRenameTab !== undefined) {
    add({
      id: "rename-tab",
      label: "Rename tab…",
      keywords: ["rename", "name", "alias", "label", "tab", "repo", "repository"],
      run: app.onRenameTab,
    });
  }
  if (tabs === undefined) return commands;
  const tabItems = tabs.otherOpenRepos.map((repo) => ({
    id: `switch-repo:${repo.path}`,
    label: `Switch to ${repo.displayName}`,
    keywords: ["repo", "switch", "tab", ...tabKeywords(repo)],
    run: () => tabs.onSwitchRepoTab(repo.path),
  }));
  tabItems.forEach((item) => add(item));
  // The same tabs as one picker (#625): their names, folder names and paths are searchable.
  if (tabItems.length > 0) {
    const pickerItems = tabs.otherOpenRepos.map((repo, index) => ({
      ...tabItems[index],
      label: repo.displayName,
      keywords: tabKeywords(repo),
    }));
    add({
      id: "switch-to-tab",
      label: "Switch to tab…",
      keywords: ["switch", "tab", "tabs", "repo", "repository", "open", "list", "go to"],
      run: () => {},
      picker: {
        placeholder: "Tab name or path…",
        emptyText: "No other open tab matches",
        items: (query) => filterPickerItems(pickerItems, query),
      },
    });
  }
  return commands;
}

const PICKER_KEYWORDS = ["go", "go to", "jump", "find", "show", "reveal", "select", "navigate"];

/** What finds a tab in the palette: its name, its folder name and its path (#625). */
function tabKeywords(repo: OpenRepo): string[] {
  return [repo.displayName, repoNameOf(repo.path), repo.path];
}

function goToPickers(appState: UseAppStateResult, goTo: GoToCommands): Command[] {
  const { state } = appState;
  const { commands: pickers, add } = family("Go to");
  const noop = () => {};

  if (goTo.onGoBack !== undefined) {
    add({
      id: "go-back",
      label: "Back to previous commit",
      keywords: ["back", "previous", "history", "navigate", "go back"],
      run: goTo.onGoBack,
    });
  }
  if (goTo.onGoForward !== undefined) {
    add({
      id: "go-forward",
      label: "Forward to next commit",
      keywords: ["forward", "next", "history", "navigate", "go forward"],
      run: goTo.onGoForward,
    });
  }
  add({
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
  const branchItems: PickerItem[] = [
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
    add({
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
    add({
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
    add({
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
export function commitPickerItems(commits: GraphCommit[], query: string, goTo: GoToCommands): PickerItem[] {
  const q = query.trim();
  const needle = q.toLowerCase();
  const matches =
    needle === ""
      ? commits
      : commits.filter((commit) => commit.id.startsWith(needle) || commit.summary.toLowerCase().includes(needle));
  const items: PickerItem[] = matches.slice(0, RESULT_LIMIT).map((commit) => ({
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

/** Picker items matching `query` (best match first, else in the given order; `matchItem`). */
export function filterPickerItems(items: PickerItem[], query: string): PickerItem[] {
  const scored: { item: PickerItem; score: number; index: number }[] = [];
  items.forEach((item, index) => {
    const match = matchItem(item, query);
    if (match !== null) scored.push({ item, score: match.score, index });
  });
  scored.sort((a, b) => b.score - a.score || a.index - b.index);
  return scored.slice(0, RESULT_LIMIT).map((entry) => entry.item);
}

/**
 * Commands matching `query`, best first; ties go to the recently used, then by label. Fuzzy and
 * in any word order (`matchItem`, #644).
 */
export function filterAndSortCommands<T extends PickerItem>(commands: T[], query: string): T[] {
  const recent = loadRecentCommandIds();
  const scored: { command: T; score: number }[] = [];
  for (const command of commands) {
    const match = matchItem(command, query);
    if (match !== null) scored.push({ command, score: match.score });
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
  // The empty query lists every command, grouped by the palette (#644); a filtered list is capped.
  const kept = query.trim() === "" ? scored : scored.slice(0, RESULT_LIMIT);
  return kept.map((entry) => entry.command);
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

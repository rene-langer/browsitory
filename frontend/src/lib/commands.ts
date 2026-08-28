import type { UseAppStateResult } from "../state/useAppState";
import type { OpenRepo } from "../state/useOpenRepos";
import { SIDEBAR_PANEL_IDS, type SidebarPanelId } from "../state/useSidebarPanelVisibility";

export interface Command {
  id: string;
  label: string;
  keywords: string[];
  run: () => void;
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
  "Pull Requests",
] as const;

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
    case "Pull Requests":
      return "pullRequests";
    case "Branches":
      return null;
  }
}

function goToSidebarSection(title: string): void {
  // Scoped to the active tab's workspace, not the whole document: every open repo's
  // `RepoWorkspace` stays mounted simultaneously (inactive ones are only `display: none`), so a
  // document-wide lookup always resolves to whichever tab is first in document order. On any
  // other tab that silently expanded a hidden tab's accordion and scrolled an invisible element
  // into view — a no-op from the user's side. `App.tsx` marks the visible workspace with
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
  button.closest("section")?.scrollIntoView({ behavior: "smooth", block: "nearest" });
}

export function buildCommands(
  appState: UseAppStateResult,
  onOpenRepoTab: (path: string) => void = () => {},
  otherOpenRepos: OpenRepo[] = [],
  onSwitchRepoTab: (path: string) => void = () => {},
  panelVisibility: Record<SidebarPanelId, boolean> = defaultPanelVisibility(),
): Command[] {
  const { state } = appState;
  const commands: Command[] = [];

  // Same guard App.tsx derives as `repositoryOperationDisabled` and gates every
  // sidebar mutation button behind. Mutating command families must be omitted
  // (not merely disabled) while it's true; navigation and the in-progress
  // escape hatches (merge-abort/rebase-continue/rebase-abort/refresh) stay.
  const repositoryOperationDisabled =
    state.pending ||
    state.transfer !== null ||
    state.mergeMessage !== null ||
    state.rebaseProgress !== null;

  if (!repositoryOperationDisabled && state.upstream !== null) {
    commands.push({
      id: "pull",
      label: "Pull",
      keywords: ["pull", "sync"],
      run: () => void appState.pullCurrentUpstream(),
    });
  }
  // DiffPane's real "Save stash" button is only disabled by
  // `status.length === 0 || rebaseProgress !== null` — it does not carry the
  // full repositoryOperationDisabled guard, so this command stays unconditional.
  commands.push({
    id: "save-stash",
    label: "Save stash",
    keywords: ["stash", "save"],
    run: () => void appState.saveStash(),
  });
  if (!repositoryOperationDisabled) {
    commands.push({
      id: "prune-worktrees",
      label: "Prune worktrees",
      keywords: ["worktree", "prune"],
      run: () => void appState.pruneWorktrees(),
    });
  }
  if (state.rebaseProgress !== null) {
    commands.push({
      id: "rebase-continue",
      label: "Continue rebase",
      keywords: ["rebase", "continue"],
      run: () => void appState.rebaseContinue(),
    });
    commands.push({
      id: "rebase-abort",
      label: "Abort rebase",
      keywords: ["rebase", "abort", "cancel"],
      run: () => void appState.abortRebase(),
    });
  }
  if (state.mergeMessage !== null) {
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

  if (!repositoryOperationDisabled) {
    for (const branch of state.branches) {
      if (branch.isCurrent) continue;
      commands.push({
        id: `switch-branch:${branch.name}`,
        label: `Switch to ${branch.name}`,
        keywords: ["branch", "switch", "checkout", branch.name],
        run: () => void appState.switchBranch(branch.name),
      });
    }

    for (const remote of state.remotes) {
      commands.push({
        id: `fetch-remote:${remote.name}`,
        label: `Fetch ${remote.name}`,
        keywords: ["fetch", "remote", remote.name],
        run: () => void appState.fetchRemote(remote.name),
      });
      commands.push({
        id: `push-branch:${remote.name}`,
        label: `Push to ${remote.name}`,
        keywords: ["push", "remote", remote.name],
        run: () => void appState.pushCurrentBranch(remote.name),
      });
      commands.push({
        id: `push-tags:${remote.name}`,
        label: `Push all tags to ${remote.name}`,
        keywords: ["push", "tags", "remote", remote.name],
        run: () => void appState.pushTags(remote.name, []),
      });
    }

    commands.push({
      id: "add-remote",
      label: "Add remote",
      keywords: ["remote", "add"],
      run: () => appState.openAddRemoteDraft(),
    });

    // Every loop below is also skipped while its sidebar panel is hidden (the new sidebar
    // panel-visibility toggles fully unmount hidden panels, not just CSS-hide them) — otherwise
    // these commands either navigate to a section that no longer renders (Tags/Worktrees'
    // `goToSidebarSection` calls) or silently mutate state the user can't see the result of
    // (stash/submodule). "Branches" has no toggle, so it's never filtered.

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

    if (panelVisibility.stash) {
      for (const stash of state.stashes) {
        commands.push({
          id: `apply-stash:${stash.index}`,
          label: `Apply stash: ${stash.message}`,
          keywords: ["stash", "apply"],
          run: () => void appState.applyStash(stash.index),
        });
        commands.push({
          id: `drop-stash:${stash.index}`,
          label: `Drop stash: ${stash.message}`,
          keywords: ["stash", "drop", "delete"],
          run: () => void appState.dropStash(stash.index),
        });
      }
    }

    if (panelVisibility.worktree) {
      for (const worktree of state.worktrees) {
        if (worktree.isMain) continue;
        commands.push({
          id: `open-worktree:${worktree.path}`,
          label: `Open worktree ${worktree.name}`,
          keywords: ["worktree", "open", worktree.name],
          run: () => onOpenRepoTab(worktree.path),
        });
        // Removing a worktree has a real confirmation dialog in WorktreePanel —
        // navigate there instead of removing directly from the palette.
        commands.push({
          id: `remove-worktree:${worktree.name}`,
          label: `Remove worktree ${worktree.name}`,
          keywords: ["worktree", "remove", "delete", worktree.name],
          run: () => goToSidebarSection("Worktrees"),
        });
      }
    }

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
      }
    }

    // Restoring a reflog entry has a real confirmation dialog in ReflogPanel,
    // and state.reflog is unbounded (a full, uncapped reflog read for the
    // selected reference) — so this is not a per-entry loop. The existing
    // "Go to Reflog" command below covers navigating there; no separate
    // restore-reflog command is emitted.
  }

  for (const title of SIDEBAR_SECTIONS) {
    const panelId = sidebarSectionPanelId(title);
    if (panelId !== null && !panelVisibility[panelId]) continue;
    commands.push({
      id: `go-to:${title}`,
      label: `Go to ${title}`,
      keywords: ["go", "navigate", title.toLowerCase()],
      run: () => goToSidebarSection(title),
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

function scoreCommand(command: Command, query: string): number | null {
  const q = query.trim().toLowerCase();
  if (q === "") return 0;
  const label = command.label.toLowerCase();
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

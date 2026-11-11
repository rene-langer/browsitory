import { useCallback, useEffect, useRef, useState } from "react";
import type { CommitLocation, RepoClient } from "../ipc/RepoClient";
import { type SavedGraphSelection, toGraphSelection } from "./graphSelection";
import type { SelectedRow, UseAppStateResult } from "./useAppState";

/**
 * Why a "Go to" target has no row in the graph, and the one change that would show it. Stays
 * until dismissed, the next "Go to", or (for a commit that was found) another row is selected.
 */
export interface GoToNotice {
  message: string;
  /** The commit the notice is about; `null` when nothing was found. */
  commitId: string | null;
  action: { label: string; run: () => void } | null;
}

type Selection = SavedGraphSelection;

/** `keepSearch`: an open search stays open, for its previous/next match (#687). */
export interface RevealOptions {
  keepSearch?: boolean;
}

/** A notice as found: its action changes the graph's selection to `selection`, then goes again. */
interface Explanation {
  message: string;
  commitId: string | null;
  label: string;
  action: { label: string; show: () => Promise<void>; selection: Selection } | null;
}

export interface UseGoToResult {
  /**
   * Selects `revision`'s commit (a full id, SHA prefix, branch or tag name) and scrolls the
   * graph to it, loading more history when it is further down. When the graph doesn't show it,
   * the commit is still selected and `notice` says why. `label` names the target in messages.
   */
  goToRevision(revision: string, label?: string, options?: RevealOptions): Promise<void>;
  goToBranch(kind: "local" | "remote", name: string): Promise<void>;
  /** `commitId` `null`: a tag of a tree or blob, which has no commit to show. */
  goToTag(name: string, commitId: string | null): Promise<void>;
  notice: GoToNotice | null;
  dismissNotice(): void;
}

function list(names: string[]): string {
  if (names.length <= 2) return names.join(" and ");
  return `${names.slice(0, 2).join(", ")} and ${names.length - 2} more`;
}

/**
 * The "Go to commit / branch / tag" logic (#441): never touches the working tree, only the
 * selection, the loaded history and (when the user asks) the graph's branch selection.
 * `reveal` selects a row and scrolls the graph to it (closing search, file history, compare).
 */
export function useGoTo(
  client: RepoClient,
  repoPath: string,
  appState: UseAppStateResult,
  reveal: (row: SelectedRow, options?: RevealOptions) => void,
): UseGoToResult {
  const [notice, setNotice] = useState<Explanation | null>(null);
  const generation = useRef(0);
  const latest = useRef(appState);
  useEffect(() => {
    latest.current = appState;
  }, [appState]);

  const run = useCallback(
    async (revision: string, label: string, selection?: Selection, options?: RevealOptions): Promise<void> => {
      const ticket = ++generation.current;
      setNotice(null);
      const { state } = latest.current;
      if (selection === undefined && state.commits.some((commit) => commit.id === revision)) {
        reveal({ commitId: revision }, options);
        return;
      }
      const { local, remote } = selection ?? state.graphSelection;
      let location: CommitLocation | null;
      try {
        location = await client.locateCommit(
          repoPath,
          revision,
          toGraphSelection({ local, remote }, state.collapseMerges),
        );
      } catch (error) {
        if (ticket !== generation.current) return;
        setNotice({ message: error instanceof Error ? error.message : String(error), commitId: null, label, action: null });
        return;
      }
      if (ticket !== generation.current) return;
      if (location === null) {
        setNotice({ message: `No commit matches “${label}”.`, commitId: null, label, action: null });
        return;
      }
      const { commitId, position } = location;
      reveal({ commitId }, options);
      if (position !== null) {
        if (!latest.current.state.commits.some((commit) => commit.id === commitId)) {
          await latest.current.loadHistoryThrough(position + 1);
        }
        return;
      }
      // Inside a collapsed merge (#617): load the graph through the merge's row, then expand the
      // merge far enough to list the commit; the selection then has its row.
      const { merge } = location;
      if (merge !== undefined) {
        if (!latest.current.state.commits.some((commit) => commit.id === merge.mergeId)) {
          await latest.current.loadHistoryThrough(merge.mergePosition + 1);
        }
        if (ticket !== generation.current) return;
        await latest.current.expandMerge(merge.mergeId, merge.offset + 1);
        return;
      }
      setNotice(explainHidden(location, label, local, remote, latest.current));
    },
    [client, repoPath, reveal],
  );

  const goToRevision = useCallback(
    (revision: string, label?: string, options?: RevealOptions) => run(revision, label ?? revision, undefined, options),
    [run],
  );

  const goToBranch = useCallback(
    (kind: "local" | "remote", name: string) => {
      const loaded = latest.current.state.commits.find((commit) =>
        (kind === "local" ? commit.branchRefs : commit.remoteBranchRefs).includes(name),
      );
      if (loaded !== undefined) return run(loaded.id, name);
      return run(kind === "local" ? `refs/heads/${name}` : `refs/remotes/${name}`, name);
    },
    [run],
  );

  const goToTag = useCallback(
    async (name: string, commitId: string | null) => {
      if (commitId === null) {
        generation.current++;
        setNotice({
          message: `Tag ${name} points at a file or folder, not a commit.`,
          commitId: null,
          label: name,
          action: null,
        });
        return;
      }
      await run(commitId, name);
    },
    [run],
  );

  const dismissNotice = useCallback(() => setNotice(null), []);

  // The notice's button: changes the graph's selection as the notice says, then goes again.
  const runNoticeAction = useCallback(() => {
    if (notice === null || notice.action === null || notice.commitId === null) return;
    const { commitId, label, action } = notice;
    void action.show().then(() => run(commitId, label, action.selection));
  }, [notice, run]);

  // A notice about a commit goes once another row is selected.
  const selectedRow = appState.state.selectedRow;
  const stale =
    notice !== null &&
    notice.commitId !== null &&
    (selectedRow === "uncommitted" || selectedRow.commitId !== notice.commitId);
  const shownNotice: GoToNotice | null =
    notice === null || stale
      ? null
      : {
          message: notice.message,
          commitId: notice.commitId,
          action: notice.action === null ? null : { label: notice.action.label, run: runNoticeAction },
        };

  return { goToRevision, goToBranch, goToTag, notice: shownNotice, dismissNotice };
}

function explainHidden(
  location: CommitLocation,
  label: string,
  local: string[] | null,
  remote: string[] | null,
  appState: UseAppStateResult,
): Explanation {
  const { commitId } = location;
  const { state } = appState;
  const shownLocal = local ?? state.branches.map((branch) => branch.name);
  const shownRemote = remote ?? [];
  const hiddenLocal = location.branches.filter((name) => !shownLocal.includes(name));
  const hiddenRemote = location.remoteBranches.filter((name) => !shownRemote.includes(name));
  const what = label === commitId ? commitId.slice(0, 7) : label;

  if (hiddenLocal.length > 0 || hiddenRemote.length > 0) {
    const hidden = [...hiddenLocal, ...hiddenRemote];
    const target = hidden[0];
    const branchWord = hidden.length === 1 ? "branch" : "branches";
    let show: () => Promise<void>;
    let selection: Selection;
    if (hiddenLocal.length > 0) {
      const next = [...shownLocal, target];
      const everyBranch = state.branches.every((branch) => next.includes(branch.name));
      selection = { local: everyBranch ? null : next, remote };
      show = () => (everyBranch ? appState.clearGraphBranchSelection() : appState.setGraphBranchSelection(next));
    } else {
      // As BranchTree's swatch: without a saved remote selection the badges in the graph count
      // as shown.
      const badges = Array.from(new Set(state.commits.flatMap((commit) => commit.remoteBranchRefs)));
      const next = [...(remote ?? badges), target];
      selection = { local, remote: next };
      show = () => appState.setGraphRemoteBranchSelection(next);
    }
    return {
      message: `${what} is not in the graph: it is only on ${branchWord} ${list(hidden)}, hidden in the graph.`,
      commitId,
      label,
      action: { label: `Show ${target}`, show, selection },
    };
  }

  if (location.branches.length > 0 || location.remoteBranches.length > 0) {
    // Every branch with it is shown: the graph follows one isolated branch's first parents.
    const isolated = [...location.branches, ...location.remoteBranches][0];
    return {
      message: `${what} is on a side branch merged into ${isolated}; the graph follows only the first parents of ${isolated}.`,
      commitId,
      label,
      action: { label: "Show all branches", show: appState.showAllGraphBranches, selection: { local: null, remote: null } },
    };
  }

  return {
    message: `${what} is not on any branch, so the graph does not show it.`,
    commitId,
    label,
    action: null,
  };
}

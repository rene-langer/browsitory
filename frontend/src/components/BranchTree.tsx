import { stableMemo } from "../lib/stableMemo";
import { useEffect, useRef, useState } from "react";
import type { BranchLaneColors } from "../lib/laneColors";
import { GitBranch, Plus } from "lucide-react";
import type { BranchInfo, RemoteInfo, UpstreamInfo } from "../ipc/RepoClient";
import { rectAnchor } from "../lib/menuKeys";
import type { OperationBlockers } from "../lib/operationStatus";
import { scrollBehavior } from "../lib/scrollBehavior";
import { useCopyToClipboard } from "../state/useCopyToClipboard";
import type { SidebarRevealRequest } from "../state/useSidebarReveal";
import type { RemoteDialog } from "../state/workspaceDialog";
import { AccordionSection } from "./primitives/AccordionSection";
import { ConfirmDialog } from "./primitives/ConfirmDialog";
import { ContextMenu } from "./primitives/ContextMenu";
import { InlineError } from "./primitives/InlineError";
import { AddRemoteForm } from "./AddRemoteForm";
import {
  branchDropActions,
  DROP_NEEDS_BRANCH,
  type BranchDrop,
  type RebaseTarget,
} from "../lib/actions/branchActions";
import { scrollParent, usePointerDrag } from "../state/usePointerDrag";
import { useCommitDrag, type CommitDropTarget } from "../state/useCommitDrag";
import {
  COMMIT_DROP_NEEDS_BRANCH,
  commitDropActions,
  type CommitDrop,
  type DraggedCommits,
} from "../lib/actions/commitActions";
import { branchDropKey, parseBranchDropKey } from "../lib/branchDropKey";
import {
  addMenuItems,
  branchDropMenuItems,
  commitDropMenuItems,
  localBranchMenuItems,
  remoteBranchMenuItems,
  remoteFolderMenuItems,
  rowMenuKey,
  type RowContextMenu,
} from "./branchMenuItems";
import { BRANCH_DROP_SCOPE, BranchTreeView, type BranchTreeDrag } from "./BranchTreeView";
import { NewBranchForm } from "./NewBranchForm";
import { RenameBranchDialog } from "./RenameBranchDialog";
import { SetUpstreamDialog } from "./SetUpstreamDialog";
import { useRemoteBranchCache } from "./useRemoteBranchCache";
import styles from "./BranchTree.module.css";
import { useRefocusRemovedRow } from "./useRefocusRemovedRow";
import type { UserCommand } from "../ipc/RepoClient";
import { NO_SELECTION, userCommandMenuItems, type RunUserCommand } from "../lib/userCommands";
import { useUserCommandsContext } from "../state/userCommandsContext";

/**
 * The sidebar's Branches section: the local and remote branch tree (`BranchTreeView`), its
 * context menus (`branchMenuItems.ts`), the inline New branch and Add remote forms, the Rename
 * branch dialog (#1002) and the upstream dialogs (the current branch's menu opens them, #1025). Pull lives in the repository toolbar, and its diverged
 * prompt in `WorkspaceDialogs` (#636, #637). A remote's Edit / Credentials / Remove dialog is opened
 * through `onOpenRemoteDialog` and rendered by its owner (`WorkspaceDialogs`' `RemoteDialogs`).
 */
function BranchTreeContent({
  branches,
  createBranchDraft,
  onSwitchBranch,
  onCreateBranch,
  onDeleteBranch,
  onCountCommitsOnlyOnBranch,
  onRenameBranch,
  onOpenCreateBranchDraft,
  onCloseCreateBranchDraft,
  onMergeBranch,
  onRebaseOnto,
  operationDisabled,
  operationDisabledReason,
  graphBranchSelection,
  onSetGraphBranchSelection,
  onClearGraphBranchSelection,
  onShowAllGraphBranches,
  graphRemoteBranchSelection,
  onSetGraphRemoteBranchSelection,
  onIsolateGraphBranch,
  remoteBranchesInGraph,
  branchColors,
  remotes,
  upstream,
  onAddRemote,
  onSetUpstream,
  onClearUpstream,
  onListRemoteBranches,
  onFetchRemote,
  onPushCurrentBranch,
  addRemoteDraftOpen,
  onOpenAddRemoteDraft,
  onCloseAddRemoteDraft,
  onOpenPushDialog,
  onOpenDeleteRemoteBranch,
  onCompareBranch,
  onGoToBranch,
  refreshGeneration,
  onOpenRemoteDialog,
  revealRequest,
  userCommands: userCommandsProp,
  onRunUserCommand: onRunUserCommandProp,
}: {
  /** The user commands (#895); those set for branches close the branch menus. */
  userCommands?: UserCommand[];
  onRunUserCommand?: RunUserCommand;
  branches: BranchInfo[];
  createBranchDraft: { startPoint: string } | null;
  onSwitchBranch: (name: string) => void;
  // `null` = created; a string = the failure message to show inline next to the draft form. See
  // `useAppState.ts`'s `createBranch` (issue #30/UX-002).
  // `checkout` (default true): switch to the new branch too (#656).
  onCreateBranch: (name: string, startPoint: string, checkout?: boolean) => Promise<string | null>;
  onDeleteBranch: (name: string, force: boolean) => Promise<void>;
  /** How many commits of a branch no other branch has: Delete asks first when some (#996). */
  onCountCommitsOnlyOnBranch: (name: string) => Promise<number>;
  /** Resolves to `null` once renamed, else the failure the Rename branch dialog shows (#422). */
  onRenameBranch: (oldName: string, newName: string) => Promise<string | null>;
  onOpenCreateBranchDraft: (startPoint: string) => void;
  onCloseCreateBranchDraft: () => void;
  onMergeBranch: (name: string) => void;
  /** "Rebase current branch onto <name>…" (#619): opens the planner onto a full ref name. */
  onRebaseOnto: (onto: string) => void;
  // `operationBlockers` (#495): every branch and remote action follows it, and so does the New
  // branch form's Create (#561), which can also switch to the new branch.
  operationDisabled: boolean;
  // Human-readable reason `operationDisabled` is true (e.g. "A rebase is in progress."), shown
  // as a `title` on the buttons it disables so they don't just go inert with no explanation
  // (issue #31/UX-003). `null` when nothing is blocking.
  operationDisabledReason: string | null;
  // `null` means no filter is saved — every branch shows in CommitGraph (see `graph_log` in
  // `git-core` and `useAppState`'s `AppState.graphBranchSelection`).
  graphBranchSelection: string[] | null;
  onSetGraphBranchSelection: (selectedBranches: string[]) => void;
  // Forgets the saved local selection (`null` again). A dot click that turns every local
  // branch on calls it instead of saving their names, so later branches show too and a repo
  // with one branch keeps the full walk (#402).
  onClearGraphBranchSelection: () => void;
  // "Show all branches": forgets both selections, back to the default graph (#402).
  onShowAllGraphBranches: () => void;
  // Same shape as `graphBranchSelection`/`onSetGraphBranchSelection` above, but scoped to
  // remote-tracking branches, keyed by their qualified `<remoteName>/<branchName>` name (see
  // `useAppState`'s `AppState.graphRemoteBranchSelection`). A selected remote branch's commits
  // join the graph (#385); `null` walks none and shows every remote badge in the graph.
  graphRemoteBranchSelection: string[] | null;
  onSetGraphRemoteBranchSelection: (selectedBranches: string[]) => void;
  // "Isolate branch" on a local or a remote branch: shows only that branch, clearing the other
  // selection (see `useBranchActions`' `isolateGraphBranch`).
  onIsolateGraphBranch: (kind: "local" | "remote", name: string) => void;
  // Every remote-tracking ref actually present in the currently-loaded commit graph (same source
  // CommitGraph.tsx's own null-selection fallback uses: `commits.flatMap((c) => c.remoteBranchRefs)`,
  // deduped). Used ONLY as the null-selection fallback below (the remote swatch's `aria-pressed`
  // and `toggleGraphRemoteBranch`'s `shown` computation) — deliberately NOT `remoteBranches`
  // (this component's lazy per-remote-folder fetch cache, which only has entries for remotes
  // whose folder has been expanded and would silently drop never-expanded remotes' badges on
  // the first toggle).
  remoteBranchesInGraph: string[];
  /** Each branch tip's lane colour in the graph, for the swatches (#673). */
  branchColors?: BranchLaneColors;
  remotes: RemoteInfo[];
  upstream: UpstreamInfo | null;
  onAddRemote: (name: string, fetchUrl: string, pushUrl: string | null) => Promise<string | null>;
  onSetUpstream: (remoteName: string, remoteBranch: string) => Promise<void>;
  onClearUpstream: () => Promise<void>;
  onListRemoteBranches: (remoteName: string) => Promise<string[]>;
  onFetchRemote: (remoteName: string) => Promise<void>;
  onPushCurrentBranch: (remoteName: string) => Promise<void>;
  addRemoteDraftOpen: boolean;
  onOpenAddRemoteDraft: () => void;
  onCloseAddRemoteDraft: () => void;
  /** "Push to…" on a local branch: opens the shared Push dialog for it. */
  onOpenPushDialog: (localBranch: string) => void;
  /** "Delete <remote>/<branch>…" on a remote branch: opens the delete confirmation. */
  onOpenDeleteRemoteBranch: (remoteName: string, branchName: string) => void;
  /**
   * "Compare with current branch" on a non-current local branch or a remote-tracking branch:
   * `refName` is the full ref (`refs/heads/…`, `refs/remotes/…`), `label` its display name.
   */
  onCompareBranch?: (refName: string, label: string) => void;
  /**
   * A click on a branch row selects and scrolls to its tip in the graph (#441); a remote one as
   * `<remote>/<branch>`. Double-click (or Enter) still switches / checks out.
   */
  onGoToBranch?: (kind: "local" | "remote", name: string) => void;
  /**
   * `AppState.refreshGeneration`. A change means remote branches may have changed (a fetch from
   * anywhere, a push that published one, a remote branch delete): open remote folders reload
   * their list in place and closed ones forget theirs.
   */
  refreshGeneration: number;
  /**
   * A remote folder menu's Edit / Credentials / Remove: opens that dialog in app state (#493), so
   * the palette opens the same one. The owner renders it outside the sidebar, which stays mounted
   * but `hidden` while collapsed (a modal there would be invisible).
   */
  onOpenRemoteDialog: (dialog: RemoteDialog) => void;
  /**
   * A reveal request for the Branches section (`useSidebarReveal`, #494): expands and scrolls to
   * the section, or with `remote` to that remote's folder, expanded so its branch rows can be
   * reached by Tab. The value it mounts with is not a request.
   */
  revealRequest?: Pick<SidebarRevealRequest, "nonce" | "remote">;
}) {
  const userCommandsApi = useUserCommandsContext();
  const userCommands = userCommandsProp ?? userCommandsApi?.commands ?? [];
  const onRunUserCommand = onRunUserCommandProp ?? userCommandsApi?.run;
  // The branch Delete asks about, with the commits only it has (`null`: couldn't count them).
  const [deleteConfirm, setDeleteConfirm] = useState<{ name: string; count: number | null } | null>(null);
  // The branches whose commits Delete is counting (#1067): their Delete is off meanwhile, so a
  // second click starts no second count and no second delete.
  const [counting, setCounting] = useState<ReadonlySet<string>>(new Set());
  // The branch the Rename branch dialog renames (#1002), `null` while it's closed.
  const [renaming, setRenaming] = useState<string | null>(null);
  // The new name while a rename runs: the list already shows it (optimistically), and the dialog
  // must stay open to show a failure (#996).
  const [renamingTo, setRenamingTo] = useState<string | null>(null);
  const [rowMenu, setRowMenu] = useState<RowContextMenu | null>(null);
  // The branch whose delete runs, watched with its menu and confirmation (#1141).
  const [deleting, setDeleting] = useState<string | null>(null);
  const [upstreamDialogOpen, setUpstreamDialogOpen] = useState(false);
  const [clearUpstreamConfirm, setClearUpstreamConfirm] = useState(false);
  const [checkoutError, setCheckoutError] = useState<string | null>(null);
  // What the status region reads out about a branch drag (#744).
  const [announcement, setAnnouncement] = useState("");
  const treeRef = useRef<HTMLDivElement>(null);
  const remoteCache = useRemoteBranchCache({ remotes, listRemoteBranches: onListRemoteBranches, refreshGeneration });
  const copyToClipboard = useCopyToClipboard();
  // The menus' availability and labels come from the shared action rules (#509), like the palette's.
  const blockers: OperationBlockers = { blocked: operationDisabled, reason: operationDisabledReason };

  // A remote's reveal request opens its folder during render (not in an effect, which
  // `react-hooks/set-state-in-effect` forbids), so the folder's header is there for the effect
  // below to scroll to, even when the section itself was collapsed until this render.
  const [seenRevealRequest, setSeenRevealRequest] = useState(revealRequest?.nonce);
  if (revealRequest?.nonce !== seenRevealRequest) {
    setSeenRevealRequest(revealRequest?.nonce);
    const remote = revealRequest?.remote;
    if (remote !== undefined) remoteCache.markOpen(remote);
  }
  const remoteHeaders = useRef(new Map<string, HTMLButtonElement>());
  const handledRevealRequest = useRef(revealRequest?.nonce);
  useEffect(() => {
    if (revealRequest === undefined || revealRequest.nonce === handledRevealRequest.current) return;
    handledRevealRequest.current = revealRequest.nonce;
    const remote = revealRequest.remote;
    if (remote === undefined) return;
    remoteCache.persistOpenAndLoad(remote);
    remoteHeaders.current.get(remote)?.scrollIntoView?.({ behavior: scrollBehavior(), block: "nearest" });
    // Only a new request triggers this.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [revealRequest]);

  const checkoutRemoteBranch = async (remoteName: string, branchName: string) => {
    const existingLocal = branches.find((b) => b.name === branchName);
    if (existingLocal !== undefined) {
      onSwitchBranch(branchName);
      return;
    }
    const failure = await onCreateBranch(branchName, `${remoteName}/${branchName}`);
    if (failure !== null) {
      setCheckoutError(failure);
      return;
    }
    await onSetUpstream(remoteName, branchName);
  };

  const openRowMenu = (menu: RowContextMenu) => setRowMenu(menu);
  // For the menu buttons ("…", "Add"): a second click on the button of the open menu closes it.
  const isRowMenuTrigger = (button: Element) =>
    rowMenu !== null && button.getAttribute("data-menu-key") === rowMenuKey(rowMenu);
  // The button carries `data-menu-key` so its menu knows it as its own button however it opened.
  const toggleRowMenu = (menu: RowContextMenu) => {
    if (rowMenu !== null && rowMenuKey(rowMenu) === rowMenuKey(menu)) {
      // Focus stays on the button (WebKit may not have focused it on the click).
      menu.returnFocusTo?.focus({ preventScroll: true });
      setRowMenu(null);
    } else openRowMenu(menu);
  };

  // A branch dragged onto a branch (#744): what the drop would do comes from the menus' rules.
  const refName = (ref: RebaseTarget) => (ref.kind === "local" ? ref.name : `${ref.remoteName}/${ref.branchName}`);
  const currentBranch = branches.find((branch) => branch.isCurrent)?.name ?? null;
  const dropAt = (source: RebaseTarget, key: string | undefined): { target: RebaseTarget; drop: BranchDrop } | null => {
    const target = key === undefined ? null : parseBranchDropKey(key);
    return target === null ? null : { target, drop: branchDropActions(blockers, branches, source, target) };
  };
  const { drag, startDrag } = usePointerDrag<RebaseTarget>({
    scope: BRANCH_DROP_SCOPE,
    autoScroll: () => scrollParent(treeRef.current),
    onStart: (source) => {
      setRowMenu(null);
      const name = refName(source);
      if (currentBranch === null) setAnnouncement(`Dragging ${name}. ${DROP_NEEDS_BRANCH}`);
      else if (source.kind === "local" && source.name === currentBranch) {
        setAnnouncement(`Dragging ${name}. Drop it on a branch to rebase onto it or merge it; Escape cancels.`);
      } else setAnnouncement(`Dragging ${name}. Drop it on ${currentBranch} to merge or rebase; Escape cancels.`);
    },
    onCancel: () => setAnnouncement("Drag cancelled."),
    onDrop: (source, over) => {
      const hit = dropAt(source, over?.key);
      if (hit === null || hit.drop.kind === "none") {
        setAnnouncement("Drag cancelled.");
        return;
      }
      if (hit.drop.kind === "refused") {
        setAnnouncement(`Can't drop here. ${hit.drop.reason}`);
        return;
      }
      setAnnouncement(
        `Dropped ${refName(source)} on ${refName(hit.target)}: ${hit.drop.items.map((item) => item.label).join(", or ")}`,
      );
      setRowMenu({
        kind: "branch-drop",
        source,
        target: hit.target,
        x: drag?.x ?? 0,
        y: drag?.y ?? 0,
        returnFocusTo: over?.element,
      });
    },
  });

  // Commits dragged from the graph onto a branch (#744): the graph starts the drag, this section
  // is its drop side. Only the current branch takes a cherry-pick.
  const commitDrag = useCommitDrag();
  const draggedName = (dragged: DraggedCommits) =>
    dragged.commits.length === 1 ? `commit ${dragged.commits[0].shortId}` : `${dragged.commits.length} commits`;
  const commitDropAt = (
    dragged: DraggedCommits,
    key: string | undefined,
  ): { target: RebaseTarget; drop: CommitDrop } | null => {
    const target = key === undefined ? null : parseBranchDropKey(key);
    return target === null ? null : { target, drop: commitDropActions(blockers, branches, dragged, target) };
  };
  const commitDropTarget: CommitDropTarget = {
    autoScroll: () => scrollParent(treeRef.current),
    onStart: (dragged) => {
      setRowMenu(null);
      const name = draggedName(dragged);
      if (currentBranch === null) setAnnouncement(`Dragging ${name}. ${COMMIT_DROP_NEEDS_BRANCH}`);
      else {
        const them = dragged.commits.length === 1 ? "it" : "them";
        setAnnouncement(`Dragging ${name}. Drop ${them} on ${currentBranch} to cherry-pick; Escape cancels.`);
      }
    },
    onCancel: () => setAnnouncement("Drag cancelled."),
    onDrop: (dragged, over) => {
      const hit = commitDropAt(dragged, over?.key);
      if (hit === null) {
        setAnnouncement("Drag cancelled.");
        return;
      }
      if (hit.drop.kind === "refused") {
        setAnnouncement(`Can't drop here. ${hit.drop.reason}`);
        return;
      }
      setAnnouncement(`Dropped ${draggedName(dragged)} on ${refName(hit.target)}: ${hit.drop.item.label}`);
      setRowMenu({
        kind: "commit-drop",
        dragged,
        target: hit.target,
        x: commitDrag?.drag?.x ?? 0,
        y: commitDrag?.drag?.y ?? 0,
        returnFocusTo: over?.element,
      });
    },
  };
  // The session calls the latest handlers, which close over this render's branches and state.
  const commitDropTargetRef = useRef(commitDropTarget);
  useEffect(() => {
    commitDropTargetRef.current = commitDropTarget;
  });
  const registerCommitDropTarget = commitDrag?.registerTarget;
  useEffect(
    () =>
      registerCommitDropTarget?.({
        autoScroll: () => commitDropTargetRef.current.autoScroll?.() ?? null,
        onStart: (dragged) => commitDropTargetRef.current.onStart(dragged),
        onCancel: () => commitDropTargetRef.current.onCancel(),
        onDrop: (dragged, over) => commitDropTargetRef.current.onDrop(dragged, over),
      }),
    [registerCommitDropTarget],
  );

  const hovered = drag === null ? null : dropAt(drag.payload, drag.over?.key);
  const commitDragState = commitDrag?.drag ?? null;
  const commitHovered =
    commitDragState === null ? null : commitDropAt(commitDragState.payload, commitDragState.over?.key);
  const treeDrag: BranchTreeDrag | null =
    drag !== null
      ? {
          sourceKey: branchDropKey(drag.payload),
          overKey: drag.over?.key ?? null,
          overState:
            hovered === null || hovered.drop.kind === "none" ? null : hovered.drop.kind === "menu" ? "on" : "refused",
        }
      : commitDragState !== null
        ? {
            sourceKey: null,
            overKey: commitDragState.over?.key ?? null,
            overState: commitHovered === null ? null : commitHovered.drop.kind === "menu" ? "on" : "refused",
          }
        : null;
  // The label that follows the pointer: what the drop offers, or why it can't run.
  const dragLabel =
    drag !== null
      ? hovered === null || hovered.drop.kind === "none"
        ? refName(drag.payload)
        : hovered.drop.kind === "menu"
          ? hovered.drop.items.map((item) => item.label).join(" / ")
          : hovered.drop.reason
      : commitDragState !== null
        ? commitHovered === null
          ? draggedName(commitDragState.payload)
          : commitHovered.drop.kind === "menu"
            ? commitHovered.drop.item.label
            : commitHovered.drop.reason
        : null;
  const labelAt = drag ?? commitDragState;

  const deleteBranch = async (name: string) => {
    setDeleting(name);
    try {
      await onDeleteBranch(name, true);
    } finally {
      setDeleting((current) => (current === name ? null : current));
    }
  };
  // A deleted branch, or one a refresh removed while its menu was open: the focus goes to the
  // row now in that place (#1141).
  const localKey = (name: string | null | undefined) => (name == null ? null : `local-branch:${name}`);
  useRefocusRemovedRow({
    list: treeRef,
    watching: [
      rowMenu === null ? null : rowMenuKey(rowMenu),
      localKey(deleteConfirm?.name),
      localKey(deleting),
      ...[...counting].map(localKey),
    ],
    rows: 'button[data-menu-key]:not([data-menu-key="add"])',
  });

  const handleDeleteClick = async (name: string) => {
    // Counted first (#996): a branch whose commits another branch has loses nothing and goes at
    // once; otherwise one confirm says what would be left on no branch. Either way it is deleted
    // with force, since git's own check (merged into HEAD) is what this replaces.
    if (counting.has(name)) return;
    setCounting((prev) => new Set(prev).add(name));
    let count: number | null;
    try {
      count = await onCountCommitsOnlyOnBranch(name);
    } catch {
      count = null;
    } finally {
      setCounting((prev) => {
        const next = new Set(prev);
        next.delete(name);
        return next;
      });
    }
    if (count === 0) void deleteBranch(name);
    else setDeleteConfirm({ name, count });
  };

  // "Show all branches" in the "+" menu and every branch menu (#985).
  const graphFilter = {
    active: graphBranchSelection !== null || graphRemoteBranchSelection !== null,
    onShowAll: onShowAllGraphBranches,
  };

  // Why the force delete and upstream dialogs (#521), the New branch form and the remote dialogs
  // (#561) can't act now, shown in them; `null` when nothing blocks.
  const busyReason = operationDisabled ? operationDisabledReason : null;

  // What every row menu's `ContextMenu` shares; `key` gives each target a fresh one.
  const menuProps = (menu: RowContextMenu) => ({
    isTrigger: isRowMenuTrigger,
    x: menu.x,
    y: menu.y,
    returnFocusTo: menu.returnFocusTo,
    onClose: () => setRowMenu(null),
  });

  return (
    <>
      <AccordionSection
        title="Branches"
        storageKey="sidebar-branches"
        revealRequest={revealRequest?.nonce}
        icon={GitBranch}
        count={branches.length}
        defaultOpen
        actions={
          <button
            type="button"
            className={styles.iconButton}
            aria-label="Add"
            aria-haspopup="menu"
            data-menu-key="add"
            aria-expanded={rowMenu?.kind === "add"}
            onClick={(event) => toggleRowMenu({ kind: "add", ...rectAnchor(event.currentTarget), returnFocusTo: event.currentTarget })}
          >
            <Plus size={14} aria-hidden="true" />
          </button>
        }
      >
        <NewBranchForm
          draft={createBranchDraft}
          branches={branches}
          operationDisabled={operationDisabled}
          busyReason={busyReason}
          onCreateBranch={onCreateBranch}
          onClose={onCloseCreateBranchDraft}
        />

        <AddRemoteForm
          open={addRemoteDraftOpen}
          remotes={remotes}
          operationDisabled={operationDisabled}
          operationDisabledReason={operationDisabledReason}
          onAddRemote={onAddRemote}
          onClose={onCloseAddRemoteDraft}
        />

        <p role="status" className={styles.srOnly}>
          {announcement}
        </p>
        <div ref={treeRef}>
          <BranchTreeView
            branches={branches}
            remotes={remotes}
            blockers={blockers}
            upstream={upstream}
            graphBranchSelection={graphBranchSelection}
            onSetGraphBranchSelection={onSetGraphBranchSelection}
            onClearGraphBranchSelection={onClearGraphBranchSelection}
            graphRemoteBranchSelection={graphRemoteBranchSelection}
            onSetGraphRemoteBranchSelection={onSetGraphRemoteBranchSelection}
            remoteBranchesInGraph={remoteBranchesInGraph}
            branchColors={branchColors}
            remoteCache={remoteCache}
            remoteHeaders={remoteHeaders}
            rowMenu={rowMenu}
            onOpenRowMenu={openRowMenu}
            onToggleRowMenu={toggleRowMenu}
            onSwitchBranch={onSwitchBranch}
            onCheckoutRemoteBranch={(remoteName, branchName) => void checkoutRemoteBranch(remoteName, branchName)}
            onGoToBranch={onGoToBranch}
            drag={treeDrag}
            onStartDrag={startDrag}
          />
        </div>
        {labelAt !== null && dragLabel !== null && (
          <div className={styles.dragLabel} style={{ left: labelAt.x + 12, top: labelAt.y + 12 }} aria-hidden="true">
            {dragLabel}
          </div>
        )}

        {checkoutError !== null && <InlineError message={checkoutError} onDismiss={() => setCheckoutError(null)} />}
        {remoteCache.error !== null && <InlineError message={remoteCache.error} onDismiss={remoteCache.clearError} />}

        {/* Guarded by `branches.some(...)`: a branch that went meanwhile (a refresh, another
            window) has nothing left to delete. */}
        {deleteConfirm !== null && branches.some((b) => b.name === deleteConfirm.name) && (
          <ConfirmDialog
            title={`Delete ${deleteConfirm.name}`}
            message={
              <p>
                {deleteConfirm.count === null
                  ? `Delete ${deleteConfirm.name}? Commits that no other branch has would be on no branch.`
                  : `${deleteConfirm.name} has ${deleteConfirm.count} ${deleteConfirm.count === 1 ? "commit" : "commits"} on no other branch. Delete it anyway?`}{" "}
                Undo can bring it back.
              </p>
            }
            // Plain, not a danger (#988): Undo brings the branch back.
            confirmLabel="Delete branch"
            confirmDisabled={operationDisabled}
            // #521: the repository can become busy while the confirmation is open.
            confirmReason={busyReason}
            onConfirm={() => {
              if (operationDisabled) return;
              void deleteBranch(deleteConfirm.name);
              setDeleteConfirm(null);
            }}
            onCancel={() => setDeleteConfirm(null)}
          />
        )}

        {rowMenu !== null &&
          rowMenu.kind === "local-branch" &&
          (() => {
            // A non-null assertion here would throw at render time if a background refresh
            // removed this branch while its context menu was still open — guard instead so the
            // menu just doesn't render for a branch that's gone, rather than crashing the app.
            const branch = branches.find((b) => b.name === rowMenu.name);
            return branch !== undefined ? (
              <ContextMenu
                key={rowMenuKey(rowMenu)}
                {...menuProps(rowMenu)}
                items={localBranchMenuItems({
                  branch,
                  branches,
                  blockers,
                  remoteCount: remotes.length,
                  upstream,
                  onSwitchBranch,
                  onStartRename: setRenaming,
                  onMergeBranch,
                  onRebaseOnto,
                  onCompareBranch,
                  onPushCurrentBranch,
                  onOpenSetUpstream: () => setUpstreamDialogOpen(true),
                  onClearUpstream: () => setClearUpstreamConfirm(true),
                  onOpenPushDialog,
                  onIsolateGraphBranch,
                  graphFilter,
                  onCopy: copyToClipboard,
                  onDelete: (name) => void handleDeleteClick(name),
                  counting: counting.has(branch.name),
                }).concat(
                  onRunUserCommand === undefined
                    ? []
                    : userCommandMenuItems(
                        userCommands,
                        "Branch",
                        { ...NO_SELECTION, branch: branch.name, refName: `refs/heads/${branch.name}` },
                        onRunUserCommand,
                      ),
                )}
              />
            ) : null;
          })()}
        {rowMenu !== null &&
          rowMenu.kind === "branch-drop" &&
          (() => {
            // Re-checked at render: a refresh or a busy operation may have changed the answer.
            const drop = branchDropActions(blockers, branches, rowMenu.source, rowMenu.target);
            return drop.kind === "menu" ? (
              <ContextMenu
                key={rowMenuKey(rowMenu)}
                {...menuProps(rowMenu)}
                items={branchDropMenuItems(drop, { onMergeBranch, onRebaseOnto })}
              />
            ) : null;
          })()}
        {rowMenu !== null &&
          rowMenu.kind === "commit-drop" &&
          (() => {
            // Re-checked at render, like a branch drop's menu.
            const drop = commitDropActions(blockers, branches, rowMenu.dragged, rowMenu.target);
            return drop.kind === "menu" && commitDrag !== null ? (
              <ContextMenu
                key={rowMenuKey(rowMenu)}
                {...menuProps(rowMenu)}
                items={commitDropMenuItems(drop, commitDrag.run)}
              />
            ) : null;
          })()}
        {rowMenu !== null && rowMenu.kind === "add" && (
          <ContextMenu
            key={rowMenuKey(rowMenu)}
            {...menuProps(rowMenu)}
            items={addMenuItems({
              hasGraphSelection: graphFilter.active,
              onOpenCreateBranchDraft,
              onOpenAddRemoteDraft,
              onShowAllGraphBranches,
            })}
          />
        )}
        {rowMenu !== null &&
          rowMenu.kind === "remote-folder" &&
          (() => {
            const remote = remotes.find((r) => r.name === rowMenu.name);
            if (remote === undefined) return null;
            const remoteName = remote.name;
            return (
              <ContextMenu
                key={rowMenuKey(rowMenu)}
                {...menuProps(rowMenu)}
                items={remoteFolderMenuItems({
                  remoteName,
                  fetchUrl: remote.fetchUrl,
                  pushUrl: remote.pushUrl,
                  blockers,
                  // The cache's `refreshGeneration` reload already refreshes the branch list after
                  // any fetch; this explicit drop-and-reload just makes the update immediate here.
                  onFetch: () => void onFetchRemote(remoteName).then(() => remoteCache.reload(remoteName)),
                  onPushCurrentBranch: () => void onPushCurrentBranch(remoteName),
                  onEdit: () => onOpenRemoteDialog({ kind: "remote", action: "edit", remoteName }),
                  onCredentials: () => onOpenRemoteDialog({ kind: "remote", action: "credentials", remoteName }),
                  onCopy: copyToClipboard,
                  onRemove: () => onOpenRemoteDialog({ kind: "remote", action: "remove", remoteName }),
                })}
              />
            );
          })()}
        {rowMenu !== null && rowMenu.kind === "remote-branch" && (
          <ContextMenu
            key={rowMenuKey(rowMenu)}
            {...menuProps(rowMenu)}
            items={remoteBranchMenuItems({
              remoteName: rowMenu.remoteName,
              branchName: rowMenu.branchName,
              branches,
              blockers,
              onCheckout: () => void checkoutRemoteBranch(rowMenu.remoteName, rowMenu.branchName),
              onSetUpstream,
              onRebaseOnto,
              onCompareBranch,
              onIsolateGraphBranch,
              graphFilter,
              onCopy: copyToClipboard,
              onOpenDeleteRemoteBranch,
            }).concat(
              onRunUserCommand === undefined
                ? []
                : userCommandMenuItems(
                    userCommands,
                    "Branch",
                    {
                      ...NO_SELECTION,
                      branch: `${rowMenu.remoteName}/${rowMenu.branchName}`,
                      refName: `refs/remotes/${rowMenu.remoteName}/${rowMenu.branchName}`,
                    },
                    onRunUserCommand,
                  ),
            )}
          />
        )}

        {/* Guarded like Delete's confirmation: a branch that went meanwhile has nothing to rename. */}
        {renaming !== null && branches.some((b) => b.name === renaming || b.name === renamingTo) && (
          <RenameBranchDialog
            key={renaming}
            name={renaming}
            // Not the name being given: the running rename put it in the list already.
            branchNames={branches.map((b) => b.name).filter((name) => name !== renamingTo)}
            disabled={operationDisabled}
            disabledReason={busyReason}
            onRename={async (newName) => {
              setRenamingTo(newName);
              const failure = await onRenameBranch(renaming, newName);
              setRenamingTo(null);
              if (failure === null) setRenaming(null);
              return failure;
            }}
            onCancel={() => setRenaming(null)}
          />
        )}

        {clearUpstreamConfirm && upstream !== null && (
          <ConfirmDialog
            title={`Clear upstream of ${upstream.localBranch}`}
            message={
              <>
                <p>
                  Stop {upstream.localBranch} tracking {upstream.remoteName}/{upstream.remoteBranch}?
                </p>
              </>
            }
            confirmLabel="Clear upstream"
            confirmDisabled={operationDisabled}
            confirmReason={busyReason}
            onConfirm={() => {
              if (operationDisabled) return;
              setClearUpstreamConfirm(false);
              void onClearUpstream();
            }}
            onCancel={() => setClearUpstreamConfirm(false)}
          />
        )}

        {upstreamDialogOpen && (
          <SetUpstreamDialog
            currentBranchName={branches.find((b) => b.isCurrent)?.name ?? ""}
            remotes={remotes}
            operationDisabled={operationDisabled}
            busyReason={busyReason}
            onListRemoteBranches={onListRemoteBranches}
            onSetUpstream={onSetUpstream}
            onClose={() => setUpstreamDialogOpen(false)}
          />
        )}

      </AccordionSection>
    </>
  );
}

/** Renders again only when its data changed (#844); see `stableMemo`. */
export const BranchTree = stableMemo(BranchTreeContent, "BranchTree");

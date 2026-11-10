import { useState, type KeyboardEvent } from "react";
import { GitBranch, Plus } from "lucide-react";
import type {
  BranchInfo,
  PullOutcome,
  RemoteAuthMode,
  RemoteInfo,
  UpstreamInfo,
} from "../ipc/RepoClient";
import { rectAnchor } from "../lib/menuKeys";
import type { OperationBlockers } from "../lib/operationStatus";
import { branchNameProblem } from "../lib/refName";
import type { RemoteDialog } from "../state/useAppState";
import { AccordionSection } from "./primitives/AccordionSection";
import { FormDialog } from "./primitives/FormDialog";
import formStyles from "./primitives/FormDialog.module.css";
import { ConfirmDialog } from "./primitives/ConfirmDialog";
import { ContextMenu } from "./primitives/ContextMenu";
import { InlineError } from "./primitives/InlineError";
import { Toolbar } from "./primitives/Toolbar";
import { AddRemoteForm } from "./AddRemoteForm";
import {
  addMenuItems,
  localBranchMenuItems,
  remoteBranchMenuItems,
  remoteFolderMenuItems,
  rowMenuKey,
  type RowContextMenu,
} from "./branchMenuItems";
import { BranchTreeView, type BranchRename } from "./BranchTreeView";
import { NewBranchForm } from "./NewBranchForm";
import { RemoteDialogs } from "./RemoteDialogs";
import { SetUpstreamDialog } from "./SetUpstreamDialog";
import { useRemoteBranchCache } from "./useRemoteBranchCache";
import styles from "./BranchTree.module.css";

/**
 * The sidebar's Branches section: the local and remote branch tree (`BranchTreeView`), its
 * context menus (`branchMenuItems.ts`), the inline New branch and Add remote forms, inline rename,
 * the upstream block and its dialogs, and the remote dialogs when uncontrolled.
 */
export function BranchTree({
  branches,
  createBranchDraft,
  onSwitchBranch,
  onCreateBranch,
  onDeleteBranch,
  onRenameBranch,
  onOpenCreateBranchDraft,
  onCloseCreateBranchDraft,
  onMergeBranch,
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
  remotes,
  upstream,
  remoteUpstreams,
  onAddRemote,
  onRenameRemote,
  onUpdateRemoteUrls,
  onRemoveRemote,
  onSaveHttpsCredential,
  onForgetHttpsCredential,
  onSetRemoteAuthMode,
  onSetUpstream,
  onClearUpstream,
  onListRemoteBranches,
  onFetchRemote,
  onPushCurrentBranch,
  onPull,
  pendingPull,
  pullOutcome,
  onMergePull,
  onRebasePull,
  onCancelPull,
  addRemoteDraftOpen,
  onOpenAddRemoteDraft,
  onCloseAddRemoteDraft,
  onOpenPushDialog,
  onOpenDeleteRemoteBranch,
  onCompareBranch,
  onGoToBranch,
  refreshGeneration,
  remoteDialog: remoteDialogProp,
  onRemoteDialogChange,
}: {
  branches: BranchInfo[];
  createBranchDraft: { startPoint: string } | null;
  onSwitchBranch: (name: string) => void;
  // `null` = created; a string = the failure message to show inline next to the draft form. See
  // `useAppState.ts`'s `createBranch` (issue #30/UX-002).
  onCreateBranch: (name: string, startPoint: string) => Promise<string | null>;
  onDeleteBranch: (name: string, force: boolean) => Promise<void>;
  onRenameBranch: (oldName: string, newName: string) => void;
  onOpenCreateBranchDraft: (startPoint: string) => void;
  onCloseCreateBranchDraft: () => void;
  onMergeBranch: (name: string) => void;
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
  // Remote props: accepted from Task 7 onward for the type to match App.tsx's eventual single
  // call site, rendered starting in Task 8.
  remotes: RemoteInfo[];
  upstream: UpstreamInfo | null;
  remoteUpstreams: Record<string, UpstreamInfo[]>;
  onAddRemote: (name: string, fetchUrl: string, pushUrl: string | null) => Promise<string | null>;
  onRenameRemote: (oldName: string, newName: string) => Promise<boolean>;
  onUpdateRemoteUrls: (name: string, fetchUrl: string, pushUrl: string | null) => Promise<void>;
  onRemoveRemote: (name: string, clearUpstreams: boolean) => Promise<void>;
  onSaveHttpsCredential: (remoteName: string, username: string, token: string) => Promise<void>;
  onForgetHttpsCredential: (remoteName: string) => Promise<void>;
  onSetRemoteAuthMode: (remoteName: string, mode: RemoteAuthMode, username: string | null) => Promise<boolean>;
  onSetUpstream: (remoteName: string, remoteBranch: string) => Promise<void>;
  onClearUpstream: () => Promise<void>;
  onListRemoteBranches: (remoteName: string) => Promise<string[]>;
  onFetchRemote: (remoteName: string) => Promise<void>;
  onPushCurrentBranch: (remoteName: string) => Promise<void>;
  onPull: () => Promise<void>;
  pendingPull: { upstreamRef: string } | null;
  pullOutcome: PullOutcome | null;
  onMergePull: (upstreamRef: string) => Promise<void>;
  onRebasePull: (upstreamRef: string) => void;
  onCancelPull: () => void;
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
   * The open remote dialog (Edit / Credentials / Remove), controlled by app state so the palette
   * can open it. Controlled when `onRemoteDialogChange` is given, local state otherwise (same
   * pattern as `DiffPane`'s `discardRequest`). Controlled, the owner renders the dialogs
   * (`RemoteDialogs`); uncontrolled, this tree does.
   */
  remoteDialog?: RemoteDialog | null;
  onRemoteDialogChange?: (dialog: RemoteDialog | null) => void;
}) {
  const [pendingForceFor, setPendingForceFor] = useState<string | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [rowMenu, setRowMenu] = useState<RowContextMenu | null>(null);
  const [localRemoteDialog, setLocalRemoteDialog] = useState<RemoteDialog | null>(null);
  const remoteDialog = onRemoteDialogChange !== undefined ? (remoteDialogProp ?? null) : localRemoteDialog;
  const setRemoteDialog = onRemoteDialogChange ?? setLocalRemoteDialog;
  const [upstreamDialogOpen, setUpstreamDialogOpen] = useState(false);
  const [clearUpstreamConfirm, setClearUpstreamConfirm] = useState(false);
  const [checkoutError, setCheckoutError] = useState<string | null>(null);
  const remoteCache = useRemoteBranchCache({ remotes, listRemoteBranches: onListRemoteBranches, refreshGeneration });
  // The menus' availability and labels come from the shared action rules (#509), like the palette's.
  const blockers: OperationBlockers = { blocked: operationDisabled, reason: operationDisabledReason };

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

  // Opening a context menu starts a fresh interaction with whatever row triggered it, so any
  // pending rename on a *different* row is dropped rather than staying attached to it — the same
  // guard BranchSwitcher's popover close used to provide (see BranchTree.test.tsx's "right-clicking
  // a different branch after starting a rename on one clears that rename's input").
  const openRowMenu = (menu: RowContextMenu) => {
    setRenaming(null);
    setRenameValue("");
    setRowMenu(menu);
  };
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

  const renameProblem = renaming !== null ? branchNameProblem(renameValue.trim()) : null;

  const handleDeleteClick = async (name: string) => {
    await onDeleteBranch(name, false);
    setPendingForceFor(name);
  };

  // Like the New branch form (#418): the name is trimmed, an unchanged one just closes the input
  // without a backend call, and a name `branchNameProblem` rejects stays open with its message.
  const handleRenameKeyDown = (event: KeyboardEvent<HTMLInputElement>, oldName: string) => {
    if (event.key === "Enter") {
      const newName = renameValue.trim();
      if (newName === "" || renameProblem !== null) return;
      setRenaming(null);
      if (newName !== oldName) onRenameBranch(oldName, newName);
    } else if (event.key === "Escape") {
      event.stopPropagation();
      setRenaming(null);
    }
  };

  const rename: BranchRename = {
    name: renaming,
    value: renameValue,
    problem: renameProblem,
    onChange: setRenameValue,
    onCancel: () => setRenaming(null),
    onKeyDown: handleRenameKeyDown,
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
          onSwitchBranch={onSwitchBranch}
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
          remoteCache={remoteCache}
          rename={rename}
          rowMenu={rowMenu}
          onOpenRowMenu={openRowMenu}
          onToggleRowMenu={toggleRowMenu}
          onSwitchBranch={onSwitchBranch}
          onCheckoutRemoteBranch={(remoteName, branchName) => void checkoutRemoteBranch(remoteName, branchName)}
          onGoToBranch={onGoToBranch}
        />

        {checkoutError !== null && <InlineError message={checkoutError} onDismiss={() => setCheckoutError(null)} />}
        {remoteCache.error !== null && <InlineError message={remoteCache.error} onDismiss={remoteCache.clearError} />}

        {/* Guarded by `branches.some(...)`, not just `pendingForceFor !== null`: the app's
            optimistic-update flow removes a deleted branch from `branches` immediately on a
            successful soft delete, only rolling back on failure. So "is the name still in
            `branches`" is the reliable signal for "did the soft delete actually fail" — without
            it, this dialog would also fire right after every successful delete, since
            `handleDeleteClick` always sets `pendingForceFor` unconditionally. */}
        {pendingForceFor !== null && branches.some((b) => b.name === pendingForceFor) && (
          <ConfirmDialog
            ariaLabel={`Force delete ${pendingForceFor}`}
            message={
              <>
                <p>Force delete "{pendingForceFor}"? This discards any unmerged commits and cannot be undone.</p>
                {/* #521: the repository can become busy while the confirmation is open. */}
                {busyReason !== null && <p className={formStyles.blocked}>{busyReason}</p>}
              </>
            }
            confirmLabel="Force delete"
            confirmDisabled={operationDisabled}
            confirmTitle={busyReason ?? undefined}
            onConfirm={() => {
              if (operationDisabled) return;
              void onDeleteBranch(pendingForceFor, true);
              setPendingForceFor(null);
            }}
            onCancel={() => setPendingForceFor(null)}
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
                  onStartRename: (name) => {
                    setRenaming(name);
                    setRenameValue(name);
                  },
                  onMergeBranch,
                  onCompareBranch,
                  onPushCurrentBranch,
                  onOpenSetUpstream: () => setUpstreamDialogOpen(true),
                  onOpenPushDialog,
                  onIsolateGraphBranch,
                  onDelete: (name) => void handleDeleteClick(name),
                })}
              />
            ) : null;
          })()}
        {rowMenu !== null && rowMenu.kind === "add" && (
          <ContextMenu
            key={rowMenuKey(rowMenu)}
            {...menuProps(rowMenu)}
            items={addMenuItems({
              hasGraphSelection: graphBranchSelection !== null || graphRemoteBranchSelection !== null,
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
                  blockers,
                  // The cache's `refreshGeneration` reload already refreshes the branch list after
                  // any fetch; this explicit drop-and-reload just makes the update immediate here.
                  onFetch: () => void onFetchRemote(remoteName).then(() => remoteCache.reload(remoteName)),
                  onPushCurrentBranch: () => void onPushCurrentBranch(remoteName),
                  onEdit: () => setRemoteDialog({ kind: "edit", remoteName }),
                  onCredentials: () => setRemoteDialog({ kind: "credentials", remoteName }),
                  onRemove: () => setRemoteDialog({ kind: "remove", remoteName }),
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
              onCompareBranch,
              onIsolateGraphBranch,
              onOpenDeleteRemoteBranch,
            })}
          />
        )}

        <section className={styles.upstreamBlock}>
          <h3 className={styles.upstreamHeading}>Upstream</h3>
          {upstream === null ? (
            <p className={styles.helperText}>No upstream for the current branch.</p>
          ) : (
            <p className={styles.helperText}>
              {upstream.localBranch} tracks {upstream.remoteName}/{upstream.remoteBranch}.
            </p>
          )}
          <Toolbar aria-label="Upstream actions">
            <button
              type="button"
              disabled={operationDisabled || upstream === null || pendingPull !== null}
              title={
                operationDisabled
                  ? (operationDisabledReason ?? undefined)
                  : upstream === null
                    ? "No upstream set for the current branch."
                    : undefined
              }
              onClick={() => void onPull()}
            >
              Pull
            </button>
            {upstream !== null && (
              <button
                type="button"
                disabled={operationDisabled}
                title={busyReason ?? undefined}
                onClick={() => setClearUpstreamConfirm(true)}
              >
                Clear upstream
              </button>
            )}
          </Toolbar>
          {pullOutcome?.kind === "UpToDate" && <p role="status">Already up to date.</p>}
        </section>

        {clearUpstreamConfirm && upstream !== null && (
          <ConfirmDialog
            ariaLabel="Clear upstream confirmation"
            message={
              <>
                <p>
                  Stop {upstream.localBranch} tracking {upstream.remoteName}/{upstream.remoteBranch}?
                </p>
                {busyReason !== null && <p className={formStyles.blocked}>{busyReason}</p>}
              </>
            }
            confirmLabel="Clear upstream"
            confirmDisabled={operationDisabled}
            confirmTitle={busyReason ?? undefined}
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

        {pendingPull !== null && (
          <FormDialog ariaLabel="Pull has diverged" onCancel={onCancelPull}>
            <p>The pull has diverged from {pendingPull.upstreamRef}.</p>
            <button type="button" disabled={operationDisabled} onClick={() => void onMergePull(pendingPull.upstreamRef)}>
              Merge
            </button>
            <button type="button" disabled={operationDisabled} onClick={() => onRebasePull(pendingPull.upstreamRef)}>
              Rebase
            </button>
            <button type="button" data-autofocus onClick={onCancelPull}>
              Cancel
            </button>
          </FormDialog>
        )}
      </AccordionSection>
      {/* Controlled (`App`), the dialogs render at app level: this tree sits in the sidebar, which
          stays mounted but `hidden` while collapsed, and a modal there would be invisible. */}
      {onRemoteDialogChange === undefined && (
        <RemoteDialogs
          remoteDialog={remoteDialog}
          remotes={remotes}
          remoteUpstreams={remoteUpstreams}
          blockedReason={busyReason}
          onRenameRemote={onRenameRemote}
          onUpdateRemoteUrls={onUpdateRemoteUrls}
          onRemoveRemote={onRemoveRemote}
          onSaveHttpsCredential={onSaveHttpsCredential}
          onForgetHttpsCredential={onForgetHttpsCredential}
          onSetRemoteAuthMode={onSetRemoteAuthMode}
          onClose={() => setRemoteDialog(null)}
        />
      )}
    </>
  );
}

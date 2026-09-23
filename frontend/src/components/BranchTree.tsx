import { branchNameProblem } from "../lib/refName";
import { useRef, useState, type CSSProperties, type KeyboardEvent, type ReactElement } from "react";
import { ChevronRight, Cloud, Copy, GitBranch, MoreHorizontal, Plus } from "lucide-react";
import type {
  BranchInfo,
  PullOutcome,
  RemoteAuthMode,
  RemoteInfo,
  UpstreamInfo,
} from "../ipc/RepoClient";
import { branchSwatchColor } from "../lib/laneColors";
import { buildBranchTree, type BranchTreeNode } from "../lib/branchTree";
import { loadPersistedOpen, persistOpen } from "../lib/persistedOpenState";
import { AccordionSection } from "./primitives/AccordionSection";
import { FormDialog } from "./primitives/FormDialog";
import { ConfirmDialog } from "./primitives/ConfirmDialog";
import { ContextMenu, type ContextMenuItem } from "./primitives/ContextMenu";
import { InlineError } from "./primitives/InlineError";
import { ListRow } from "./primitives/ListRow";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./BranchTree.module.css";

const LOCAL_FOLDER_KEY = "branchtree.local";

type RowContextMenu =
  | { kind: "local-branch"; name: string; x: number; y: number }
  | { kind: "add"; x: number; y: number }
  | { kind: "remote-folder"; name: string; x: number; y: number }
  | { kind: "remote-branch"; remoteName: string; branchName: string; x: number; y: number };

function deriveRemoteName(fetchUrl: string, existingNames: string[]): string {
  if (!existingNames.includes("origin")) return "origin";
  const withoutGitSuffix = fetchUrl.replace(/\.git\/?$/, "");
  const slug = withoutGitSuffix.split(/[/:]/).filter((part) => part !== "").pop();
  return slug ?? "";
}

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
  isMerging,
  isRebasing,
  operationDisabled,
  operationDisabledReason,
  graphBranchSelection,
  onSetGraphBranchSelection,
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
  isMerging: boolean;
  // Every ref-mutating action here is disabled while a rebase is paused mid-flight: the rebase
  // runs on a detached HEAD and `git-core::rebase`'s `finish` moves the *original* branch ref at
  // the end, so switching/renaming/deleting branches underneath it silently retargets an
  // unrelated branch. `git-core::rebase::rebase_continue` also refuses outright once HEAD has
  // drifted — this just stops the user from getting there.
  isRebasing: boolean;
  operationDisabled: boolean;
  // Human-readable reason `operationDisabled` is true (e.g. "A rebase is in progress."), shown
  // as a `title` on the buttons it disables so they don't just go inert with no explanation
  // (issue #31/UX-003). `null` when nothing is blocking.
  operationDisabledReason: string | null;
  // `null` means no filter is saved — every branch shows in CommitGraph (see `graph_log` in
  // `git-core` and `useAppState`'s `AppState.graphBranchSelection`).
  graphBranchSelection: string[] | null;
  onSetGraphBranchSelection: (selectedBranches: string[]) => void;
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
}) {
  const [checkoutAfterCreate, setCheckoutAfterCreate] = useState(false);
  const [newBranchName, setNewBranchName] = useState("");
  const [createError, setCreateError] = useState<string | null>(null);
  const [pendingForceFor, setPendingForceFor] = useState<string | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [rowMenu, setRowMenu] = useState<RowContextMenu | null>(null);
  const [localOpen, setLocalOpen] = useState(() => loadPersistedOpen(LOCAL_FOLDER_KEY, true));
  // Selection for a single row in the local/remote branch tree — a plain visual highlight, not a
  // listbox/roving-tabindex pattern: see ListRow.tsx's doc comment for why that heavier pattern
  // is reserved for containers that also own keyboard navigation, which this tree doesn't (yet).
  // Keyed "local:<branch>" or "remote:<remoteName>:<branchName>" so the two namespaces can't collide.
  const [selectedRow, setSelectedRow] = useState<string | null>(null);
  // Open/closed state for the "/"-split subfolders inside the Local tree and inside each remote's
  // tree — distinct from `localOpen` (the top-level "Local" folder) and `openRemotes` (the
  // top-level remote folders, which also trigger the lazy branch-list fetch on first expand).
  const [openSubfolders, setOpenSubfolders] = useState<Record<string, boolean>>({});

  const [openRemotes, setOpenRemotes] = useState<Record<string, boolean>>({});
  const [remoteBranches, setRemoteBranches] = useState<Record<string, string[]>>({});
  const [removeConfirmation, setRemoveConfirmation] = useState<string | null>(null);
  const [editingRemote, setEditingRemote] = useState<RemoteInfo | null>(null);
  const [editName, setEditName] = useState("");
  const [editFetchUrl, setEditFetchUrl] = useState("");
  const [editPushUrl, setEditPushUrl] = useState("");
  const [credentialRemote, setCredentialRemote] = useState<string | null>(null);
  const [credentialMode, setCredentialMode] = useState<RemoteAuthMode>("HttpsToken");
  const [credentialUsername, setCredentialUsername] = useState("");
  const accessTokenRef = useRef<HTMLInputElement>(null);
  const [upstreamDialogOpen, setUpstreamDialogOpen] = useState(false);
  const [clearUpstreamConfirm, setClearUpstreamConfirm] = useState(false);
  const [upstreamRemoteField, setUpstreamRemoteField] = useState("");
  const [upstreamBranchField, setUpstreamBranchField] = useState("");
  const [remoteBranchOptions, setRemoteBranchOptions] = useState<string[]>([]);
  const [newRemoteName, setNewRemoteName] = useState("");
  const [newFetchUrl, setNewFetchUrl] = useState("");
  const [newPushUrl, setNewPushUrl] = useState("");
  const [nameTouched, setNameTouched] = useState(false);
  const [showPushUrl, setShowPushUrl] = useState(false);
  const [addError, setAddError] = useState<string | null>(null);
  const [checkoutError, setCheckoutError] = useState<string | null>(null);
  const [remoteBranchesError, setRemoteBranchesError] = useState<string | null>(null);

  function remoteFolderKey(remoteName: string): string {
    return `branchtree.remote.${remoteName}`;
  }

  function isRemoteOpen(remoteName: string): boolean {
    return openRemotes[remoteName] ?? loadPersistedOpen(remoteFolderKey(remoteName), false);
  }

  function isSubfolderOpen(folderPath: string): boolean {
    return openSubfolders[folderPath] ?? loadPersistedOpen(`branchtree.folder.${folderPath}`, true);
  }

  function toggleSubfolder(folderPath: string): void {
    const next = !isSubfolderOpen(folderPath);
    setOpenSubfolders((prev) => ({ ...prev, [folderPath]: next }));
    persistOpen(`branchtree.folder.${folderPath}`, next);
  }

  function renderLocalNodes(nodes: BranchTreeNode<BranchInfo>[]): ReactElement[] {
    return nodes.map((node) => {
      if (node.kind === "folder") {
        const folderPath = `local/${node.path}`;
        const open = isSubfolderOpen(folderPath);
        return (
          <li key={node.path} className={styles.folder}>
            <button
              type="button"
              className={styles.folderHeader}
              aria-expanded={open}
              onClick={() => toggleSubfolder(folderPath)}
            >
              <ChevronRight
                size={14}
                aria-hidden="true"
                className={open ? `${styles.chevron} ${styles.chevronOpen}` : styles.chevron}
              />
              {node.name}
            </button>
            {open && <ul className={styles.folderBody}>{renderLocalNodes(node.children)}</ul>}
          </li>
        );
      }

      const branch = node.value;
      const rowKey = `local:${branch.name}`;
      const shownInGraph = (graphBranchSelection ?? branches.map((b) => b.name)).includes(branch.name);
      const isRenaming = renaming === branch.name;
      return (
        <ListRow
          key={branch.name}
          className={selectedRow === rowKey ? `${styles.treeRow} ${styles.selectedRow}` : styles.treeRow}
          onClick={isRenaming ? undefined : () => setSelectedRow(rowKey)}
          onDoubleClick={
            isRenaming
              ? undefined
              : () => {
                  if (!isRebasing) onSwitchBranch(branch.name);
                }
          }
          onContextMenu={
            isRenaming
              ? undefined
              : (event) => {
                  event.preventDefault();
                  openRowMenu({ kind: "local-branch", name: branch.name, x: event.clientX, y: event.clientY });
                }
          }
        >
          <div className={styles.rowInner}>
            <button
              type="button"
              className={styles.swatch}
              aria-label={`Show ${branch.name} in graph`}
              aria-pressed={shownInGraph}
              style={{ "--swatch": branchSwatchColor(branch.name) } as CSSProperties}
              onClick={(event) => {
                event.stopPropagation();
                toggleGraphBranch(branch.name);
              }}
            />
            {isRenaming ? (
              <input
                autoFocus
                aria-label={`Rename ${branch.name}`}
                value={renameValue}
                onChange={(event) => setRenameValue(event.target.value)}
                onFocus={(event) => event.currentTarget.select()}
                onBlur={() => {
                  if (renameValue === branch.name) setRenaming(null);
                }}
                onKeyDown={(event) => handleRenameKeyDown(event, branch.name)}
              />
            ) : (
              <span
                className={branch.isCurrent ? `${styles.name} ${styles.currentName}` : styles.name}
                title={branch.name}
              >
                {branch.isCurrent && (
                  <span className={styles.currentMarker} role="img" aria-label="current branch" />
                )}
                {node.name}
                {branch.isCurrent && " (current)"}
              </span>
            )}
            {!isRenaming && (
              <button
                type="button"
                className={styles.iconButton}
                aria-label={`Actions for ${branch.name}`}
                aria-haspopup="menu"
                onClick={(event) => {
                  const rect = event.currentTarget.getBoundingClientRect();
                  openRowMenu({ kind: "local-branch", name: branch.name, x: rect.left, y: rect.bottom });
                }}
              >
                <MoreHorizontal size={12} aria-hidden="true" />
              </button>
            )}
          </div>
        </ListRow>
      );
    });
  }

  function renderRemoteNodes(remoteName: string, nodes: BranchTreeNode<string>[]): ReactElement[] {
    return nodes.map((node) => {
      if (node.kind === "folder") {
        const folderPath = `remote/${remoteName}/${node.path}`;
        const open = isSubfolderOpen(folderPath);
        return (
          <li key={node.path} className={styles.folder}>
            <button
              type="button"
              className={styles.folderHeader}
              aria-expanded={open}
              onClick={() => toggleSubfolder(folderPath)}
            >
              <ChevronRight
                size={14}
                aria-hidden="true"
                className={open ? `${styles.chevron} ${styles.chevronOpen}` : styles.chevron}
              />
              {node.name}
            </button>
            {open && <ul className={styles.folderBody}>{renderRemoteNodes(remoteName, node.children)}</ul>}
          </li>
        );
      }

      const branchName = node.value;
      const rowKey = `remote:${remoteName}:${branchName}`;
      return (
        <ListRow
          key={branchName}
          className={selectedRow === rowKey ? `${styles.treeRow} ${styles.selectedRow}` : styles.treeRow}
          onClick={() => setSelectedRow(rowKey)}
          onDoubleClick={() => {
            if (!operationDisabled) void checkoutRemoteBranch(remoteName, branchName);
          }}
          onContextMenu={(event) => {
            event.preventDefault();
            openRowMenu({ kind: "remote-branch", remoteName, branchName, x: event.clientX, y: event.clientY });
          }}
        >
          <div className={styles.rowInner}>
            <span className={styles.name} title={branchName}>
              {node.name}
            </span>
            {/* Explicit affordance for the actions `onContextMenu` above only reaches via a
                native contextmenu event (right-click, or Shift+F10/Menu-key once the branch
                name button has focus) — without this, a keyboard/screen-reader user has no
                visible indication those actions exist at all (AUD-2026-09-05-FE-001). */}
            <button
              type="button"
              className={styles.iconButton}
              aria-label={`Actions for ${branchName}`}
              aria-haspopup="menu"
              onClick={(event) => {
                const rect = event.currentTarget.getBoundingClientRect();
                openRowMenu({ kind: "remote-branch", remoteName, branchName, x: rect.left, y: rect.bottom });
              }}
            >
              <MoreHorizontal size={12} aria-hidden="true" />
            </button>
          </div>
        </ListRow>
      );
    });
  }

  // Shared by the initial lazy-expand fetch and by the Fetch context-menu item's cache
  // invalidation below, so both paths get the same error handling — matching the Set-upstream
  // dialog's `onListRemoteBranches(...).catch(() => setRemoteBranchOptions([]))` pattern instead
  // of leaving this call's promise unhandled.
  const fetchRemoteBranchList = (remoteName: string) => {
    void onListRemoteBranches(remoteName)
      .then((names) => {
        setRemoteBranches((prev) => ({ ...prev, [remoteName]: names }));
        setRemoteBranchesError(null);
      })
      .catch(() => setRemoteBranchesError(`Could not list branches for ${remoteName}.`));
  };

  const toggleRemote = (remoteName: string) => {
    const willOpen = !isRemoteOpen(remoteName);
    setOpenRemotes((prev) => ({ ...prev, [remoteName]: willOpen }));
    persistOpen(remoteFolderKey(remoteName), willOpen);
    if (willOpen && remoteBranches[remoteName] === undefined) {
      fetchRemoteBranchList(remoteName);
    }
  };

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

  const submitAddRemote = async () => {
    setAddError(null);
    const fetchUrl = newFetchUrl.trim();
    const name = (newRemoteName.trim() || deriveRemoteName(fetchUrl, remotes.map((r) => r.name))).trim();
    if (name === "" || fetchUrl === "") return;
    const failure = await onAddRemote(name, fetchUrl, newPushUrl.trim() || null);
    if (failure !== null) {
      setAddError(failure);
      return;
    }
    setNewRemoteName("");
    setNewFetchUrl("");
    setNewPushUrl("");
    setNameTouched(false);
    setShowPushUrl(false);
  };

  function remoteFolderItems(remote: RemoteInfo): ContextMenuItem[] {
    return [
      {
        label: "Fetch",
        disabled: operationDisabled,
        title: operationDisabled ? (operationDisabledReason ?? undefined) : undefined,
        // The remote's branch list is cached in `remoteBranches` and never otherwise
        // invalidated (folder open/closed state is localStorage-persisted, so even an app
        // restart mid-session doesn't clear it) — drop the cache entry after a successful fetch
        // so the tree reflects any new/removed remote branches, and re-fetch immediately if the
        // folder is currently open so the user sees the update without a manual re-toggle.
        onSelect: () =>
          void onFetchRemote(remote.name).then(() => {
            setRemoteBranches((prev) => {
              const next = { ...prev };
              delete next[remote.name];
              return next;
            });
            if (isRemoteOpen(remote.name)) fetchRemoteBranchList(remote.name);
          }),
      },
      {
        label: "Push current branch here",
        disabled: operationDisabled,
        title: operationDisabled ? (operationDisabledReason ?? undefined) : undefined,
        onSelect: () => void onPushCurrentBranch(remote.name),
      },
      {
        label: "Edit remote",
        onSelect: () => {
          setEditingRemote(remote);
          setEditName(remote.name);
          setEditFetchUrl(remote.fetchUrl);
          setEditPushUrl(remote.pushUrl ?? "");
        },
      },
      {
        label: "Manage credentials",
        onSelect: () => {
          setCredentialRemote(remote.name);
          setCredentialMode(remote.authMode ?? "HttpsToken");
          setCredentialUsername(remote.authUsername ?? "");
        },
      },
      {
        label: "Remove remote",
        destructive: true,
        disabled: operationDisabled,
        onSelect: () =>
          setRemoveConfirmation(
            (remoteUpstreams[remote.name]?.length ?? 0) > 0 ? `clear:${remote.name}` : remote.name,
          ),
      },
    ];
  }

  function remoteBranchItems(remoteName: string, branchName: string): ContextMenuItem[] {
    return [
      {
        label: "Checkout",
        disabled: operationDisabled,
        title: operationDisabled ? (operationDisabledReason ?? undefined) : undefined,
        onSelect: () => void checkoutRemoteBranch(remoteName, branchName),
      },
      {
        label: "Set as upstream for current branch",
        disabled: operationDisabled,
        title: operationDisabled ? (operationDisabledReason ?? undefined) : undefined,
        onSelect: () => void onSetUpstream(remoteName, branchName),
      },
    ];
  }

  // Opening a context menu starts a fresh interaction with whatever row triggered it, so any
  // pending rename on a *different* row is dropped rather than staying attached to it — the same
  // guard BranchSwitcher's popover close used to provide (see BranchTree.test.tsx's "right-clicking
  // a different branch after starting a rename on one clears that rename's input").
  const openRowMenu = (menu: RowContextMenu) => {
    setRenaming(null);
    setRenameValue("");
    setRowMenu(menu);
  };

  const newBranchProblem = branchNameProblem(newBranchName.trim());
  const startPoint = createBranchDraft?.startPoint ?? "HEAD";
  const currentBranchName = branches.find((branch) => branch.isCurrent)?.name;
  const createBaseLabel =
    startPoint === "HEAD" ? (currentBranchName ?? "HEAD") : /^[0-9a-f]{40}$/i.test(startPoint) ? startPoint.slice(0, 7) : startPoint;

  const submitCreate = async () => {
    if (newBranchName.trim() === "" || createBranchDraft === null) return;
    const failure = await onCreateBranch(newBranchName.trim(), createBranchDraft.startPoint);
    if (failure !== null) {
      setCreateError(failure);
      return;
    }
    setNewBranchName("");
    setCreateError(null);
    if (checkoutAfterCreate) await onSwitchBranch(newBranchName.trim());
  };

  const handleDeleteClick = async (name: string) => {
    await onDeleteBranch(name, false);
    setPendingForceFor(name);
  };

  const toggleGraphBranch = (name: string) => {
    const shown = graphBranchSelection ?? branches.map((b) => b.name);
    const next = shown.includes(name) ? shown.filter((n) => n !== name) : [...shown, name];
    onSetGraphBranchSelection(next);
  };

  const handleRenameKeyDown = (event: KeyboardEvent<HTMLInputElement>, oldName: string) => {
    if (event.key === "Enter") {
      if (renameValue.trim() === "") return;
      onRenameBranch(oldName, renameValue);
      setRenaming(null);
    } else if (event.key === "Escape") {
      event.stopPropagation();
      setRenaming(null);
    }
  };

  function branchContextItems(branch: BranchInfo): ContextMenuItem[] {
    const items: ContextMenuItem[] = [
      {
        label: "Checkout",
        title: branch.isCurrent ? "Already the current branch." : "Same as double-clicking the branch.",
        disabled: isRebasing || branch.isCurrent,
        onSelect: () => onSwitchBranch(branch.name),
      },
      {
        label: "Rename",
        disabled: isRebasing,
        onSelect: () => {
          setRenaming(branch.name);
          setRenameValue(branch.name);
        },
      },
    ];
    if (!branch.isCurrent) {
      items.push({
        label: "Merge into current branch",
        disabled: isMerging || isRebasing || operationDisabled,
        title: operationDisabled ? (operationDisabledReason ?? undefined) : undefined,
        onSelect: () => onMergeBranch(branch.name),
      });
    }
    if (branch.isCurrent) {
      if (upstream !== null) {
        items.push({
          label: `Push to ${upstream.remoteName}`,
          disabled: operationDisabled,
          title: operationDisabled ? (operationDisabledReason ?? undefined) : undefined,
          onSelect: () => void onPushCurrentBranch(upstream.remoteName),
        });
      }
      items.push({
        label: "Set upstream…",
        onSelect: () => {
          setUpstreamRemoteField("");
          setUpstreamBranchField("");
          setUpstreamDialogOpen(true);
        },
      });
    }
    items.push({
      label: "Isolate branch",
      title: "Show only this branch in the commit graph",
      onSelect: () => onSetGraphBranchSelection([branch.name]),
    });
    items.push({
      label: "Delete",
      disabled: isRebasing,
      destructive: true,
      onSelect: () => void handleDeleteClick(branch.name),
    });
    return items;
  }

  return (
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
          onClick={(event) => {
            const rect = event.currentTarget.getBoundingClientRect();
            openRowMenu({ kind: "add", x: rect.left, y: rect.bottom });
          }}
        >
          <Plus size={14} aria-hidden="true" />
        </button>
      }
    >
      {createBranchDraft !== null && (
        <form
          className={styles.draftForm}
          aria-label="New branch"
          onSubmit={(event) => {
            event.preventDefault();
            if (newBranchProblem === null) void submitCreate();
          }}
        >
          <p className={styles.draftBase}>
            New branch from <strong>{createBaseLabel}</strong>
          </p>
          <label htmlFor="new-branch-name" className={styles.draftLabel}>
            Branch name
          </label>
          <input
            id="new-branch-name"
            autoFocus
            value={newBranchName}
            aria-invalid={newBranchProblem !== null}
            aria-describedby={newBranchProblem !== null ? "new-branch-name-problem" : undefined}
            onChange={(event) => {
              setNewBranchName(event.target.value);
              setCreateError(null);
            }}
            placeholder="New branch name"
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                setCreateError(null);
                onCloseCreateBranchDraft();
              }
            }}
          />
          {newBranchProblem !== null && (
            <p id="new-branch-name-problem" className={styles.draftProblem}>
              {newBranchProblem}
            </p>
          )}
          <label className={styles.draftCheckbox}>
            <input
              type="checkbox"
              checked={checkoutAfterCreate}
              onChange={(event) => setCheckoutAfterCreate(event.target.checked)}
            />
            Check out after creating
          </label>
          <div className={styles.draftActions}>
            <button type="submit" disabled={newBranchName.trim() === "" || newBranchProblem !== null || isRebasing}>
              Create
            </button>
            <button
              type="button"
              onClick={() => {
                setCreateError(null);
                onCloseCreateBranchDraft();
              }}
            >
              Cancel
            </button>
          </div>
          {createError !== null && <InlineError message={createError} onDismiss={() => setCreateError(null)} />}
        </form>
      )}

      {addRemoteDraftOpen && (
        <form
          className={styles.form}
          aria-label="Add remote"
          onKeyDown={(event) => {
            if (event.key === "Escape") onCloseAddRemoteDraft();
          }}
          onSubmit={(event) => {
            event.preventDefault();
            void submitAddRemote();
          }}
        >
          <label className={styles.label}>
            Remote name
            <input
              autoFocus
              placeholder="origin"
              value={newRemoteName}
              onChange={(event) => {
                setNameTouched(true);
                setNewRemoteName(event.target.value);
              }}
            />
          </label>
          <label className={styles.label}>
            Fetch URL
            <input
              data-testid="add-remote-fetch-url"
              placeholder="git@github.com:user/repo.git"
              value={newFetchUrl}
              onChange={(event) => {
                const value = event.target.value;
                setNewFetchUrl(value);
                setAddError(null);
                if (!nameTouched) {
                  setNewRemoteName(deriveRemoteName(value, remotes.map((r) => r.name)));
                }
              }}
            />
          </label>
          {addError !== null && <InlineError message={addError} onDismiss={() => setAddError(null)} />}
          <details open={showPushUrl} onToggle={(event) => setShowPushUrl(event.currentTarget.open)}>
            <summary
              onClick={(event) => {
                event.preventDefault();
                setShowPushUrl((open) => !open);
              }}
            >
              Push URL (optional)
            </summary>
            {showPushUrl && (
              <label className={styles.label}>
                Push URL
                <input
                  placeholder="git@github.com:user/repo.git"
                  value={newPushUrl}
                  onChange={(event) => setNewPushUrl(event.target.value)}
                />
              </label>
            )}
          </details>
          <button
            type="submit"
            disabled={operationDisabled}
            title={operationDisabled ? (operationDisabledReason ?? undefined) : undefined}
          >
            Add remote
          </button>
          <button type="button" onClick={onCloseAddRemoteDraft}>
            Cancel
          </button>
        </form>
      )}

      <ul className={styles.tree}>
        <li className={styles.folder}>
          <button
            type="button"
            className={styles.folderHeader}
            aria-expanded={localOpen}
            onClick={() => {
              const next = !localOpen;
              setLocalOpen(next);
              persistOpen(LOCAL_FOLDER_KEY, next);
            }}
          >
            <ChevronRight
              size={14}
              aria-hidden="true"
              className={localOpen ? `${styles.chevron} ${styles.chevronOpen}` : styles.chevron}
            />
            Local
          </button>
          {localOpen && (
          <ul className={styles.folderBody}>
            {renderLocalNodes(buildBranchTree(branches.map((branch) => ({ path: branch.name, value: branch }))))}
          </ul>
          )}
        </li>

        {remotes.map((remote) => (
          <li key={remote.name} className={styles.folder}>
            <button
              type="button"
              className={styles.folderHeader}
              aria-expanded={isRemoteOpen(remote.name)}
              onClick={() => toggleRemote(remote.name)}
              onContextMenu={(event) => {
                event.preventDefault();
                openRowMenu({ kind: "remote-folder", name: remote.name, x: event.clientX, y: event.clientY });
              }}
            >
              <ChevronRight
                size={14}
                aria-hidden="true"
                className={isRemoteOpen(remote.name) ? `${styles.chevron} ${styles.chevronOpen}` : styles.chevron}
              />
              <Cloud size={14} aria-hidden="true" />
              {remote.name}
            </button>
            {isRemoteOpen(remote.name) && (
              <ul className={styles.folderBody}>
                {remoteBranches[remote.name] === undefined && remoteBranchesError === null && (
                  <li className={styles.statusRow}>Loading…</li>
                )}
                {remoteBranches[remote.name]?.length === 0 && <li className={styles.statusRow}>No branches</li>}
                {renderRemoteNodes(
                  remote.name,
                  buildBranchTree((remoteBranches[remote.name] ?? []).map((name) => ({ path: name, value: name }))),
                )}
              </ul>
            )}
          </li>
        ))}
      </ul>

      {checkoutError !== null && <InlineError message={checkoutError} onDismiss={() => setCheckoutError(null)} />}
      {remoteBranchesError !== null && (
        <InlineError message={remoteBranchesError} onDismiss={() => setRemoteBranchesError(null)} />
      )}

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
            <p>Force delete "{pendingForceFor}"? This discards any unmerged commits and cannot be undone.</p>
          }
          confirmLabel="Force delete"
          confirmDisabled={isRebasing}
          onConfirm={() => {
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
              x={rowMenu.x}
              y={rowMenu.y}
              onClose={() => setRowMenu(null)}
              items={branchContextItems(branch)}
            />
          ) : null;
        })()}
      {rowMenu !== null && rowMenu.kind === "add" && (
        <ContextMenu
          x={rowMenu.x}
          y={rowMenu.y}
          onClose={() => setRowMenu(null)}
          items={[
            { label: "New branch…", onSelect: () => onOpenCreateBranchDraft("HEAD") },
            { label: "Add remote…", onSelect: onOpenAddRemoteDraft },
            {
              label: "Show all branches",
              disabled: graphBranchSelection === null,
              onSelect: () => onSetGraphBranchSelection(branches.map((b) => b.name)),
            },
          ]}
        />
      )}
      {rowMenu !== null &&
        rowMenu.kind === "remote-folder" &&
        (() => {
          const remote = remotes.find((r) => r.name === rowMenu.name);
          return remote !== undefined ? (
            <ContextMenu
              x={rowMenu.x}
              y={rowMenu.y}
              onClose={() => setRowMenu(null)}
              items={remoteFolderItems(remote)}
            />
          ) : null;
        })()}
      {rowMenu !== null && rowMenu.kind === "remote-branch" && (
        <ContextMenu
          x={rowMenu.x}
          y={rowMenu.y}
          onClose={() => setRowMenu(null)}
          items={remoteBranchItems(rowMenu.remoteName, rowMenu.branchName)}
        />
      )}

      {removeConfirmation !== null && (
        <ConfirmDialog
          ariaLabel="Remove remote confirmation"
          message={
            removeConfirmation.startsWith("clear:") ? (
              <p>
                Remove {removeConfirmation.slice(6)} and clear upstreams for{" "}
                {remoteUpstreams[removeConfirmation.slice(6)].map((item) => item.localBranch).join(", ")}?
              </p>
            ) : (
              <p>Remove remote {removeConfirmation}?</p>
            )
          }
          confirmLabel="Confirm remove"
          confirmDisabled={operationDisabled}
          onConfirm={() => {
            const target = removeConfirmation.startsWith("clear:") ? removeConfirmation.slice(6) : removeConfirmation;
            const clearUpstreams = removeConfirmation.startsWith("clear:");
            void onRemoveRemote(target, clearUpstreams).then(() => setRemoveConfirmation(null));
          }}
          onCancel={() => setRemoveConfirmation(null)}
        />
      )}

      {editingRemote !== null && (
        <FormDialog ariaLabel={`Edit ${editingRemote.name}`} onCancel={() => setEditingRemote(null)}>
          <form
            onSubmit={async (event) => {
              event.preventDefault();
              const name = editName.trim();
              const fetchUrl = editFetchUrl.trim();
              if (name === "" || fetchUrl === "") return;
              if (name !== editingRemote.name && !(await onRenameRemote(editingRemote.name, name))) return;
              await onUpdateRemoteUrls(name, fetchUrl, editPushUrl.trim() || null);
              setEditingRemote(null);
            }}
          >
            <label className={styles.label}>
              Remote name
              <input value={editName} onChange={(event) => setEditName(event.target.value)} />
            </label>
            <label className={styles.label}>
              Fetch URL
              <input value={editFetchUrl} onChange={(event) => setEditFetchUrl(event.target.value)} />
            </label>
            <button
              type="button"
              className={styles.iconButton}
              aria-label={`Copy fetch URL for ${editingRemote.name}`}
              onClick={() => {
                void navigator.clipboard.writeText(editingRemote.fetchUrl);
              }}
            >
              <Copy size={12} aria-hidden="true" />
            </button>
            <label className={styles.label}>
              Push URL
              <input value={editPushUrl} onChange={(event) => setEditPushUrl(event.target.value)} />
            </label>
            {editingRemote.pushUrl !== null && (
              <button
                type="button"
                className={styles.iconButton}
                aria-label={`Copy push URL for ${editingRemote.name}`}
                onClick={() => {
                  void navigator.clipboard.writeText(editingRemote.pushUrl!);
                }}
              >
                <Copy size={12} aria-hidden="true" />
              </button>
            )}
            <button type="submit">Save remote</button>
            <button type="button" onClick={() => setEditingRemote(null)}>
              Cancel
            </button>
          </form>
        </FormDialog>
      )}

      {credentialRemote !== null && (
        <FormDialog ariaLabel={`Credentials for ${credentialRemote}`} onCancel={() => setCredentialRemote(null)}>
          <form
            onSubmit={async (event) => {
              event.preventDefault();
              const username = credentialUsername.trim();
              const token = accessTokenRef.current?.value ?? "";
              try {
                if (credentialMode === "SshAgent") {
                  await onSetRemoteAuthMode(credentialRemote, "SshAgent", null);
                  setCredentialRemote(null);
                } else if (username !== "" && token !== "") {
                  const configured = await onSetRemoteAuthMode(credentialRemote, "HttpsToken", username);
                  if (configured) {
                    await onSaveHttpsCredential(credentialRemote, username, token);
                    setCredentialRemote(null);
                  }
                }
              } catch {
                // The application state owns remediation messages for failed credential operations.
              } finally {
                if (accessTokenRef.current !== null) accessTokenRef.current.value = "";
              }
            }}
          >
            <label className={styles.label}>
              Authentication for {credentialRemote}
              <select value={credentialMode} onChange={(event) => setCredentialMode(event.target.value as RemoteAuthMode)}>
                <option value="HttpsToken">HTTPS token</option>
                <option value="SshAgent">SSH agent</option>
              </select>
            </label>
            {credentialMode === "HttpsToken" ? (
              <>
                <label className={styles.label}>
                  HTTPS username
                  <input value={credentialUsername} onChange={(event) => setCredentialUsername(event.target.value)} autoComplete="off" />
                </label>
                <label className={styles.label}>
                  Access token
                  <input ref={accessTokenRef} type="password" autoComplete="off" />
                </label>
                <button type="submit">Save HTTPS credential</button>
                <button type="button" onClick={() => void onForgetHttpsCredential(credentialRemote)}>
                  Forget HTTPS credential
                </button>
              </>
            ) : (
              <>
                <p className={styles.helperText}>
                  Uses your system's SSH agent to authenticate — make sure one is running (for
                  example via <code>ssh-add</code>) before fetching or pushing.
                </p>
                <button type="submit">Use SSH agent</button>
              </>
            )}
            <button type="button" onClick={() => setCredentialRemote(null)}>
              Cancel credentials
            </button>
          </form>
        </FormDialog>
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
            <button type="button" onClick={() => setClearUpstreamConfirm(true)}>
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
            <p>
              Stop {upstream.localBranch} tracking {upstream.remoteName}/{upstream.remoteBranch}?
            </p>
          }
          confirmLabel="Clear upstream"
          onConfirm={() => {
            setClearUpstreamConfirm(false);
            void onClearUpstream();
          }}
          onCancel={() => setClearUpstreamConfirm(false)}
        />
      )}

      {upstreamDialogOpen && (
        <FormDialog
          ariaLabel={`Set upstream for ${branches.find((b) => b.isCurrent)?.name ?? ""}`}
          onCancel={() => setUpstreamDialogOpen(false)}
        >
          <form
            onSubmit={async (event) => {
              event.preventDefault();
              const branch = upstreamBranchField.trim();
              if (upstreamRemoteField === "" || branch === "") return;
              await onSetUpstream(upstreamRemoteField, branch);
              setUpstreamDialogOpen(false);
            }}
          >
            <label className={styles.label}>
              Upstream remote
              <select
                value={upstreamRemoteField}
                onChange={(event) => {
                  const remoteName = event.target.value;
                  setUpstreamRemoteField(remoteName);
                  setRemoteBranchOptions([]);
                  if (remoteName !== "") {
                    void onListRemoteBranches(remoteName).then(setRemoteBranchOptions).catch(() => setRemoteBranchOptions([]));
                  }
                }}
              >
                <option value="">Choose a remote</option>
                {remotes.map((remote) => (
                  <option key={remote.name} value={remote.name}>
                    {remote.name}
                  </option>
                ))}
              </select>
            </label>
            <label className={styles.label}>
              Upstream branch
              <input
                list="upstream-branch-options"
                value={upstreamBranchField}
                onChange={(event) => setUpstreamBranchField(event.target.value)}
              />
            </label>
            <datalist id="upstream-branch-options">
              {remoteBranchOptions.map((name) => (
                <option key={name} value={name} />
              ))}
            </datalist>
            <button type="submit">Set upstream</button>
            <button type="button" onClick={() => setUpstreamDialogOpen(false)}>
              Cancel
            </button>
          </form>
        </FormDialog>
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
  );
}

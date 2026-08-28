import { useState, type KeyboardEvent } from "react";
import { ChevronRight, GitBranch, Plus } from "lucide-react";
import type {
  BranchInfo,
  PullOutcome,
  RemoteAuthMode,
  RemoteInfo,
  UpstreamInfo,
} from "../ipc/RepoClient";
import { loadPersistedOpen, persistOpen } from "../lib/persistedOpenState";
import { AccordionSection } from "./primitives/AccordionSection";
import { ConfirmDialog } from "./primitives/ConfirmDialog";
import { ContextMenu, type ContextMenuItem } from "./primitives/ContextMenu";
import { InlineError } from "./primitives/InlineError";
import { ListRow } from "./primitives/ListRow";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./BranchTree.module.css";

const LOCAL_FOLDER_KEY = "branchtree.local";

type RowContextMenu =
  | { kind: "local-branch"; name: string; x: number; y: number }
  | { kind: "add"; x: number; y: number };

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
  onOpenAddRemoteDraft,
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
  const [newBranchName, setNewBranchName] = useState("");
  const [createError, setCreateError] = useState<string | null>(null);
  const [pendingForceFor, setPendingForceFor] = useState<string | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [rowMenu, setRowMenu] = useState<RowContextMenu | null>(null);
  const [localOpen, setLocalOpen] = useState(() => loadPersistedOpen(LOCAL_FOLDER_KEY, true));

  // Opening a context menu starts a fresh interaction with whatever row triggered it, so any
  // pending rename on a *different* row is dropped rather than staying attached to it — the same
  // guard BranchSwitcher's popover close used to provide (see BranchTree.test.tsx's "right-clicking
  // a different branch after starting a rename on one clears that rename's input").
  const openRowMenu = (menu: RowContextMenu) => {
    setRenaming(null);
    setRenameValue("");
    setRowMenu(menu);
  };

  const submitCreate = async () => {
    if (newBranchName.trim() === "" || createBranchDraft === null) return;
    const failure = await onCreateBranch(newBranchName.trim(), createBranchDraft.startPoint);
    if (failure !== null) {
      setCreateError(failure);
      return;
    }
    setNewBranchName("");
    setCreateError(null);
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
    }
  };

  function branchContextItems(branch: BranchInfo): ContextMenuItem[] {
    const items: ContextMenuItem[] = [
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
    items.push({
      label: "Delete",
      disabled: isRebasing,
      destructive: true,
      onSelect: () => void handleDeleteClick(branch.name),
    });
    return items;
  }

  return (
    <AccordionSection title="Branches" storageKey="sidebar-branches" icon={GitBranch} count={branches.length} defaultOpen>
      <Toolbar aria-label="Branches actions">
        <button
          type="button"
          aria-label="Add"
          onClick={(event) => openRowMenu({ kind: "add", x: event.clientX, y: event.clientY })}
        >
          <Plus size={14} aria-hidden="true" />
        </button>
      </Toolbar>

      {createBranchDraft !== null && (
        <div className={styles.draftForm}>
          <input
            value={newBranchName}
            onChange={(event) => {
              setNewBranchName(event.target.value);
              setCreateError(null);
            }}
            placeholder="New branch name"
            onKeyDown={(event) => {
              if (event.key === "Enter") void submitCreate();
            }}
          />
          <button onClick={() => void submitCreate()} disabled={newBranchName.trim() === "" || isRebasing}>
            Create
          </button>
          <button
            onClick={() => {
              setCreateError(null);
              onCloseCreateBranchDraft();
            }}
          >
            Cancel
          </button>
          {createError !== null && <InlineError message={createError} onDismiss={() => setCreateError(null)} />}
        </div>
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
            {branches.map((branch) => (
              <ListRow key={branch.name}>
                <Toolbar>
                  <input
                    type="checkbox"
                    aria-label={`Show ${branch.name} in graph`}
                    checked={(graphBranchSelection ?? branches.map((b) => b.name)).includes(branch.name)}
                    onChange={() => toggleGraphBranch(branch.name)}
                  />
                  {renaming === branch.name ? (
                    <input
                      value={renameValue}
                      onChange={(event) => setRenameValue(event.target.value)}
                      onKeyDown={(event) => handleRenameKeyDown(event, branch.name)}
                    />
                  ) : (
                    <button
                      disabled={isRebasing}
                      onContextMenu={(event) => {
                        event.preventDefault();
                        openRowMenu({ kind: "local-branch", name: branch.name, x: event.clientX, y: event.clientY });
                      }}
                      onClick={() => {
                        if (!isRebasing) onSwitchBranch(branch.name);
                      }}
                    >
                      {branch.name}
                      {branch.isCurrent && " (current)"}
                    </button>
                  )}
                </Toolbar>
              </ListRow>
            ))}
          </ul>
          )}
        </li>
      </ul>

      {pendingForceFor !== null && (
        <ConfirmDialog
          ariaLabel={`Force delete ${pendingForceFor}`}
          message={
            <p>Force delete "{pendingForceFor}"? This discards any unmerged commits and cannot be undone.</p>
          }
          confirmLabel="Force Delete"
          confirmDisabled={isRebasing}
          onConfirm={() => {
            void onDeleteBranch(pendingForceFor, true);
            setPendingForceFor(null);
          }}
          onCancel={() => setPendingForceFor(null)}
        />
      )}

      {rowMenu !== null && rowMenu.kind === "local-branch" && (
        <ContextMenu
          x={rowMenu.x}
          y={rowMenu.y}
          onClose={() => setRowMenu(null)}
          items={branchContextItems(branches.find((b) => b.name === rowMenu.name)!)}
        />
      )}
      {rowMenu !== null && rowMenu.kind === "add" && (
        <ContextMenu
          x={rowMenu.x}
          y={rowMenu.y}
          onClose={() => setRowMenu(null)}
          items={[
            { label: "New Branch…", onSelect: () => onOpenCreateBranchDraft("HEAD") },
            { label: "Add Remote…", onSelect: onOpenAddRemoteDraft },
          ]}
        />
      )}
    </AccordionSection>
  );
}

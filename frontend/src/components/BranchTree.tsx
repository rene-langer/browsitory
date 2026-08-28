import { useEffect, useRef, useState, type KeyboardEvent, type RefObject } from "react";
import { ChevronRight, Cloud, Copy, GitBranch, Plus } from "lucide-react";
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
  const [newBranchName, setNewBranchName] = useState("");
  const [createError, setCreateError] = useState<string | null>(null);
  const [pendingForceFor, setPendingForceFor] = useState<string | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [rowMenu, setRowMenu] = useState<RowContextMenu | null>(null);
  const [localOpen, setLocalOpen] = useState(() => loadPersistedOpen(LOCAL_FOLDER_KEY, true));

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
  const editDialogRef = useRef<HTMLDialogElement>(null);
  const credentialDialogRef = useRef<HTMLDialogElement>(null);
  const upstreamFormDialogRef = useRef<HTMLDialogElement>(null);
  const [upstreamDialogOpen, setUpstreamDialogOpen] = useState(false);
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
  const pullDialogRef = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const dialog = pullDialogRef.current;
    if (pendingPull === null || dialog === null) return;
    if (!dialog.open && typeof dialog.showModal === "function") {
      dialog.showModal();
    } else if (!dialog.open) {
      dialog.setAttribute("open", "");
    }
    dialog.querySelector<HTMLButtonElement>("[data-autofocus]")?.focus();
  }, [pendingPull]);

  function openNativeDialog(ref: RefObject<HTMLDialogElement | null>): void {
    const dialog = ref.current;
    if (dialog === null || dialog.open) return;
    if (typeof dialog.showModal === "function") {
      dialog.showModal();
    } else {
      dialog.setAttribute("open", "");
    }
  }

  useEffect(() => {
    if (editingRemote !== null) openNativeDialog(editDialogRef);
  }, [editingRemote]);
  useEffect(() => {
    if (credentialRemote !== null) openNativeDialog(credentialDialogRef);
  }, [credentialRemote]);
  useEffect(() => {
    if (upstreamDialogOpen) openNativeDialog(upstreamFormDialogRef);
  }, [upstreamDialogOpen]);

  function remoteFolderKey(remoteName: string): string {
    return `branchtree.remote.${remoteName}`;
  }

  function isRemoteOpen(remoteName: string): boolean {
    return openRemotes[remoteName] ?? loadPersistedOpen(remoteFolderKey(remoteName), false);
  }

  const toggleRemote = (remoteName: string) => {
    const willOpen = !isRemoteOpen(remoteName);
    setOpenRemotes((prev) => ({ ...prev, [remoteName]: willOpen }));
    persistOpen(remoteFolderKey(remoteName), willOpen);
    if (willOpen && remoteBranches[remoteName] === undefined) {
      void onListRemoteBranches(remoteName).then((names) =>
        setRemoteBranches((prev) => ({ ...prev, [remoteName]: names })),
      );
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
        onSelect: () => void onFetchRemote(remote.name),
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

      {addRemoteDraftOpen && (
        <form
          className={styles.form}
          aria-label="Add remote"
          onSubmit={(event) => {
            event.preventDefault();
            void submitAddRemote();
          }}
        >
          <label className={styles.label}>
            Remote name
            <input
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
                {(remoteBranches[remote.name] ?? []).map((branchName) => (
                  <ListRow key={branchName}>
                    <span
                      onContextMenu={(event) => {
                        event.preventDefault();
                        openRowMenu({
                          kind: "remote-branch",
                          remoteName: remote.name,
                          branchName,
                          x: event.clientX,
                          y: event.clientY,
                        });
                      }}
                    >
                      {branchName}
                    </span>
                  </ListRow>
                ))}
              </ul>
            )}
          </li>
        ))}
      </ul>

      {checkoutError !== null && <InlineError message={checkoutError} onDismiss={() => setCheckoutError(null)} />}

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
      {rowMenu !== null && rowMenu.kind === "remote-folder" && (
        <ContextMenu
          x={rowMenu.x}
          y={rowMenu.y}
          onClose={() => setRowMenu(null)}
          items={remoteFolderItems(remotes.find((r) => r.name === rowMenu.name)!)}
        />
      )}
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
        <dialog
          ref={editDialogRef}
          aria-label={`Edit ${editingRemote.name}`}
          onCancel={(event) => {
            event.preventDefault();
            setEditingRemote(null);
          }}
        >
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
        </dialog>
      )}

      {credentialRemote !== null && (
        <dialog
          ref={credentialDialogRef}
          aria-label={`Credentials for ${credentialRemote}`}
          onCancel={(event) => {
            event.preventDefault();
            setCredentialRemote(null);
          }}
        >
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
        </dialog>
      )}

      <section>
        <h3>Upstream</h3>
        {upstream === null ? <p>No upstream for the current branch.</p> : <p>{upstream.localBranch} tracks {upstream.remoteName}/{upstream.remoteBranch}.</p>}
        <button
          type="button"
          disabled={operationDisabled || upstream === null || pendingPull !== null}
          title={operationDisabled ? (operationDisabledReason ?? undefined) : undefined}
          onClick={() => void onPull()}
        >
          Pull
        </button>
        {pullOutcome?.kind === "UpToDate" && <p role="status">Already up to date.</p>}
        {upstream !== null && (
          <button type="button" onClick={() => void onClearUpstream()}>
            Clear upstream
          </button>
        )}
      </section>

      {upstreamDialogOpen && (
        <dialog
          ref={upstreamFormDialogRef}
          aria-label={`Set upstream for ${branches.find((b) => b.isCurrent)?.name ?? ""}`}
          onCancel={(event) => {
            event.preventDefault();
            setUpstreamDialogOpen(false);
          }}
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
        </dialog>
      )}

      {pendingPull !== null && (
        <dialog
          ref={pullDialogRef}
          aria-label="Pull has diverged"
          onCancel={(event) => {
            event.preventDefault();
            onCancelPull();
          }}
        >
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
        </dialog>
      )}
    </AccordionSection>
  );
}

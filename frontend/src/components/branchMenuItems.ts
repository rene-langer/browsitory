import type { BranchInfo, UpstreamInfo } from "../ipc/RepoClient";
import { menuItem } from "../lib/actions/action";
import { branchActions, remoteBranchActions } from "../lib/actions/branchActions";
import { addRemoteAction, remoteActions } from "../lib/actions/remoteActions";
import type { OperationBlockers } from "../lib/operationStatus";
import type { ContextMenuItem } from "./primitives/ContextMenu";

/**
 * The context menus of the Branches section (`BranchTree`), like `tagMenuItems.ts` for tags. The
 * items' availability and labels come from the shared action rules (#509), like the palette's;
 * these functions only add the handlers and the items that have no shared rule.
 */

/**
 * The open row menu and what it was opened on. `returnFocusTo`: the element the menu was opened
 * from by the pointer, which is not necessarily focused (WebKit focuses neither a clicked button
 * nor a right-clicked row); without it `ContextMenu` returns focus to whatever had focus when it
 * opened (right for a key).
 */
export type RowContextMenu = { x: number; y: number; returnFocusTo?: HTMLElement } & (
  | { kind: "local-branch"; name: string }
  | { kind: "add" }
  | { kind: "remote-folder"; name: string }
  | { kind: "remote-branch"; remoteName: string; branchName: string }
);

/**
 * A menu's identity: the `key` that gives each target a fresh `ContextMenu` (so a menu re-targeted
 * while open never keeps the old one's focus state), and the `data-menu-key` of its button.
 */
export function rowMenuKey(menu: RowContextMenu): string {
  switch (menu.kind) {
    case "local-branch":
    case "remote-folder":
      return `${menu.kind}:${menu.name}`;
    case "remote-branch":
      return `${menu.kind}:${menu.remoteName}/${menu.branchName}`;
    case "add":
      return menu.kind;
  }
}

/** "Compare with current branch", or "Compare with HEAD" on a detached HEAD. Read-only, so never disabled. */
export function compareLabel(branches: BranchInfo[]): string {
  return branches.some((branch) => branch.isCurrent) ? "Compare with current branch" : "Compare with HEAD";
}

/** The menu of a local branch row. */
export function localBranchMenuItems({
  branch,
  branches,
  blockers,
  remoteCount,
  upstream,
  onSwitchBranch,
  onStartRename,
  onMergeBranch,
  onCompareBranch,
  onPushCurrentBranch,
  onOpenSetUpstream,
  onOpenPushDialog,
  onIsolateGraphBranch,
  onDelete,
}: {
  branch: BranchInfo;
  branches: BranchInfo[];
  blockers: OperationBlockers;
  remoteCount: number;
  upstream: UpstreamInfo | null;
  onSwitchBranch: (name: string) => void;
  onStartRename: (name: string) => void;
  onMergeBranch: (name: string) => void;
  onCompareBranch?: (refName: string, label: string) => void;
  onPushCurrentBranch: (remoteName: string) => Promise<void>;
  onOpenSetUpstream: () => void;
  onOpenPushDialog: (localBranch: string) => void;
  onIsolateGraphBranch: (kind: "local" | "remote", name: string) => void;
  /** A soft delete that fails asks to force delete. */
  onDelete: (name: string) => void;
}): ContextMenuItem[] {
  const actions = branchActions({ blockers, remoteCount, upstream }, branch);
  const items: ContextMenuItem[] = [
    {
      ...menuItem(actions.checkout, () => onSwitchBranch(branch.name)),
      title: actions.checkout.reason ?? "Same as double-clicking the branch.",
    },
    menuItem(actions.rename, () => onStartRename(branch.name)),
  ];
  if (!branch.isCurrent) {
    items.push(menuItem(actions.merge, () => onMergeBranch(branch.name)));
    if (onCompareBranch !== undefined) {
      items.push({
        label: compareLabel(branches),
        onSelect: () => onCompareBranch(`refs/heads/${branch.name}`, branch.name),
      });
    }
  }
  if (actions.pushToUpstream !== null && upstream !== null) {
    const remoteName = upstream.remoteName;
    items.push(menuItem(actions.pushToUpstream, () => void onPushCurrentBranch(remoteName)));
  }
  if (actions.setUpstream !== null) {
    items.push(menuItem(actions.setUpstream, onOpenSetUpstream));
  }
  items.push(menuItem(actions.pushTo, () => onOpenPushDialog(branch.name)));
  items.push({
    label: "Isolate branch",
    title: "Show only this branch in the commit graph",
    onSelect: () => onIsolateGraphBranch("local", branch.name),
  });
  items.push({ ...menuItem(actions.delete, () => onDelete(branch.name)), destructive: true });
  return items;
}

/** The menu of a remote folder header. */
export function remoteFolderMenuItems({
  remoteName,
  blockers,
  onFetch,
  onPushCurrentBranch,
  onEdit,
  onCredentials,
  onRemove,
}: {
  remoteName: string;
  blockers: OperationBlockers;
  onFetch: () => void;
  onPushCurrentBranch: () => void;
  onEdit: () => void;
  onCredentials: () => void;
  onRemove: () => void;
}): ContextMenuItem[] {
  const actions = remoteActions(blockers, remoteName);
  return [
    menuItem(actions.fetch, onFetch),
    menuItem(actions.pushCurrentBranch, onPushCurrentBranch),
    menuItem(actions.edit, onEdit),
    menuItem(actions.credentials, onCredentials),
    { ...menuItem(actions.remove, onRemove), destructive: true },
  ];
}

/** The menu of a remote-tracking branch row. */
export function remoteBranchMenuItems({
  remoteName,
  branchName,
  branches,
  blockers,
  onCheckout,
  onSetUpstream,
  onCompareBranch,
  onIsolateGraphBranch,
  onOpenDeleteRemoteBranch,
}: {
  remoteName: string;
  branchName: string;
  branches: BranchInfo[];
  blockers: OperationBlockers;
  onCheckout: () => void;
  onSetUpstream: (remoteName: string, remoteBranch: string) => Promise<void>;
  onCompareBranch?: (refName: string, label: string) => void;
  onIsolateGraphBranch: (kind: "local" | "remote", name: string) => void;
  onOpenDeleteRemoteBranch: (remoteName: string, branchName: string) => void;
}): ContextMenuItem[] {
  const actions = remoteBranchActions(blockers, remoteName, branchName);
  return [
    menuItem(actions.checkout, onCheckout),
    menuItem(actions.setAsUpstream, () => void onSetUpstream(remoteName, branchName)),
    ...(onCompareBranch !== undefined
      ? [
          {
            label: compareLabel(branches),
            onSelect: () => onCompareBranch(`refs/remotes/${remoteName}/${branchName}`, `${remoteName}/${branchName}`),
          },
        ]
      : []),
    {
      label: "Isolate branch",
      title: "Show only this branch in the commit graph",
      onSelect: () => onIsolateGraphBranch("remote", `${remoteName}/${branchName}`),
    },
    { ...menuItem(actions.delete, () => onOpenDeleteRemoteBranch(remoteName, branchName)), destructive: true },
  ];
}

/** The menu of the section header's "+" button. */
export function addMenuItems({
  hasGraphSelection,
  onOpenCreateBranchDraft,
  onOpenAddRemoteDraft,
  onShowAllGraphBranches,
}: {
  /** A local or a remote graph selection is saved: "Show all branches" has something to undo. */
  hasGraphSelection: boolean;
  onOpenCreateBranchDraft: (startPoint: string) => void;
  onOpenAddRemoteDraft: () => void;
  onShowAllGraphBranches: () => void;
}): ContextMenuItem[] {
  return [
    { label: "New branch…", onSelect: () => onOpenCreateBranchDraft("HEAD") },
    menuItem(addRemoteAction(), onOpenAddRemoteDraft),
    { label: "Show all branches", disabled: !hasGraphSelection, onSelect: onShowAllGraphBranches },
  ];
}

import type { BranchInfo, UpstreamInfo } from "../ipc/RepoClient";
import { menuItem } from "../lib/actions/action";
import { branchActions, rebaseOntoAction, remoteBranchActions } from "../lib/actions/branchActions";
import { addRemoteAction, remoteActions } from "../lib/actions/remoteActions";
import type { OperationBlockers } from "../lib/operationStatus";
import type { CopyToClipboard } from "../state/useCopyToClipboard";
import type { ContextMenuItem } from "./primitives/ContextMenu";

/**
 * The context menus of the Branches section (`BranchTree`), like `tagMenuItems.ts` for tags. The
 * items' availability and labels come from the shared action rules (#509), like the palette's;
 * these functions only add the handlers and the items that have no shared rule.
 *
 * The "Copy …" items (#614) sit last, before the destructive item, and are never disabled:
 * copying is not a Git operation, so the busy rule doesn't apply.
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

/** The current branch's name, `null` on a detached HEAD. */
function currentBranchName(branches: BranchInfo[]): string | null {
  return branches.find((branch) => branch.isCurrent)?.name ?? null;
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
  onRebaseOnto,
  onCompareBranch,
  onPushCurrentBranch,
  onOpenSetUpstream,
  onOpenPushDialog,
  onIsolateGraphBranch,
  onCopy,
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
  /** Opens the rebase planner onto `onto`, a full ref name (#619). */
  onRebaseOnto: (onto: string) => void;
  onCompareBranch?: (refName: string, label: string) => void;
  onPushCurrentBranch: (remoteName: string) => Promise<void>;
  onOpenSetUpstream: () => void;
  onOpenPushDialog: (localBranch: string) => void;
  onIsolateGraphBranch: (kind: "local" | "remote", name: string) => void;
  onCopy: CopyToClipboard;
  /** A soft delete that fails asks to force delete. */
  onDelete: (name: string) => void;
}): ContextMenuItem[] {
  const actions = branchActions({ blockers, remoteCount, upstream }, branch);
  const items: ContextMenuItem[] = [
    {
      ...menuItem(actions.checkout, () => onSwitchBranch(branch.name)),
      title: actions.checkout.reason ?? "Same as double-clicking the branch.",
      group: "branch",
    },
    { ...menuItem(actions.rename, () => onStartRename(branch.name)), group: "branch" },
  ];
  // Grouped (#645): the branch itself, merging, the remote, viewing and copying, then Delete.
  if (!branch.isCurrent) {
    items.push({ ...menuItem(actions.merge, () => onMergeBranch(branch.name)), group: "merge" });
    const rebase = rebaseOntoAction(blockers, currentBranchName(branches), { kind: "local", name: branch.name });
    if (rebase !== null) items.push({ ...menuItem(rebase, () => onRebaseOnto(rebase.onto)), group: "merge" });
    if (onCompareBranch !== undefined) {
      items.push({
        label: compareLabel(branches),
        onSelect: () => onCompareBranch(`refs/heads/${branch.name}`, branch.name),
        group: "merge",
      });
    }
  }
  if (actions.pushToUpstream !== null && upstream !== null) {
    const remoteName = upstream.remoteName;
    items.push({ ...menuItem(actions.pushToUpstream, () => void onPushCurrentBranch(remoteName)), group: "remote" });
  }
  if (actions.setUpstream !== null) {
    items.push({ ...menuItem(actions.setUpstream, onOpenSetUpstream), group: "remote" });
  }
  items.push({ ...menuItem(actions.pushTo, () => onOpenPushDialog(branch.name)), group: "remote" });
  items.push({
    label: "Isolate branch",
    title: "Show only this branch in the commit graph",
    onSelect: () => onIsolateGraphBranch("local", branch.name),
    group: "view",
  });
  items.push({ label: "Copy branch name", onSelect: () => onCopy(branch.name, "branch name"), group: "view" });
  items.push({ ...menuItem(actions.delete, () => onDelete(branch.name)), destructive: true, group: "delete" });
  return items;
}

/** The menu of a remote folder header. "Copy push URL" only when it differs from the fetch URL. */
export function remoteFolderMenuItems({
  remoteName,
  fetchUrl,
  pushUrl,
  blockers,
  onFetch,
  onPushCurrentBranch,
  onEdit,
  onCredentials,
  onCopy,
  onRemove,
}: {
  remoteName: string;
  fetchUrl: string;
  /** `null`: pushes go to the fetch URL. */
  pushUrl: string | null;
  blockers: OperationBlockers;
  onFetch: () => void;
  onPushCurrentBranch: () => void;
  onEdit: () => void;
  onCredentials: () => void;
  onCopy: CopyToClipboard;
  onRemove: () => void;
}): ContextMenuItem[] {
  const actions = remoteActions(blockers, remoteName);
  return [
    { ...menuItem(actions.fetch, onFetch), group: "sync" },
    { ...menuItem(actions.pushCurrentBranch, onPushCurrentBranch), group: "sync" },
    { ...menuItem(actions.edit, onEdit), group: "settings" },
    { ...menuItem(actions.credentials, onCredentials), group: "settings" },
    { label: "Copy fetch URL", onSelect: () => onCopy(fetchUrl, "fetch URL"), group: "copy" },
    ...(pushUrl !== null && pushUrl !== fetchUrl
      ? [{ label: "Copy push URL", onSelect: () => onCopy(pushUrl, "push URL"), group: "copy" }]
      : []),
    { ...menuItem(actions.remove, onRemove), destructive: true, group: "delete" },
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
  onRebaseOnto,
  onCompareBranch,
  onIsolateGraphBranch,
  onCopy,
  onOpenDeleteRemoteBranch,
}: {
  remoteName: string;
  branchName: string;
  branches: BranchInfo[];
  blockers: OperationBlockers;
  onCheckout: () => void;
  onSetUpstream: (remoteName: string, remoteBranch: string) => Promise<void>;
  /** Opens the rebase planner onto `onto`, a full ref name (#619). */
  onRebaseOnto: (onto: string) => void;
  onCompareBranch?: (refName: string, label: string) => void;
  onIsolateGraphBranch: (kind: "local" | "remote", name: string) => void;
  onCopy: CopyToClipboard;
  onOpenDeleteRemoteBranch: (remoteName: string, branchName: string) => void;
}): ContextMenuItem[] {
  const actions = remoteBranchActions(blockers, remoteName, branchName);
  const rebase = rebaseOntoAction(blockers, currentBranchName(branches), { kind: "remote", remoteName, branchName });
  return [
    { ...menuItem(actions.checkout, onCheckout), group: "branch" },
    { ...menuItem(actions.setAsUpstream, () => void onSetUpstream(remoteName, branchName)), group: "branch" },
    ...(rebase !== null ? [{ ...menuItem(rebase, () => onRebaseOnto(rebase.onto)), group: "merge" }] : []),
    ...(onCompareBranch !== undefined
      ? [
          {
            label: compareLabel(branches),
            onSelect: () => onCompareBranch(`refs/remotes/${remoteName}/${branchName}`, `${remoteName}/${branchName}`),
            group: "view",
          },
        ]
      : []),
    {
      label: "Isolate branch",
      title: "Show only this branch in the commit graph",
      onSelect: () => onIsolateGraphBranch("remote", `${remoteName}/${branchName}`),
      group: "view",
    },
    { label: "Copy branch name", onSelect: () => onCopy(`${remoteName}/${branchName}`, "branch name"), group: "view" },
    {
      ...menuItem(actions.delete, () => onOpenDeleteRemoteBranch(remoteName, branchName)),
      destructive: true,
      group: "delete",
    },
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
    { label: "New branch…", onSelect: () => onOpenCreateBranchDraft("HEAD"), group: "new" },
    { ...menuItem(addRemoteAction(), onOpenAddRemoteDraft), group: "new" },
    { label: "Show all branches", disabled: !hasGraphSelection, onSelect: onShowAllGraphBranches, group: "view" },
  ];
}

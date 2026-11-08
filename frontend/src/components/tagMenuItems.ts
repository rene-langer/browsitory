import type { RemoteInfo } from "../ipc/RepoClient";
import type { ContextMenuItem } from "./primitives/ContextMenu";

/**
 * The actions on one tag, shared by the tag badge menu in the graph (`CommitGraph`) and the
 * Tags panel's row menu (`TagPanel`), so both offer the same items in the same order. An action
 * whose handler is missing is left out; `commitId` `null` (a tag of a tree or blob) leaves out
 * the actions that need a commit.
 */
export function tagMenuItems({
  tag,
  commitId,
  remotes,
  disabled,
  disabledReason,
  onPushTag,
  onBranchFromCommit,
  onCheckoutCommit,
  onDeleteFromRemote,
  onDelete,
}: {
  tag: string;
  commitId: string | null;
  remotes: RemoteInfo[];
  /** A repository operation is running: the mutating items are disabled. */
  disabled: boolean;
  disabledReason?: string | null;
  onPushTag?: (remoteName: string, tag: string) => void;
  onBranchFromCommit?: (commitId: string) => void;
  onCheckoutCommit?: (commitId: string) => void;
  onDeleteFromRemote?: (tag: string) => void;
  /** Opens the local-delete confirmation. */
  onDelete?: (tag: string) => void;
}): ContextMenuItem[] {
  const busyTitle = disabled ? (disabledReason ?? undefined) : undefined;
  const noRemote = "No remote configured.";
  const items: ContextMenuItem[] = [];
  if (onPushTag !== undefined) {
    if (remotes.length === 0) {
      items.push({ label: "Push tag", onSelect: () => {}, disabled: true, title: noRemote });
    } else {
      for (const remote of remotes) {
        items.push({
          label: `Push ${tag} to ${remote.name}`,
          onSelect: () => onPushTag(remote.name, tag),
          disabled,
          title: busyTitle,
        });
      }
    }
  }
  if (commitId !== null && onBranchFromCommit !== undefined) {
    items.push({ label: "Branch from tag", onSelect: () => onBranchFromCommit(commitId) });
  }
  if (commitId !== null && onCheckoutCommit !== undefined) {
    items.push({ label: "Checkout (detached)", onSelect: () => onCheckoutCommit(commitId), disabled, title: busyTitle });
  }
  if (onDeleteFromRemote !== undefined) {
    items.push({
      label: "Delete from remote…",
      destructive: true,
      onSelect: () => onDeleteFromRemote(tag),
      disabled: disabled || remotes.length === 0,
      title: remotes.length === 0 ? noRemote : busyTitle,
    });
  }
  if (onDelete !== undefined) {
    items.push({ label: "Delete tag…", destructive: true, onSelect: () => onDelete(tag), disabled, title: busyTitle });
  }
  return items;
}

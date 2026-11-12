import type { RemoteInfo } from "../ipc/RepoClient";
import type { CopyToClipboard } from "../state/useCopyToClipboard";
import type { ContextMenuItem } from "./primitives/ContextMenu";
import { compareLabelFor } from "./branchMenuItems";

/**
 * The actions on one tag, shared by the tag badge menu in the graph (`CommitGraph`) and the
 * Tags panel's row menu (`TagPanel`), so both offer the same items in the same order. An action
 * whose handler is missing is left out; `commitId` `null` (a tag of a tree or blob) leaves out
 * the actions that need a commit. "Copy tag name" (#614) sits before the destructive items and is
 * never disabled (copying is not a Git operation), nor is "Compare with current branch" (#119),
 * which is read-only like the branch menus' compare.
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
  onCompareTag,
  currentBranch = null,
  onDeleteFromRemote,
  onCopy,
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
  /**
   * "Compare with current branch" (#119): `refs/tags/<name>` and the tag's name, the arguments of
   * the branch menus' compare (`useCompare`'s `compareBranch`). The backend peels an annotated
   * tag to its commit.
   */
  onCompareTag?: (refName: string, label: string) => void;
  /** The checked-out branch, `null` on a detached HEAD ("Compare with HEAD"). */
  currentBranch?: string | null;
  onDeleteFromRemote?: (tag: string) => void;
  onCopy?: CopyToClipboard;
  /** Opens the local-delete confirmation. */
  onDelete?: (tag: string) => void;
}): ContextMenuItem[] {
  const busyTitle = disabled ? (disabledReason ?? undefined) : undefined;
  const noRemote = "No remote configured.";
  const items: ContextMenuItem[] = [];
  if (onPushTag !== undefined) {
    if (remotes.length === 0) {
      items.push({ label: "Push tag", onSelect: () => {}, disabled: true, title: noRemote, group: "push" });
    } else {
      for (const remote of remotes) {
        items.push({
          label: `Push ${tag} to ${remote.name}`,
          onSelect: () => onPushTag(remote.name, tag),
          disabled,
          title: busyTitle,
          group: "push",
        });
      }
    }
  }
  if (commitId !== null && onBranchFromCommit !== undefined) {
    items.push({ label: "Branch from tag", onSelect: () => onBranchFromCommit(commitId), group: "commit" });
  }
  if (commitId !== null && onCheckoutCommit !== undefined) {
    items.push({
      label: "Switch to (detached)",
      onSelect: () => onCheckoutCommit(commitId),
      disabled,
      title: busyTitle,
      group: "commit",
    });
  }
  if (commitId !== null && onCompareTag !== undefined) {
    items.push({
      label: compareLabelFor(currentBranch),
      onSelect: () => onCompareTag(`refs/tags/${tag}`, tag),
      group: "commit",
    });
  }
  if (onCopy !== undefined) {
    items.push({ label: "Copy tag name", onSelect: () => onCopy(tag, "tag name"), group: "copy" });
  }
  if (onDeleteFromRemote !== undefined) {
    items.push({
      label: "Delete from remote…",
      destructive: true,
      onSelect: () => onDeleteFromRemote(tag),
      disabled: disabled || remotes.length === 0,
      title: remotes.length === 0 ? noRemote : busyTitle,
      group: "delete",
    });
  }
  if (onDelete !== undefined) {
    items.push({
      label: "Delete tag…",
      destructive: true,
      onSelect: () => onDelete(tag),
      disabled,
      title: busyTitle,
      group: "delete",
    });
  }
  return items;
}

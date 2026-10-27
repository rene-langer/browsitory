import type { MouseEvent } from "react";
import { Tag } from "lucide-react";
import type { GraphCommit } from "../ipc/RepoClient";
import graphStyles from "./CommitGraph.module.css";

/** Branch, remote-branch and tag badges for one commit row (graph, file history, search). */
export function RefBadges({
  commit,
  shownRemoteBranches = null,
  onTagContextMenu,
}: {
  commit: GraphCommit;
  /** `null`/omitted shows every remote badge; otherwise only the listed `<remote>/<branch>`es. */
  shownRemoteBranches?: ReadonlySet<string> | null;
  /** When set, right-clicking a tag opens its menu instead of the row's. */
  onTagContextMenu?: (tag: string, event: MouseEvent<HTMLSpanElement>) => void;
}) {
  const remoteRefs =
    shownRemoteBranches === null
      ? commit.remoteBranchRefs
      : commit.remoteBranchRefs.filter((ref) => shownRemoteBranches.has(ref));
  return (
    <>
      {commit.branchRefs.map((ref) => (
        <span key={`branch:${ref}`} className={graphStyles.branchBadge}>
          {ref}
        </span>
      ))}
      {remoteRefs.map((ref) => (
        <span key={`remote:${ref}`} className={graphStyles.remoteBranchBadge}>
          {ref}
        </span>
      ))}
      {(commit.tags ?? []).map((tag) => (
        <span
          key={`tag:${tag}`}
          className={graphStyles.tagBadge}
          title={`Tag ${tag}`}
          onContextMenu={
            onTagContextMenu === undefined
              ? undefined
              : (event) => {
                  // The row's own context menu (commit actions) must not open as well.
                  event.preventDefault();
                  event.stopPropagation();
                  onTagContextMenu(tag, event);
                }
          }
        >
          <Tag size={10} aria-hidden="true" />
          {tag}
        </span>
      ))}
    </>
  );
}

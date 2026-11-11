import type { CSSProperties, MouseEvent } from "react";
import { Tag } from "lucide-react";
import type { GraphCommit } from "../ipc/RepoClient";
import graphStyles from "./CommitGraph.module.css";

/** Branch, remote-branch and tag badges for one commit row (graph, file history, search). */
export function RefBadges({
  commit,
  shownRemoteBranches = null,
  detachedHead = false,
  laneColor,
  currentBranch = null,
  onTagContextMenu,
}: {
  commit: GraphCommit;
  /** This commit is the detached HEAD: a HEAD badge goes first (#467). */
  detachedHead?: boolean;
  /** `null`/omitted shows every remote badge; otherwise only the listed `<remote>/<branch>`es. */
  shownRemoteBranches?: ReadonlySet<string> | null;
  /**
   * The colour of the row's lane in the graph (#673): branch badges carry it as a stripe, the
   * colour of that branch's swatch in the sidebar. Omitted outside the graph.
   */
  laneColor?: string;
  /** The checked-out branch: its badge keeps the accent fill. */
  currentBranch?: string | null;
  /** When set, right-clicking a tag opens its menu instead of the row's. */
  onTagContextMenu?: (tag: string, event: MouseEvent<HTMLSpanElement>) => void;
}) {
  const remoteRefs =
    shownRemoteBranches === null
      ? commit.remoteBranchRefs
      : commit.remoteBranchRefs.filter((ref) => shownRemoteBranches.has(ref));
  const laneStyle = laneColor === undefined ? undefined : ({ "--lane": laneColor } as CSSProperties);
  const laneClass = laneColor === undefined ? "" : ` ${graphStyles.laneBadge}`;
  return (
    <>
      {detachedHead && (
        <span className={graphStyles.headBadge} title="Detached HEAD">
          HEAD
        </span>
      )}
      {commit.branchRefs.map((ref) => (
        <span
          key={`branch:${ref}`}
          className={`${ref === currentBranch || laneColor === undefined ? graphStyles.branchBadge : graphStyles.otherBranchBadge}${laneClass}`}
          style={laneStyle}
        >
          {ref}
        </span>
      ))}
      {remoteRefs.map((ref) => (
        <span key={`remote:${ref}`} className={`${graphStyles.remoteBranchBadge}${laneClass}`} style={laneStyle}>
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

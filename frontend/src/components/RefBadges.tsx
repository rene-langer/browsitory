import type { CSSProperties, MouseEvent, ReactNode } from "react";
import { Tag } from "lucide-react";
import type { GraphCommit } from "../ipc/RepoClient";
import graphStyles from "./CommitGraph.module.css";

/** Branch, remote-branch and tag badges for one commit row (graph, file history, search). */
/**
 * Branch, remote-branch and tag badges for one commit row (graph, file history, search). With
 * `collapse` (the graph, #1004) a row shows its first badge (the detached HEAD, else the current
 * branch, else in order) and "+N" for the rest, named in its tooltip, so the subject keeps room.
 */
export function RefBadges({
  commit,
  shownRemoteBranches = null,
  detachedHead = false,
  laneColor,
  currentBranch = null,
  onTagContextMenu,
  collapse = false,
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
  /** Show the first badge and "+N" for the others (#1004). */
  collapse?: boolean;
}) {
  const remoteRefs =
    shownRemoteBranches === null
      ? commit.remoteBranchRefs
      : commit.remoteBranchRefs.filter((ref) => shownRemoteBranches.has(ref));
  const laneStyle = laneColor === undefined ? undefined : ({ "--lane": laneColor } as CSSProperties);
  const laneClass = laneColor === undefined ? "" : ` ${graphStyles.laneBadge}`;
  // The current branch leads, so a collapsed row still says "you are here".
  const branches =
    currentBranch !== null && commit.branchRefs.includes(currentBranch)
      ? [currentBranch, ...commit.branchRefs.filter((ref) => ref !== currentBranch)]
      : commit.branchRefs;
  const badges: { key: string; name: string; node: ReactNode }[] = [
    ...(detachedHead
      ? [
          {
            key: "head",
            name: "HEAD",
            node: (
              <span key="head" className={graphStyles.headBadge} title="Detached HEAD">
                HEAD
              </span>
            ),
          },
        ]
      : []),
    ...branches.map((ref) => ({
      key: `branch:${ref}`,
      name: ref,
      node: (
        <span
          key={`branch:${ref}`}
          className={`${ref === currentBranch || laneColor === undefined ? graphStyles.branchBadge : graphStyles.otherBranchBadge}${laneClass}`}
          style={laneStyle}
        >
          {ref}
        </span>
      ),
    })),
    ...remoteRefs.map((ref) => ({
      key: `remote:${ref}`,
      name: ref,
      node: (
        <span key={`remote:${ref}`} className={`${graphStyles.remoteBranchBadge}${laneClass}`} style={laneStyle}>
          {ref}
        </span>
      ),
    })),
    ...(commit.tags ?? []).map((tag) => ({
      key: `tag:${tag}`,
      name: tag,
      node: (
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
      ),
    })),
  ];
  if (!collapse || badges.length <= 1) return <>{badges.map((badge) => badge.node)}</>;
  const rest = badges.slice(1).map((badge) => badge.name).join(", ");
  return (
    <>
      {badges[0].node}
      <span className={graphStyles.moreBadges} title={rest} aria-hidden="true">
        +{badges.length - 1}
      </span>
      <span className={graphStyles.srOnly}>, also {rest}</span>
    </>
  );
}

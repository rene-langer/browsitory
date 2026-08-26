import type { GraphCommit } from "../ipc/RepoClient";

export interface CommitLayout {
  commitId: string;
  lane: number;
  parentConnections: { parentId: string; lane: number }[];
  passThroughLanes: number[];
}

// `commits` must be in reverse-topological order (children before parents), as `graph_log`'s
// `Sort::TOPOLOGICAL | Sort::TIME` returns; any other order silently produces wrong lanes.
export function assignLanes(commits: GraphCommit[]): CommitLayout[] {
  // `lanes[i]` holds the commit id lane `i` is currently waiting to display next, or `null` if
  // that lane is free and its slot can be reused by an unrelated later commit.
  const lanes: (string | null)[] = [];
  const layouts: CommitLayout[] = [];

  const claimLane = (id: string): number => {
    const existing = lanes.indexOf(id);
    if (existing !== -1) {
      return existing;
    }
    const free = lanes.indexOf(null);
    if (free !== -1) {
      lanes[free] = id;
      return free;
    }
    lanes.push(id);
    return lanes.length - 1;
  };

  for (const commit of commits) {
    const passThroughLanes = lanes
      .map((id, i) => (id !== null ? i : -1))
      .filter((i) => i !== -1);

    const lane = claimLane(commit.id);

    const parentConnections: { parentId: string; lane: number }[] = [];
    let laneStillNeeded = false;

    commit.parentIds.forEach((parentId, parentIndex) => {
      const alreadyWaiting = lanes.indexOf(parentId) !== -1;
      if (parentIndex === 0 && !alreadyWaiting) {
        // Straight continuation: this commit's own lane keeps going, now waiting for its
        // first parent — the common case for most rows in a mostly-linear history.
        lanes[lane] = parentId;
        laneStillNeeded = true;
        parentConnections.push({ parentId, lane });
      } else {
        // Either a later (merge) parent, or a first parent some other lane is already
        // waiting for (this commit is where two lanes converge) — either way, this
        // connects to a lane other than the commit's own.
        const parentLane = claimLane(parentId);
        parentConnections.push({ parentId, lane: parentLane });
      }
    });

    // A root commit, or a commit whose first parent turned out to already be tracked
    // elsewhere, has nothing left for its own lane to continue waiting for — free it so a
    // later, unrelated commit can reuse the slot instead of lanes growing forever.
    if (!laneStillNeeded) {
      lanes[lane] = null;
    }

    layouts.push({ commitId: commit.id, lane, parentConnections, passThroughLanes });
  }

  return layouts;
}

// A squash group can only be a straight, unbranched chain: each commit in the range (other than
// the oldest) must be the sole parent of the one above it. That single check also rejects both
// ways a range can be non-linear — a merge commit inside it (more than one parent) and a fork
// point inside it (a commit whose only-in-range child isn't the one it's actually the parent
// of) — since either case breaks the parentIds[0]-equals-next.id chain somewhere in the range.
export function isSquashableRange(
  commits: GraphCommit[],
  startIndex: number,
  endIndex: number,
): boolean {
  for (let i = startIndex; i < endIndex; i++) {
    if (commits[i].parentIds.length !== 1) {
      return false;
    }
    if (commits[i].parentIds[0] !== commits[i + 1].id) {
      return false;
    }
  }
  return true;
}

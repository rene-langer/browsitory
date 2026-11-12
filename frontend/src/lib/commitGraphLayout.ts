import type { GraphCommit } from "../ipc/RepoClient";

const NO_COMMITS: ReadonlySet<string> = new Set();

export interface CommitLayout {
  commitId: string;
  lane: number;
  // Identifies which unrelated occupancy of `lane` this row belongs to — lane *numbers* get
  // freed and reused by later, unrelated branches, but a segment id never repeats, so hover
  // highlighting can key off it instead of the reused numeric lane.
  laneSegmentId: number;
  parentConnections: { parentId: string; lane: number; segmentId: number }[];
  passThroughLanes: { lane: number; segmentId: number }[];
  // Merge parents (never the first parent, except of a commit an expanded merge listed, see
  // `assignLanes`) that aren't in the list and that no lane is waiting for: the side branch of a
  // merge on an isolated branch or a collapsed merge (a first-parent walk leaves side branches
  // out, #380, #617), or one past the loaded page. They get no lane, which would otherwise run
  // down past every later row to a commit that never shows; the row draws a short stub instead.
  hiddenParentIds: string[];
}

// `commits` must be in reverse-topological order (children before parents), as `graph_log`'s
// `Sort::TOPOLOGICAL | Sort::TIME` returns; any other order silently produces wrong lanes.
//
// `merged`: the commits an expanded merge spliced in below its row (#617). Their first parent is
// usually a row of the first-parent line further down, where their lane ends; one that is not in
// the list (a fork from another merge's side branch, still collapsed) gets a stub like a hidden
// merge parent, instead of a lane running past every later row.
export function assignLanes(commits: GraphCommit[], merged: ReadonlySet<string> = NO_COMMITS): CommitLayout[] {
  return layoutLanes(commits, merged, null).layouts;
}

/** A laid-out commit list (#837): what `layoutLanes` returns, and continues from. */
export interface LaneLayout {
  /** One per commit, in order. */
  layouts: CommitLayout[];
  /** How many lanes the layouts draw (`graphLaneCount`), kept as the rows are laid out. */
  laneCount: number;
  /** Each commit's row index. */
  indexById: RowIndex;
  /** How many rows this call laid out: the appended page, or more when it had to go back. */
  rowsLaidOut: number;
  /** The walk's state, to continue from; see `layoutLanes`. */
  readonly continuation: Continuation;
}

/** A commit id's row in a laid-out list. */
export interface RowIndex {
  get(id: string): number | undefined;
  has(id: string): boolean;
  /** How many rows the list has. */
  readonly size: number;
}

/**
 * The rows of the first `size` commits of a list, over one map that later, longer lists add to
 * (#837): no copy per page, and an earlier list's index still answers for its own rows only.
 */
class PrefixRowIndex implements RowIndex {
  private readonly rows: Map<string, number>;
  readonly size: number;

  constructor(rows: Map<string, number>, size: number) {
    this.rows = rows;
    this.size = size;
  }

  get(id: string): number | undefined {
    const row = this.rows.get(id);
    return row !== undefined && row < this.size ? row : undefined;
  }

  has(id: string): boolean {
    return this.get(id) !== undefined;
  }
}

/** Rows between snapshots of the walk's state, from which a layout can go back. */
const CHECKPOINT_INTERVAL = 256;

type PassThrough = CommitLayout["passThroughLanes"];

/** The state `assignLanes` walks the rows with. */
interface WalkState {
  // `lanes[i]` holds the commit id lane `i` is currently waiting to display next, or `null` if
  // that lane is free and its slot can be reused by an unrelated later commit.
  lanes: (string | null)[];
  // `laneSegments[i]` identifies the current unbroken occupancy of `lanes[i]`; it only changes
  // when the slot is claimed anew (fresh push or reuse of a freed slot), never while an existing
  // chain keeps waiting on the same lane.
  laneSegments: number[];
  nextSegmentId: number;
  // The lanes passing through the next row. Rows share one list while the lanes don't change
  // (#837): a long history kept one object per row and lane otherwise.
  passThrough: PassThrough;
  passThroughStale: boolean;
  /** The highest lane any row so far draws. */
  highestLane: number;
}

interface Checkpoint extends WalkState {
  /** The row this is the state before. */
  row: number;
}

interface Continuation {
  /** The list laid out (the state's array: never changed after). */
  commits: readonly GraphCommit[];
  merged: ReadonlySet<string>;
  state: WalkState;
  checkpoints: Checkpoint[];
  /** Each hidden parent's first row that drew it as a stub. */
  stubRows: Map<string, number>;
  /** Every commit's row, of this list and of the longer ones laid out from it. */
  rows: Map<string, number>;
  /** One pass-through entry per lane segment, shared by every row it passes. */
  entries: Map<number, PassThrough[number]>;
  /** Set once a layout continued from this one: its state has moved on since. */
  used: boolean;
}

function copyState(state: WalkState): WalkState {
  return { ...state, lanes: [...state.lanes], laneSegments: [...state.laneSegments] };
}

/** Whether `list` starts with `prefix`: the same commit objects (a page appended), or ids. */
function isPrefix(prefix: readonly GraphCommit[], list: readonly GraphCommit[]): boolean {
  for (let index = 0; index < prefix.length; index++) {
    if (prefix[index] !== list[index] && prefix[index].id !== list[index].id) return false;
  }
  return true;
}

function sameSet(a: ReadonlySet<string>, b: ReadonlySet<string>): boolean {
  if (a.size !== b.size) return false;
  for (const value of a) if (!b.has(value)) return false;
  return true;
}

/**
 * The lanes of `commits` (see `assignLanes`), continuing from `previous` when `commits` extends
 * the list `previous` laid out (#837): infinite scroll appends a page at a time, and laying out
 * every loaded row again for each page cost O(pages²).
 *
 * Only `hiddenParentIds` looks at the whole list: a merge parent that isn't listed is a stub. A
 * page that brings such a parent changes the rows from the first one that stubbed it, so the walk
 * goes back to the last checkpoint before that row and lays out from there; otherwise it lays out
 * just the new rows. The result equals a full layout of `commits`.
 *
 * `previous` can be continued from once; a second continuation (React running a memo twice in
 * development) lays out from scratch.
 */
export function layoutLanes(
  commits: GraphCommit[],
  merged: ReadonlySet<string> = NO_COMMITS,
  previous: LaneLayout | null = null,
): LaneLayout {
  const before = previous?.continuation;
  const extends_ =
    before !== undefined &&
    !before.used &&
    commits.length >= before.commits.length &&
    sameSet(before.merged, merged) &&
    isPrefix(before.commits, commits);

  let continuation: Continuation;
  let rows: Map<string, number>;
  let firstRow: number;
  let layouts: CommitLayout[];
  if (extends_ && previous !== null && before !== undefined) {
    before.used = true;
    // Shared with the earlier results: their commits keep their rows.
    rows = before.rows;
    for (let row = before.commits.length; row < commits.length; row++) rows.set(commits[row].id, row);
    // The earliest row that drew a stub for a parent the new rows list.
    let goBackTo = before.commits.length;
    for (let row = before.commits.length; row < commits.length; row++) {
      const stubbed = before.stubRows.get(commits[row].id);
      if (stubbed !== undefined && stubbed < goBackTo) goBackTo = stubbed;
    }
    let state = before.state;
    let checkpoints = before.checkpoints;
    let stubRows = before.stubRows;
    firstRow = before.commits.length;
    if (goBackTo < before.commits.length) {
      let index = checkpoints.length - 1;
      while (checkpoints[index].row > goBackTo) index--;
      const checkpoint = checkpoints[index];
      firstRow = checkpoint.row;
      state = copyState(checkpoint);
      checkpoints = checkpoints.slice(0, index + 1);
      stubRows = new Map([...stubRows].filter(([, row]) => row < firstRow));
      // Segment ids from the checkpoint on are handed out again, possibly to other lanes.
      for (const segmentId of before.entries.keys()) if (segmentId >= checkpoint.nextSegmentId) before.entries.delete(segmentId);
    }
    continuation = {
      commits,
      merged,
      state,
      checkpoints,
      stubRows,
      entries: before.entries,
      rows,
      used: false,
    };
    layouts = previous.layouts.slice(0, firstRow);
  } else {
    rows = new Map(commits.map((commit, row) => [commit.id, row]));
    firstRow = 0;
    layouts = [];
    continuation = {
      commits,
      merged,
      state: {
        lanes: [],
        laneSegments: [],
        nextSegmentId: 0,
        passThrough: [],
        passThroughStale: false,
        highestLane: 0,
      },
      checkpoints: [],
      stubRows: new Map(),
      entries: new Map(),
      rows,
      used: false,
    };
  }

  walk(commits, firstRow, merged, rows, layouts, continuation);
  return {
    layouts,
    laneCount: continuation.state.highestLane + 1,
    indexById: new PrefixRowIndex(rows, commits.length),
    rowsLaidOut: commits.length - firstRow,
    continuation,
  };
}

/** Lays out `commits` from `firstRow` on, appending to `layouts` and moving `continuation` on. */
function walk(
  commits: GraphCommit[],
  firstRow: number,
  merged: ReadonlySet<string>,
  listed: ReadonlyMap<string, number>,
  layouts: CommitLayout[],
  continuation: Continuation,
): void {
  const state = continuation.state;
  const { lanes, laneSegments } = state;
  const { checkpoints, stubRows, entries } = continuation;

  const claimLane = (id: string): number => {
    const existing = lanes.indexOf(id);
    if (existing !== -1) {
      return existing;
    }
    state.passThroughStale = true;
    const free = lanes.indexOf(null);
    if (free !== -1) {
      lanes[free] = id;
      laneSegments[free] = state.nextSegmentId++;
      return free;
    }
    lanes.push(id);
    laneSegments.push(state.nextSegmentId++);
    return lanes.length - 1;
  };

  const entry = (lane: number, segmentId: number) => {
    let found = entries.get(segmentId);
    if (found === undefined) {
      found = { lane, segmentId };
      entries.set(segmentId, found);
    }
    return found;
  };

  for (let row = firstRow; row < commits.length; row++) {
    if (row % CHECKPOINT_INTERVAL === 0 && (checkpoints.length === 0 || checkpoints[checkpoints.length - 1].row < row)) {
      checkpoints.push({ ...copyState(state), row });
    }
    const commit = commits[row];
    if (state.passThroughStale) {
      const passThrough: PassThrough = [];
      lanes.forEach((id, i) => {
        if (id !== null) passThrough.push(entry(i, laneSegments[i]));
      });
      state.passThrough = passThrough;
      state.passThroughStale = false;
    }
    const passThroughLanes = state.passThrough;

    const lane = claimLane(commit.id);
    const laneSegmentId = laneSegments[lane];

    const parentConnections: { parentId: string; lane: number; segmentId: number }[] = [];
    const hiddenParentIds: string[] = [];
    let laneStillNeeded = false;

    const stubsFirstParent = merged.has(commit.id);
    commit.parentIds.forEach((parentId, parentIndex) => {
      const alreadyWaiting = lanes.indexOf(parentId) !== -1;
      if ((parentIndex > 0 || stubsFirstParent) && !alreadyWaiting && !listed.has(parentId)) {
        hiddenParentIds.push(parentId);
        if (!stubRows.has(parentId)) stubRows.set(parentId, row);
      } else if (parentIndex === 0 && !alreadyWaiting) {
        // Straight continuation: this commit's own lane keeps going, now waiting for its
        // first parent — the common case for most rows in a mostly-linear history.
        lanes[lane] = parentId;
        laneStillNeeded = true;
        parentConnections.push({ parentId, lane, segmentId: laneSegmentId });
      } else {
        // Either a later (merge) parent, or a first parent some other lane is already
        // waiting for (this commit is where two lanes converge) — either way, this
        // connects to a lane other than the commit's own.
        const parentLane = claimLane(parentId);
        parentConnections.push({ parentId, lane: parentLane, segmentId: laneSegments[parentLane] });
      }
    });

    // A root commit, or a commit whose first parent turned out to already be tracked
    // elsewhere, has nothing left for its own lane to continue waiting for — free it so a
    // later, unrelated commit can reuse the slot instead of lanes growing forever.
    if (!laneStillNeeded) {
      lanes[lane] = null;
      state.passThroughStale = true;
    }

    let highest = Math.max(state.highestLane, lane);
    for (const through of passThroughLanes) highest = Math.max(highest, through.lane);
    for (const connection of parentConnections) highest = Math.max(highest, connection.lane);
    state.highestLane = highest;

    layouts.push({
      commitId: commit.id,
      lane,
      laneSegmentId,
      parentConnections,
      passThroughLanes,
      hiddenParentIds,
    });
  }
}

// How many lanes `layouts` draw: one more than the highest lane any row uses. A loop, not
// `Math.max(...lanes)`: spreading every row into one call throws past the engine's argument
// limit, at about 30,000 rows of a narrow history (#836).
export function graphLaneCount(layouts: readonly CommitLayout[]): number {
  let highest = 0;
  for (const layout of layouts) {
    highest = Math.max(highest, layout.lane);
    for (const entry of layout.passThroughLanes) highest = Math.max(highest, entry.lane);
    for (const connection of layout.parentConnections) highest = Math.max(highest, connection.lane);
  }
  return highest + 1;
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


import { useEffect, useState } from "react";
import type {
  RebaseAction,
  RebasePlanCommit,
  RebasePlanEntry,
  RepoClient,
} from "../ipc/RepoClient";

type ActionKind = RebaseAction["kind"];

interface Row {
  commit: RebasePlanCommit;
  actionKind: ActionKind;
  rewordMessage: string;
  combinedMessage: string | null; // set only when this row is a group leader
}

function isGroupMember(kind: ActionKind): boolean {
  return kind === "Squash" || kind === "Fixup";
}

// The kind of the nearest row after `index` that isn't itself `Drop` — mirrors
// `git-core::rebase`'s `next_non_drop_action`. A `Drop` row never lands a commit, so it can't
// hide a squash/fixup group from the row before it, and it can't be a group's leader either
// (see `recomputeGroupLeaders` below); both this and the backend's `land_current_step` have to
// agree on where a group starts/who leads it, or the combined-message box in the UI ends up
// attached to a different row than the one the backend actually uses.
function nextNonDropActionKind(rows: Row[], index: number): ActionKind | undefined {
  for (let i = index + 1; i < rows.length; i++) {
    if (rows[i].actionKind !== "Drop") {
      return rows[i].actionKind;
    }
  }
  return undefined;
}

function defaultCombinedMessage(rows: Row[], leaderIndex: number): string {
  const parts = [rows[leaderIndex].commit.summary];
  for (let i = leaderIndex + 1; i < rows.length; i++) {
    const kind = rows[i].actionKind;
    if (kind === "Drop") {
      continue;
    }
    if (!isGroupMember(kind)) {
      break;
    }
    if (kind === "Squash") {
      parts.push(rows[i].commit.summary);
    }
  }
  return parts.join("\n\n");
}

function isLeader(rows: Row[], index: number): boolean {
  // A group leader must be neither a group member itself nor a Drop (a Drop never lands, so it
  // can't be the commit the group gets squashed onto — see `land_current_step`'s backward walk,
  // which excludes Drop from leader candidates the same way), and must be followed by at least
  // one group member.
  if (isGroupMember(rows[index].actionKind) || rows[index].actionKind === "Drop") {
    return false;
  }
  const followingKind = nextNonDropActionKind(rows, index);
  return followingKind !== undefined && isGroupMember(followingKind);
}

// A stable fingerprint of every group's membership, keyed by the leader's commit id: the ordered
// list of the members' commit ids and their kinds (Squash vs Fixup changes the default message,
// so it's part of the identity). Drop rows are excluded — they contribute nothing to the group's
// default message, so a Drop moving in or out of the middle of a group doesn't make it a
// different group.
function groupFingerprints(rows: Row[]): Map<string, string> {
  const fingerprints = new Map<string, string>();
  for (let i = 0; i < rows.length; i++) {
    if (!isLeader(rows, i)) {
      continue;
    }
    const members: string[] = [];
    for (let j = i + 1; j < rows.length; j++) {
      const kind = rows[j].actionKind;
      if (kind === "Drop") {
        continue;
      }
      if (!isGroupMember(kind)) {
        break;
      }
      members.push(`${rows[j].commit.id}:${kind}`);
    }
    fingerprints.set(rows[i].commit.id, members.join("|"));
  }
  return fingerprints;
}

// Re-derives which rows are group leaders after a reorder or action change, recomputing a fresh
// default combined message only for groups that actually changed. A leader whose group is
// untouched by the mutation keeps its current `combinedMessage` verbatim — otherwise editing any
// row would silently wipe a combined message the user hand-typed on an unrelated group.
function recomputeGroupLeaders(prev: Row[], next: Row[]): Row[] {
  const before = groupFingerprints(prev);
  const after = groupFingerprints(next);
  return next.map((row, index) => {
    if (!after.has(row.commit.id)) {
      // Not a leader (any more): no combined-message field belongs on this row.
      return { ...row, combinedMessage: null };
    }
    const unchanged =
      before.has(row.commit.id) &&
      before.get(row.commit.id) === after.get(row.commit.id) &&
      row.combinedMessage !== null;
    return {
      ...row,
      combinedMessage: unchanged ? row.combinedMessage : defaultCombinedMessage(next, index),
    };
  });
}

export function RebasePlanner({
  client,
  onto,
  onStartRebase,
  onCancel,
  operationDisabled = false,
}: {
  client: RepoClient;
  onto: string;
  onStartRebase: (onto: string, plan: RebasePlanEntry[]) => void;
  onCancel: () => void;
  operationDisabled?: boolean;
}) {
  const [rows, setRows] = useState<Row[]>([]);

  useEffect(() => {
    let ignore = false;
    client.commitsSince(onto).then((commits) => {
      if (!ignore) {
        setRows(
          commits.map((commit) => ({
            commit,
            actionKind: "Pick",
            rewordMessage: commit.summary,
            combinedMessage: null,
          })),
        );
      }
    });
    return () => {
      ignore = true;
    };
  }, [client, onto]);

  const moveRow = (index: number, direction: -1 | 1) => {
    const target = index + direction;
    if (target < 0 || target >= rows.length) {
      return;
    }
    const next = [...rows];
    [next[index], next[target]] = [next[target], next[index]];
    setRows(recomputeGroupLeaders(rows, next));
  };

  const setActionKind = (index: number, actionKind: ActionKind) => {
    const next = [...rows];
    next[index] = { ...next[index], actionKind };
    setRows(recomputeGroupLeaders(rows, next));
  };

  const setRewordMessage = (index: number, rewordMessage: string) => {
    const next = [...rows];
    next[index] = { ...next[index], rewordMessage };
    setRows(next);
  };

  const setCombinedMessage = (index: number, combinedMessage: string) => {
    const next = [...rows];
    next[index] = { ...next[index], combinedMessage };
    setRows(next);
  };

  const start = () => {
    const plan: RebasePlanEntry[] = rows.map((row) => {
      let action: RebaseAction;
      switch (row.actionKind) {
        case "Reword":
          action = { kind: "Reword", message: row.rewordMessage };
          break;
        case "Pick":
        case "Edit":
        case "Squash":
        case "Fixup":
        case "Drop":
          action = { kind: row.actionKind };
          break;
      }
      return {
        commitId: row.commit.id,
        action,
        combinedMessage: row.combinedMessage,
      };
    });
    onStartRebase(onto, plan);
  };

  return (
    <div>
      <ul>
        {rows.map((row, index) => (
          <li key={row.commit.id}>
            <span>
              {row.commit.shortId} {row.commit.summary}
            </span>
            <button onClick={() => moveRow(index, -1)} disabled={index === 0}>
              Move up
            </button>
            <button onClick={() => moveRow(index, 1)} disabled={index === rows.length - 1}>
              Move down
            </button>
            <label>
              Action
              <select
                aria-label="Action"
                value={row.actionKind}
                onChange={(event) => setActionKind(index, event.target.value as ActionKind)}
              >
                <option value="Pick">Pick</option>
                <option value="Reword">Reword</option>
                <option value="Edit">Edit</option>
                <option value="Squash" disabled={index === 0}>
                  Squash
                </option>
                <option value="Fixup" disabled={index === 0}>
                  Fixup
                </option>
                <option value="Drop">Drop</option>
              </select>
            </label>
            {row.actionKind === "Reword" && (
              <input
                placeholder="New commit message"
                value={row.rewordMessage}
                onChange={(event) => setRewordMessage(index, event.target.value)}
              />
            )}
            {row.combinedMessage !== null && (
              <label>
                Combined message
                <textarea
                  aria-label="Combined message"
                  value={row.combinedMessage}
                  onChange={(event) => setCombinedMessage(index, event.target.value)}
                />
              </label>
            )}
          </li>
        ))}
      </ul>
      <button onClick={start} disabled={operationDisabled}>Start Rebase</button>
      <button onClick={onCancel}>Cancel</button>
    </div>
  );
}

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

function recomputeGroupLeaders(rows: Row[]): Row[] {
  const next = rows.map((r) => ({ ...r, combinedMessage: null as string | null }));
  for (let i = 0; i < next.length; i++) {
    // A group leader must be neither a group member itself nor a Drop (a Drop never lands, so
    // it can't be the commit the group gets squashed onto — see `land_current_step`'s backward
    // walk, which excludes Drop from leader candidates the same way).
    if (isGroupMember(next[i].actionKind) || next[i].actionKind === "Drop") {
      continue;
    }
    const followingKind = nextNonDropActionKind(next, i);
    if (followingKind !== undefined && isGroupMember(followingKind)) {
      next[i].combinedMessage = defaultCombinedMessage(next, i);
    }
  }
  return next;
}

export function RebasePlanner({
  client,
  onto,
  onStartRebase,
  onCancel,
}: {
  client: RepoClient;
  onto: string;
  onStartRebase: (onto: string, plan: RebasePlanEntry[]) => void;
  onCancel: () => void;
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
    setRows(recomputeGroupLeaders(next));
  };

  const setActionKind = (index: number, actionKind: ActionKind) => {
    const next = [...rows];
    next[index] = { ...next[index], actionKind };
    setRows(recomputeGroupLeaders(next));
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
      <button onClick={start}>Start Rebase</button>
      <button onClick={onCancel}>Cancel</button>
    </div>
  );
}

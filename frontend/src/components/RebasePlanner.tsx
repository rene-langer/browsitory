import { useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
import type {
  RebaseAction,
  RebasePlanCommit,
  RebasePlanEntry,
  RepoClient,
} from "../ipc/RepoClient";
import { ListRow } from "./primitives/ListRow";
import { Panel } from "./primitives/Panel";
import { Toolbar } from "./primitives/Toolbar";
import type { PresetAction } from "../lib/rebaseSelection";
import styles from "./RebasePlanner.module.css";
import formStyles from "./primitives/FormDialog.module.css";

type ActionKind = RebaseAction["kind"];

interface Row {
  commit: RebasePlanCommit;
  actionKind: ActionKind;
  /** The full message to reword, body included (#655); `null` until it is loaded. */
  rewordMessage: string | null;
  rewordLoading: boolean;
  /** Why the full message couldn't be read; the summary is then filled in instead. */
  rewordError: string | null;
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
  repoPath,
  client,
  onto,
  onStartRebase,
  onCancel,
  blockedReason = null,
  presetActions,
}: {
  repoPath: string;
  client: RepoClient;
  onto: string;
  onStartRebase: (onto: string, plan: RebasePlanEntry[]) => void;
  onCancel: () => void;
  /** Why repository operations are blocked now (`operationBlockers`), or `null` (#521). */
  blockedReason?: string | null;
  // Per-commit actions to default to instead of "Pick" — set when the plan is opened from a
  // multi-select action in the commit graph, so the selection arrives already marked instead of
  // the user re-marking every row by hand. Every id must appear in the planned span (see
  // `presetMissing`).
  presetActions?: ReadonlyMap<string, PresetAction>;
}) {
  const [rows, setRows] = useState<Row[]>([]);
  // True when a preset commit isn't in `commitsSince(onto)`, e.g. the selection includes a
  // commit from another branch. Starting would silently ignore part of the selection.
  const [presetMissing, setPresetMissing] = useState(false);
  const [load, setLoad] = useState<{ kind: "loading" } | { kind: "ready" } | { kind: "error"; message: string }>({
    kind: "loading",
  });
  const [attempt, setAttempt] = useState(0);
  // The control that moved a row: a keyed reorder can take its node out of the document, which
  // drops focus, so it is focused again after the render.
  const refocus = useRef<HTMLElement | null>(null);

  useLayoutEffect(() => {
    const element = refocus.current;
    refocus.current = null;
    if (element !== null && document.activeElement !== element && element.isConnected) element.focus();
  }, [rows]);

  useEffect(() => {
    let ignore = false;
    client
      .commitsSince(repoPath, onto)
      .then((commits) => {
        if (!ignore) {
          const initialRows = commits.map((commit) => ({
            commit,
            actionKind: (presetActions?.get(commit.id) ?? "Pick") as ActionKind,
            rewordMessage: null,
            rewordLoading: false,
            rewordError: null,
            combinedMessage: null,
          }));
          const planned = new Set(commits.map((commit) => commit.id));
          setPresetMissing(
            presetActions !== undefined && [...presetActions.keys()].some((id) => !planned.has(id)),
          );
          setRows(recomputeGroupLeaders(initialRows, initialRows));
          setLoad({ kind: "ready" });
        }
      })
      .catch((err: unknown) => {
        if (!ignore) setLoad({ kind: "error", message: err instanceof Error ? err.message : String(err) });
      });
    return () => {
      ignore = true;
    };
  }, [repoPath, client, onto, presetActions, attempt]);

  const retry = () => {
    setLoad({ kind: "loading" });
    setAttempt((n) => n + 1);
  };

  const moveRow = (index: number, direction: -1 | 1, control?: HTMLElement) => {
    const target = index + direction;
    if (target < 0 || target >= rows.length) {
      return;
    }
    const next = [...rows];
    [next[index], next[target]] = [next[target], next[index]];
    refocus.current = control ?? null;
    setRows(recomputeGroupLeaders(rows, next));
  };

  // Alt+Up/Down moves the row holding the focused control; plain arrows stay with the control.
  const onRowKeyDown = (index: number, event: KeyboardEvent<HTMLElement>) => {
    if (!event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
    if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
    event.preventDefault();
    moveRow(index, event.key === "ArrowUp" ? -1 : 1, event.target as HTMLElement);
  };

  const updateRow = (commitId: string, change: Partial<Row>) => {
    setRows((prev) => prev.map((row) => (row.commit.id === commitId ? { ...row, ...change } : row)));
  };

  const loadRewordMessage = (commit: RebasePlanCommit) => {
    client
      .getCommitMessage(repoPath, commit.id)
      .then((message) => updateRow(commit.id, { rewordMessage: message, rewordLoading: false }))
      .catch((err: unknown) =>
        updateRow(commit.id, {
          rewordMessage: commit.summary,
          rewordLoading: false,
          rewordError: err instanceof Error ? err.message : String(err),
        }),
      );
  };

  const setActionKind = (index: number, actionKind: ActionKind) => {
    const next = [...rows];
    const row = next[index];
    const needsMessage = actionKind === "Reword" && row.rewordMessage === null && !row.rewordLoading;
    next[index] = { ...row, actionKind, rewordLoading: row.rewordLoading || needsMessage };
    setRows(recomputeGroupLeaders(rows, next));
    if (needsMessage) loadRewordMessage(row.commit);
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

  const messageLoading = rows.some((row) => row.actionKind === "Reword" && row.rewordLoading);
  const startDisabled =
    blockedReason !== null || presetMissing || load.kind !== "ready" || rows.length === 0 || messageLoading;

  const start = () => {
    if (startDisabled) return;
    const plan: RebasePlanEntry[] = rows.map((row) => {
      let action: RebaseAction;
      switch (row.actionKind) {
        case "Reword":
          action = { kind: "Reword", message: row.rewordMessage ?? row.commit.summary };
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
    <Panel title="Rebase plan">
      {load.kind === "loading" && <p className={styles.caption}>Loading commits…</p>}
      {load.kind === "error" && (
        <div role="alert">
          <p>Couldn&apos;t list the commits to rebase: {load.message}</p>
          <button onClick={retry}>Retry</button>
        </div>
      )}
      {load.kind === "ready" && rows.length === 0 && (
        <p>No commits to rebase: the current branch has nothing above this base.</p>
      )}
      {rows.length > 0 && (
        <p className={styles.caption}>
          Oldest commit first: the rebase replays them from top to bottom. Alt+Up and Alt+Down move
          the row you are in.
        </p>
      )}
      <ul className={styles.list}>
        {rows.map((row, index) => {
          const name = `${row.commit.shortId} ${row.commit.summary}`;
          return (
            <ListRow key={row.commit.id}>
              <div className={styles.rowKeys} onKeyDown={(event) => onRowKeyDown(index, event)}>
                <span>{name}</span>
                <button
                  aria-label={`Move up: ${name}`}
                  aria-keyshortcuts="Alt+ArrowUp"
                  onClick={(event) => moveRow(index, -1, event.currentTarget)}
                  disabled={index === 0}
                >
                  Move up
                </button>
                <button
                  aria-label={`Move down: ${name}`}
                  aria-keyshortcuts="Alt+ArrowDown"
                  onClick={(event) => moveRow(index, 1, event.currentTarget)}
                  disabled={index === rows.length - 1}
                >
                  Move down
                </button>
                <label>
                  Action
                  <select
                    aria-label={`Action for ${name}`}
                    className={styles.actionSelect}
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
                {row.actionKind === "Reword" &&
                  (row.rewordMessage === null ? (
                    <p className={styles.caption}>Loading the message…</p>
                  ) : (
                    <>
                      <textarea
                        aria-label={`New message for ${row.commit.shortId}`}
                        placeholder="New commit message"
                        className={styles.message}
                        rows={Math.min(10, Math.max(3, row.rewordMessage.split("\n").length))}
                        value={row.rewordMessage}
                        onChange={(event) => setRewordMessage(index, event.target.value)}
                      />
                      {row.rewordError !== null && (
                        <p role="alert">
                          Couldn&apos;t read the full message of {row.commit.shortId} ({row.rewordError}). Only
                          the summary is filled in; add the body back if it had one.
                        </p>
                      )}
                    </>
                  ))}
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
              </div>
            </ListRow>
          );
        })}
      </ul>
      {presetMissing && (
        <p role="alert">
          Some selected commits are not on the current branch&apos;s history above this base, so
          they can&apos;t be part of this rebase.
        </p>
      )}
      {blockedReason !== null && <p className={formStyles.blocked}>{blockedReason}</p>}
      <Toolbar>
        <button onClick={start} disabled={startDisabled} title={blockedReason ?? undefined}>
          Start rebase
        </button>
        <button onClick={onCancel}>Cancel</button>
      </Toolbar>
    </Panel>
  );
}

import { useEffect, useId, useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
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
import { BlockedReason } from "./primitives/BlockedReason";
import { describedBy } from "../lib/ariaDescribedBy";

type ActionKind = RebaseAction["kind"];

interface Row {
  commit: RebasePlanCommit;
  actionKind: ActionKind;
  /**
   * The reword message the user typed; `null` while untouched, and the field then shows the
   * commit's full message, body included (#655).
   */
  rewordMessage: string | null;
  /**
   * The combined message the user typed on a group leader; `null` while untouched, and the field
   * then shows the default built from the group's full messages (#721).
   */
  combinedMessage: string | null;
}

/** A commit's full message, read with `getCommitMessage` when a field needs it. */
type FullMessage = { kind: "loading" } | { kind: "ready"; text: string } | { kind: "failed"; error: string };

type FullMessages = ReadonlyMap<string, FullMessage>;

/** The commit's full message once read; its summary while loading or when it couldn't be read. */
function messageOf(messages: FullMessages, commit: RebasePlanCommit): string {
  const message = messages.get(commit.id);
  return message?.kind === "ready" ? message.text : commit.summary;
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

// The commits whose messages make up a group's default: the leader and its Squash members, as
// `git rebase -i` builds a squash message (a Fixup's message is dropped).
function groupMessageCommits(rows: Row[], leaderIndex: number): RebasePlanCommit[] {
  const commits = [rows[leaderIndex].commit];
  for (let i = leaderIndex + 1; i < rows.length; i++) {
    const kind = rows[i].actionKind;
    if (kind === "Drop") {
      continue;
    }
    if (!isGroupMember(kind)) {
      break;
    }
    if (kind === "Squash") {
      commits.push(rows[i].commit);
    }
  }
  return commits;
}

// The full messages, bodies included, one after another (#721). Until a message is read, or when
// it can't be, its summary stands in.
function defaultCombinedMessage(rows: Row[], leaderIndex: number, messages: FullMessages): string {
  return groupMessageCommits(rows, leaderIndex)
    .map((commit) => messageOf(messages, commit).trimEnd())
    .join("\n\n");
}

// The commits whose full messages the plan needs: every Reword row and every group's leader and
// Squash members.
function commitsNeedingMessages(rows: Row[]): RebasePlanCommit[] {
  const needed: RebasePlanCommit[] = [];
  rows.forEach((row, index) => {
    if (row.actionKind === "Reword") needed.push(row.commit);
    if (isLeader(rows, index)) needed.push(...groupMessageCommits(rows, index));
  });
  return needed;
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

// Re-derives which rows are group leaders after a reorder or action change. A leader keeps a
// hand-typed combined message only while its group is unchanged; otherwise editing any row would
// silently wipe a message typed on an unrelated group. A changed group goes back to the default.
function recomputeGroupLeaders(prev: Row[], next: Row[]): Row[] {
  const before = groupFingerprints(prev);
  const after = groupFingerprints(next);
  return next.map((row) => {
    const unchanged =
      after.has(row.commit.id) &&
      before.has(row.commit.id) &&
      before.get(row.commit.id) === after.get(row.commit.id);
    return unchanged ? row : { ...row, combinedMessage: null };
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
  const reasonId = useId();
  const [rows, setRows] = useState<Row[]>([]);
  // True when a preset commit isn't in `commitsSince(onto)`, e.g. the selection includes a
  // commit from another branch. Starting would silently ignore part of the selection.
  const [presetMissing, setPresetMissing] = useState(false);
  const [load, setLoad] = useState<{ kind: "loading" } | { kind: "ready" } | { kind: "error"; message: string }>({
    kind: "loading",
  });
  const [attempt, setAttempt] = useState(0);
  const [messages, setMessages] = useState<FullMessages>(new Map());
  // The same map, current at once, so a request made in the same tick doesn't read it twice.
  const messagesRef = useRef<Map<string, FullMessage>>(new Map());
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
            combinedMessage: null,
          }));
          const planned = new Set(commits.map((commit) => commit.id));
          setPresetMissing(
            presetActions !== undefined && [...presetActions.keys()].some((id) => !planned.has(id)),
          );
          setRows(initialRows);
          requestMessages(commitsNeedingMessages(initialRows));
          setLoad({ kind: "ready" });
        }
      })
      .catch((err: unknown) => {
        if (!ignore) setLoad({ kind: "error", message: err instanceof Error ? err.message : String(err) });
      });
    return () => {
      ignore = true;
    };
    // `requestMessages` only reads the ref and the props listed here.
    // eslint-disable-next-line react-hooks/exhaustive-deps
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
    changeRows(next);
  };

  // Alt+Up/Down moves the row holding the focused control; plain arrows stay with the control.
  const onRowKeyDown = (index: number, event: KeyboardEvent<HTMLElement>) => {
    if (!event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
    if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
    event.preventDefault();
    moveRow(index, event.key === "ArrowUp" ? -1 : 1, event.target as HTMLElement);
  };

  // Reads the full messages not read yet; the fields show them as they arrive.
  function requestMessages(commits: RebasePlanCommit[]) {
    const settle = (id: string, message: FullMessage) => {
      messagesRef.current.set(id, message);
      setMessages(new Map(messagesRef.current));
    };
    let requested = false;
    for (const commit of commits) {
      if (messagesRef.current.has(commit.id)) continue;
      messagesRef.current.set(commit.id, { kind: "loading" });
      requested = true;
      client
        .getCommitMessage(repoPath, commit.id)
        .then((text) => settle(commit.id, { kind: "ready", text }))
        .catch((err: unknown) =>
          settle(commit.id, { kind: "failed", error: err instanceof Error ? err.message : String(err) }),
        );
    }
    if (requested) setMessages(new Map(messagesRef.current));
  }

  const changeRows = (next: Row[]) => {
    setRows(recomputeGroupLeaders(rows, next));
    requestMessages(commitsNeedingMessages(next));
  };

  const setActionKind = (index: number, actionKind: ActionKind) => {
    const next = [...rows];
    next[index] = { ...next[index], actionKind };
    changeRows(next);
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

  const messageLoading = commitsNeedingMessages(rows).some(
    (commit) => (messages.get(commit.id)?.kind ?? "loading") === "loading",
  );
  // The combined message each group leader starts with: typed, or the default.
  const combinedMessageAt = (index: number): string | null =>
    isLeader(rows, index) ? (rows[index].combinedMessage ?? defaultCombinedMessage(rows, index, messages)) : null;
  const startDisabled =
    blockedReason !== null || presetMissing || load.kind !== "ready" || rows.length === 0 || messageLoading;

  const start = () => {
    if (startDisabled) return;
    const plan: RebasePlanEntry[] = rows.map((row, index) => {
      let action: RebaseAction;
      switch (row.actionKind) {
        case "Reword":
          action = { kind: "Reword", message: row.rewordMessage ?? messageOf(messages, row.commit) };
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
        combinedMessage: combinedMessageAt(index),
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
          const combined = combinedMessageAt(index);
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
                {row.actionKind === "Reword" && (
                  <RewordField
                    row={row}
                    message={messages.get(row.commit.id)}
                    onChange={(rewordMessage) => setRewordMessage(index, rewordMessage)}
                  />
                )}
                {combined !== null && (
                  <>
                    <label>
                      Combined message
                      <textarea
                        aria-label="Combined message"
                        className={styles.message}
                        rows={Math.min(12, Math.max(3, combined.split("\n").length))}
                        value={combined}
                        onChange={(event) => setCombinedMessage(index, event.target.value)}
                      />
                    </label>
                    {row.combinedMessage === null && (
                      <GroupMessageNotes commits={groupMessageCommits(rows, index)} messages={messages} />
                    )}
                  </>
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
      <BlockedReason id={reasonId} reason={blockedReason} />
      <Toolbar>
        <button
          onClick={start}
          disabled={startDisabled}
          title={blockedReason ?? undefined}
          aria-describedby={describedBy(reasonId, blockedReason)}
        >
          Start rebase
        </button>
        <button onClick={onCancel}>Cancel</button>
      </Toolbar>
    </Panel>
  );
}

function RewordField({
  row,
  message,
  onChange,
}: {
  row: Row;
  message: FullMessage | undefined;
  onChange: (message: string) => void;
}) {
  if (row.rewordMessage === null && (message === undefined || message.kind === "loading")) {
    return <p className={styles.caption}>Loading the message…</p>;
  }
  const value = row.rewordMessage ?? (message?.kind === "ready" ? message.text : row.commit.summary);
  return (
    <>
      <textarea
        aria-label={`New message for ${row.commit.shortId}`}
        placeholder="New commit message"
        className={styles.message}
        rows={Math.min(10, Math.max(3, value.split("\n").length))}
        value={value}
        onChange={(event) => onChange(event.target.value)}
      />
      {message?.kind === "failed" && (
        <p role="alert">
          Couldn&apos;t read the full message of {row.commit.shortId} ({message.error}). Only the
          summary is filled in; add the body back if it had one.
        </p>
      )}
    </>
  );
}

/** Under an untouched combined message: what is still loading, or couldn't be read (#721). */
function GroupMessageNotes({ commits, messages }: { commits: RebasePlanCommit[]; messages: FullMessages }) {
  const loading = commits.some((commit) => (messages.get(commit.id)?.kind ?? "loading") === "loading");
  const failed = commits.flatMap((commit) => {
    const message = messages.get(commit.id);
    return message?.kind === "failed" ? [{ commit, error: message.error }] : [];
  });
  return (
    <>
      {loading && <p className={styles.caption}>Loading the full messages…</p>}
      {failed.map(({ commit, error }) => (
        <p role="alert" key={commit.id}>
          Couldn&apos;t read the full message of {commit.shortId} ({error}). Only its summary is in
          the combined message; add the body back if it had one.
        </p>
      ))}
    </>
  );
}

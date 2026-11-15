import { useCallback, useEffect, useRef } from "react";
import type { HeadInfo, StatusEntry } from "../ipc/RepoClient";
import { type AbortWork, mergeMessageConflicts } from "../lib/abortConfirm";

/** The parts of the app state the abort's work is read from. */
export interface AbortWorkState {
  loaded: boolean;
  status: StatusEntry[];
  mergeMessage: string | null;
  headInfo: Pick<HeadInfo, "operation" | "sequence" | "rebase">;
  editMessage: "unchanged" | "changed" | "blank";
}

/** Which paused operation this is: what was resolved during it stays counted until it ends. */
function operationKey(headInfo: AbortWorkState["headInfo"]): string | null {
  if (headInfo.sequence !== null) return `sequence:${headInfo.sequence.kind}`;
  if (headInfo.rebase !== null) return `rebase:${headInfo.rebase.branch}:${headInfo.rebase.onto}`;
  if (headInfo.operation !== null) return `operation:${headInfo.operation}`;
  return null;
}

function statusSignature(status: readonly StatusEntry[]): string {
  return status
    .map((entry) => `${entry.staged ? "S" : "U"}${entry.kind}:${entry.oldPath ?? ""}>${entry.path}`)
    .sort()
    .join("\n");
}

/**
 * What Abort of the paused operation would lose (#979), read when Abort is clicked:
 * - the files resolved since it stopped: those git listed as conflicted in `MERGE_MSG`, or seen
 *   conflicted in this session (a rebase's earlier steps too), that are no longer conflicted;
 * - an edit stop's changed files: its status differs from the status when the stop was first
 *   seen. A stop that was already there when the tab opened has no such baseline, so any change
 *   in the working tree counts;
 * - an edit stop's own message, a split's committed parts and the merge editor's unsaved edits.
 */
export function useAbortWork(state: AbortWorkState, mergeEditorEdited: boolean): () => AbortWork {
  const tracked = useRef<{
    key: string | null;
    conflicted: Set<string>;
    /** A loaded state without any operation was seen: a stop seen after it started here. */
    sawNoOperation: boolean;
    edit: { commitId: string; baseline: string | null } | null;
  }>({ key: null, conflicted: new Set(), sawNoOperation: false, edit: null });
  const latest = useRef({ state, mergeEditorEdited });
  useEffect(() => {
    latest.current = { state, mergeEditorEdited };
  });

  const { loaded, status, mergeMessage, headInfo } = state;
  useEffect(() => {
    if (!loaded) return;
    const current = tracked.current;
    const key = operationKey(headInfo);
    if (key !== current.key) {
      current.key = key;
      current.conflicted = new Set();
      current.edit = null;
    }
    if (key === null) {
      current.sawNoOperation = true;
      return;
    }
    for (const path of mergeMessageConflicts(mergeMessage)) current.conflicted.add(path);
    for (const entry of status) if (entry.kind === "Conflicted") current.conflicted.add(entry.path);
    const editing = headInfo.rebase?.editing ?? null;
    if (editing === null) {
      current.edit = null;
    } else if (current.edit?.commitId !== editing) {
      current.edit = { commitId: editing, baseline: current.sawNoOperation ? statusSignature(status) : null };
    }
  }, [loaded, status, mergeMessage, headInfo]);

  return useCallback(() => {
    const { state: now, mergeEditorEdited: edited } = latest.current;
    const current = tracked.current;
    const stillConflicted = new Set(now.status.filter((entry) => entry.kind === "Conflicted").map((entry) => entry.path));
    const resolvedFiles = [...current.conflicted].filter((path) => !stillConflicted.has(path)).length;
    const edit = now.headInfo.rebase?.editing == null ? null : current.edit;
    const changedFiles =
      edit !== null &&
      (edit.baseline === null ? now.status.length > 0 : statusSignature(now.status) !== edit.baseline);
    return {
      resolvedFiles,
      mergeEditorEdits: edited,
      changedFiles,
      messageChanged: now.headInfo.rebase?.editing != null && now.editMessage !== "unchanged",
      splitParts: now.headInfo.rebase?.splitParts ?? 0,
    };
  }, []);
}

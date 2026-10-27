import { useCallback, useEffect, useRef, useState } from "react";
import type { CompareRange, RepoClient } from "../ipc/RepoClient";
import { describeError, type DescribedError } from "../lib/errorMessages";
import { rowsEqual, type SelectedRow } from "./useAppState";

export type CompareTarget =
  | { kind: "commits"; fromId: string; toId: string }
  | { kind: "branch"; refName: string; label: string }
  | { kind: "workingTree"; commitId: string };

export interface CompareState {
  target: CompareTarget;
  /** `toId === null` means the working tree. `null` while a branch compare resolves or failed. */
  range: { fromId: string; toId: string | null } | null;
  error: DescribedError | null;
}

export interface UseCompareResult {
  /** `null` while the diff pane shows the selected row as usual. */
  compare: CompareState | null;
  compareCommits(fromId: string, toId: string): void;
  compareBranch(refName: string, label: string): void;
  compareWorkingTree(commitId: string): void;
  swap(): void;
  close(): void;
  retry(): void;
}

interface Opened {
  target: CompareTarget;
  // The selection the compare was opened under; selecting anything else closes it.
  row: SelectedRow;
}

interface Resolved {
  refName: string;
  // The `retryToken` this result was produced under; an older error is hidden while a retry runs.
  token: number;
  range: CompareRange | null;
  error: DescribedError | null;
}

export function useCompare(
  client: RepoClient,
  repoPath: string,
  refreshGeneration: number,
  selectedRow: SelectedRow,
): UseCompareResult {
  const [opened, setOpened] = useState<Opened | null>(null);
  const [resolved, setResolved] = useState<Resolved | null>(null);
  const [retryToken, setRetryToken] = useState(0);
  const selectedRowRef = useRef(selectedRow);
  useEffect(() => {
    selectedRowRef.current = selectedRow;
  }, [selectedRow]);

  // A different selection closes the compare for good (it must not come back when the user
  // returns to that row). Adjusted during render, not in an effect: `react-hooks/set-state-in-effect`
  // forbids a synchronous setState there, and this also hides it in the same render.
  if (opened !== null && !rowsEqual(opened.row, selectedRow)) {
    setOpened(null);
  }
  const active = opened !== null && rowsEqual(opened.row, selectedRow) ? opened : null;

  const branchRef = active?.target.kind === "branch" ? active.target.refName : null;
  useEffect(() => {
    if (branchRef === null) return;
    let ignore = false;
    // No synchronous reset here: a result for another ref is ignored below, and a range already
    // shown for this ref stays up while it re-resolves after a refresh.
    client.getBranchCompareRange(repoPath, branchRef).then(
      (range) => {
        if (!ignore) setResolved({ refName: branchRef, token: retryToken, range, error: null });
      },
      (err: unknown) => {
        if (!ignore) setResolved({ refName: branchRef, token: retryToken, range: null, error: describeError(err) });
      },
    );
    return () => {
      ignore = true;
    };
  }, [client, repoPath, branchRef, refreshGeneration, retryToken]);

  const open = useCallback((target: CompareTarget) => setOpened({ target, row: selectedRowRef.current }), []);
  const compareCommits = useCallback((fromId: string, toId: string) => open({ kind: "commits", fromId, toId }), [open]);
  const compareBranch = useCallback((refName: string, label: string) => open({ kind: "branch", refName, label }), [open]);
  const compareWorkingTree = useCallback((commitId: string) => open({ kind: "workingTree", commitId }), [open]);
  const swap = useCallback(
    () =>
      setOpened((prev) =>
        prev?.target.kind === "commits"
          ? { ...prev, target: { kind: "commits", fromId: prev.target.toId, toId: prev.target.fromId } }
          : prev,
      ),
    [],
  );
  const close = useCallback(() => setOpened(null), []);
  const retry = useCallback(() => setRetryToken((token) => token + 1), []);

  let compare: CompareState | null = null;
  if (active !== null) {
    const { target } = active;
    if (target.kind === "commits") {
      compare = { target, range: { fromId: target.fromId, toId: target.toId }, error: null };
    } else if (target.kind === "workingTree") {
      compare = { target, range: { fromId: target.commitId, toId: null }, error: null };
    } else {
      const current = resolved?.refName === target.refName ? resolved : null;
      const error = current !== null && current.token === retryToken ? current.error : null;
      compare = { target, range: current?.range ?? null, error };
    }
  }

  return { compare, compareCommits, compareBranch, compareWorkingTree, swap, close, retry };
}

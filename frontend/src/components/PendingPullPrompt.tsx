import { useEffect, useRef, useState } from "react";
import type { FastForwardMode, PullStrategy } from "../ipc/RepoClient";
import { logFailure } from "../lib/logger";
import { PullDivergedDialog } from "./PullDivergedDialog";

/**
 * What a diverged pull does next (#652): the repository's pull strategy (remembered, else
 * `pull.rebase` / `pull.ff` from git config) merges or rebases at once (once the repository is
 * idle; until then the dialog says why it waits); with none, or with "Fast-forward only", it
 * asks with `PullDivergedDialog`. Mounted while `pendingPull` is set, so
 * each diverged pull reads the strategy once.
 *
 * With `noFastForward` the pull could fast-forward but git config wants a merge commit (#740):
 * it never asks. It merges with a merge commit (`Never`), unless the remembered strategy is
 * Rebase or Fast-forward only, which fast-forward (`Only`), as `git pull --rebase` or
 * `--ff-only` would. While the repository is busy it waits without a dialog.
 */
export function PendingPullPrompt({
  upstreamRef,
  noFastForward = false,
  branchName,
  operationDisabled,
  operationDisabledReason,
  getPullStrategy,
  setPullStrategy,
  onMerge,
  onRebase,
  onCancel,
}: {
  upstreamRef: string;
  noFastForward?: boolean;
  branchName: string | null;
  operationDisabled: boolean;
  operationDisabledReason: string | null;
  getPullStrategy: () => Promise<PullStrategy | null>;
  setPullStrategy: (strategy: PullStrategy) => Promise<void>;
  /** Both clear the pending pull first. `onMerge` gets a fast-forward mode only for `noFastForward`. */
  onMerge: (upstreamRef: string, fastForward?: FastForwardMode) => void;
  onRebase: (upstreamRef: string) => void;
  onCancel: () => void;
}) {
  // `undefined` while the strategy is read: nothing shows until it says what to do.
  const [strategy, setStrategy] = useState<PullStrategy | null | undefined>(undefined);
  const started = useRef(false);
  const acted = useRef(false);
  useEffect(() => {
    // Once per pending pull, also under StrictMode's second effect run.
    if (started.current) return;
    started.current = true;
    void getPullStrategy()
      .catch((error: unknown) => {
        logFailure("Reading the pull strategy failed", error);
        return null;
      })
      .then(setStrategy);
    // Read once per mount: the pending pull this prompt is for.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const automatic = noFastForward ? strategy !== undefined : strategy === "Merge" || strategy === "Rebase";
  useEffect(() => {
    // Waits while the repository is busy (the pull's transfer may still be settling, or another
    // window runs something); the dialog meanwhile says why.
    if (!automatic || operationDisabled || acted.current) return;
    acted.current = true;
    if (noFastForward) onMerge(upstreamRef, strategy === "Rebase" || strategy === "FastForwardOnly" ? "Only" : "Never");
    else if (strategy === "Merge") onMerge(upstreamRef);
    else onRebase(upstreamRef);
  }, [automatic, noFastForward, operationDisabled, strategy, onMerge, onRebase, upstreamRef]);
  if (strategy === undefined || (automatic && (noFastForward || !operationDisabled))) return null;
  const choose = (choice: PullStrategy, remember: boolean, run: (upstreamRef: string) => void) => {
    if (operationDisabled) return;
    if (remember) void setPullStrategy(choice);
    run(upstreamRef);
  };
  return (
    <PullDivergedDialog
      upstreamRef={upstreamRef}
      branchName={branchName}
      fastForwardOnly={strategy === "FastForwardOnly"}
      operationDisabled={operationDisabled}
      operationDisabledReason={operationDisabledReason}
      onMerge={(_, remember) => choose("Merge", remember, onMerge)}
      onRebase={(_, remember) => choose("Rebase", remember, onRebase)}
      onCancel={onCancel}
    />
  );
}

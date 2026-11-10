import { useEffect, useRef, useState } from "react";
import type { PullStrategy } from "../ipc/RepoClient";
import { logFailure } from "../lib/logger";
import { PullDivergedDialog } from "./PullDivergedDialog";

/**
 * What a diverged pull does next (#652): the repository's pull strategy (remembered, else
 * `pull.rebase` / `pull.ff` from git config) merges or rebases at once; with none, or with
 * "Fast-forward only", it asks with `PullDivergedDialog`. Mounted while `pendingPull` is set, so
 * each diverged pull reads the strategy once.
 */
export function PendingPullPrompt({
  upstreamRef,
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
  branchName: string | null;
  operationDisabled: boolean;
  operationDisabledReason: string | null;
  getPullStrategy: () => Promise<PullStrategy | null>;
  setPullStrategy: (strategy: PullStrategy) => Promise<void>;
  /** Both clear the pending pull first. */
  onMerge: (upstreamRef: string) => void;
  onRebase: (upstreamRef: string) => void;
  onCancel: () => void;
}) {
  // `null` while the strategy is read: nothing shows until it says to ask.
  const [ask, setAsk] = useState<"ask" | "fastForwardOnly" | null>(null);
  const started = useRef(false);
  useEffect(() => {
    // Once per pending pull, also under StrictMode's second effect run.
    if (started.current) return;
    started.current = true;
    void getPullStrategy()
      .catch((error: unknown) => {
        logFailure("Reading the pull strategy failed", error);
        return null;
      })
      .then((strategy) => {
        // A busy repository (another window's operation) asks, where the buttons say why.
        if (strategy === "Merge" && !operationDisabled) onMerge(upstreamRef);
        else if (strategy === "Rebase" && !operationDisabled) onRebase(upstreamRef);
        else setAsk(strategy === "FastForwardOnly" ? "fastForwardOnly" : "ask");
      });
    // Read once per mount: the pending pull this prompt is for.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  if (ask === null) return null;
  const choose = (strategy: PullStrategy, remember: boolean, run: (upstreamRef: string) => void) => {
    if (operationDisabled) return;
    if (remember) void setPullStrategy(strategy);
    run(upstreamRef);
  };
  return (
    <PullDivergedDialog
      upstreamRef={upstreamRef}
      branchName={branchName}
      fastForwardOnly={ask === "fastForwardOnly"}
      operationDisabled={operationDisabled}
      operationDisabledReason={operationDisabledReason}
      onMerge={(_, remember) => choose("Merge", remember, onMerge)}
      onRebase={(_, remember) => choose("Rebase", remember, onRebase)}
      onCancel={onCancel}
    />
  );
}

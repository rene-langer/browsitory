import { useCallback, useEffect, useState } from "react";
import type { PullStrategy } from "../ipc/RepoClient";
import { logFailure } from "../lib/logger";
import type { UseAppStateResult } from "./useAppState";

/**
 * The toolbar Pull menu's state (#652): the strategy in effect for this repository (remembered,
 * else git config's, else `null`), read on open and again once a diverged pull's prompt closes
 * (its "Remember" may have changed it), and `pullWith`, which remembers a strategy and pulls.
 */
export function usePullStrategy(appState: UseAppStateResult) {
  const { getPullStrategy, setPullStrategy, pullCurrentUpstream } = appState;
  const [current, setCurrent] = useState<PullStrategy | null>(null);
  const pending = appState.state.pendingPull !== null;
  useEffect(() => {
    if (pending) return;
    let live = true;
    getPullStrategy().then(
      (strategy) => {
        if (live) setCurrent(strategy);
      },
      (error: unknown) => logFailure("Reading the pull strategy failed", error),
    );
    return () => {
      live = false;
    };
  }, [getPullStrategy, pending]);
  const pullWith = useCallback(
    (strategy: PullStrategy) => {
      setCurrent(strategy);
      void (async () => {
        await setPullStrategy(strategy);
        await pullCurrentUpstream();
      })();
    },
    [setPullStrategy, pullCurrentUpstream],
  );
  return { current, onPullWith: pullWith };
}

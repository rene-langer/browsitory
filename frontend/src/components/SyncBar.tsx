import type { RemoteInfo, UpstreamInfo } from "../ipc/RepoClient";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./SyncBar.module.css";

/**
 * Primary entry point for the three daily sync actions on the history pane, with the current
 * branch's ahead/behind counts against its (last fetched) upstream. Fetch falls back to the
 * only/first remote when the branch has no upstream; Pull and Push need one.
 */
export function SyncBar({
  remotes,
  upstream,
  operationDisabled,
  operationDisabledReason,
  onFetch,
  onPull,
  onPush,
}: {
  remotes: RemoteInfo[];
  upstream: UpstreamInfo | null;
  operationDisabled: boolean;
  operationDisabledReason: string | null;
  onFetch: (remoteName: string) => void | Promise<void>;
  onPull: () => void | Promise<void>;
  onPush: (remoteName: string) => void | Promise<void>;
}) {
  const fetchRemote = upstream?.remoteName ?? remotes[0]?.name ?? null;
  const busyTitle = operationDisabled ? (operationDisabledReason ?? undefined) : undefined;
  const fetchTitle = busyTitle ?? (fetchRemote === null ? "No remote configured." : `Fetch from ${fetchRemote}`);
  const upstreamTitle = (verb: string) =>
    busyTitle ?? (upstream === null ? "No upstream set for the current branch." : `${verb} ${upstream.remoteName}/${upstream.remoteBranch}`);
  const showCounts = upstream !== null && upstream.ahead != null && upstream.behind != null;

  return (
    <div className={styles.bar}>
      <Toolbar aria-label="Sync">
        <button
          type="button"
          disabled={operationDisabled || fetchRemote === null}
          title={fetchTitle}
          onClick={() => fetchRemote !== null && void onFetch(fetchRemote)}
        >
          Fetch
        </button>
        <button
          type="button"
          disabled={operationDisabled || upstream === null}
          title={upstreamTitle("Pull from")}
          onClick={() => void onPull()}
        >
          Pull
        </button>
        <button
          type="button"
          disabled={operationDisabled || upstream === null}
          title={upstreamTitle("Push to")}
          onClick={() => upstream !== null && void onPush(upstream.remoteName)}
        >
          Push
        </button>
      </Toolbar>
      {showCounts && (
        <span
          className={styles.counts}
          role="status"
          aria-label={`${upstream.ahead} ahead, ${upstream.behind} behind ${upstream.remoteName}/${upstream.remoteBranch}`}
        >
          <span title="Commits to push">↑{upstream.ahead}</span>
          <span title="Commits to pull">↓{upstream.behind}</span>
        </span>
      )}
    </div>
  );
}

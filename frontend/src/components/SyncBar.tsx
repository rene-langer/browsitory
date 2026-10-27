import { useState, type ReactNode } from "react";
import { ChevronDown } from "lucide-react";
import type { RemoteInfo, UpstreamInfo } from "../ipc/RepoClient";
import { ContextMenu } from "./primitives/ContextMenu";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./SyncBar.module.css";

/**
 * Primary entry point for the three daily sync actions on the history pane, with the current
 * branch's ahead/behind counts against its (last fetched) upstream. Fetch falls back to the
 * only/first remote when the branch has no upstream; its chevron menu adds "Fetch all remotes"
 * and the Prune toggle. Pull needs an upstream; Push without one opens the Push dialog to
 * publish the branch (`onPublish`).
 */
export function SyncBar({
  remotes,
  upstream,
  detachedAt,
  operationDisabled,
  operationDisabledReason,
  onFetch,
  onPull,
  onPush,
  prune,
  onFetchAll,
  onTogglePrune,
  onPublish,
  leading,
}: {
  remotes: RemoteInfo[];
  upstream: UpstreamInfo | null;
  detachedAt: string | null;
  operationDisabled: boolean;
  operationDisabledReason: string | null;
  onFetch: (remoteName: string) => void | Promise<void>;
  onPull: () => void | Promise<void>;
  onPush: (remoteName: string) => void | Promise<void>;
  /** What the Prune checkbox shows (the saved choice, else the fetch target's git config). */
  prune: boolean;
  onFetchAll: () => void | Promise<void>;
  onTogglePrune: (prune: boolean) => void;
  /** Push on a branch without an upstream: open the Push dialog for the current branch. */
  onPublish: () => void;
  leading?: ReactNode;
}) {
  const [fetchMenu, setFetchMenu] = useState<{ x: number; y: number } | null>(null);
  const fetchRemote = upstream?.remoteName ?? remotes[0]?.name ?? null;
  const busyTitle = operationDisabled ? (operationDisabledReason ?? undefined) : undefined;
  const fetchTitle = busyTitle ?? (fetchRemote === null ? "No remote configured." : `Fetch from ${fetchRemote}`);
  const upstreamTitle = (verb: string) =>
    busyTitle ?? (upstream === null ? "No upstream set for the current branch." : `${verb} ${upstream.remoteName}/${upstream.remoteBranch}`);
  const detached = detachedAt !== null;
  const detachedTitle = "Check out a branch to push or pull.";
  const showCounts = !detached && upstream !== null && upstream.ahead != null && upstream.behind != null;
  const fetchDisabled = operationDisabled || fetchRemote === null;
  const canPublish = !detached && upstream === null && remotes.length > 0;
  const pushTitle = detached
    ? detachedTitle
    : upstream !== null
      ? upstreamTitle("Push to")
      : (busyTitle ?? (canPublish ? "Push this branch to a remote and set its upstream…" : "No remote configured."));

  return (
    <div className={styles.bar}>
      {leading}
      <Toolbar aria-label="Sync">
        <span className={styles.splitGroup}>
          <button
            type="button"
            disabled={fetchDisabled}
            title={fetchTitle}
            onClick={() => fetchRemote !== null && void onFetch(fetchRemote)}
          >
            Fetch
          </button>
          <button
            type="button"
            className={styles.split}
            aria-label="More fetch actions"
            aria-haspopup="menu"
            aria-expanded={fetchMenu !== null}
            disabled={fetchDisabled}
            title={busyTitle}
            onClick={(event) => {
              const rect = event.currentTarget.getBoundingClientRect();
              setFetchMenu({ x: rect.left, y: rect.bottom });
            }}
          >
            <ChevronDown size={12} aria-hidden="true" />
          </button>
        </span>
        <button
          type="button"
          disabled={operationDisabled || detached || upstream === null}
          title={detached ? detachedTitle : upstreamTitle("Pull from")}
          onClick={() => void onPull()}
        >
          Pull
        </button>
        <button
          type="button"
          disabled={operationDisabled || detached || (upstream === null && !canPublish)}
          title={pushTitle}
          onClick={() => (upstream !== null ? void onPush(upstream.remoteName) : onPublish())}
        >
          Push
        </button>
      </Toolbar>
      {fetchMenu !== null && (
        <ContextMenu
          x={fetchMenu.x}
          y={fetchMenu.y}
          onClose={() => setFetchMenu(null)}
          items={[
            { label: "Fetch all remotes", onSelect: () => void onFetchAll() },
            {
              label: "Prune",
              checked: prune,
              title: "Remove remote-tracking branches that were deleted on the remote",
              onSelect: () => onTogglePrune(!prune),
            },
          ]}
        />
      )}
      {detached && <span>HEAD detached at {detachedAt}</span>}
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

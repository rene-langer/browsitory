import { useEffect, useState, type ReactNode } from "react";
import { autoFetchLabel } from "../lib/autoFetch";
import { formatRelativeDate } from "../lib/formatDate";
import type { RemoteInfo, UpstreamInfo } from "../ipc/RepoClient";
import { upstreamPushBranch } from "../lib/pushTarget";
import { SplitButton } from "./primitives/SplitButton";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./SyncBar.module.css";

/**
 * The repository toolbar's row (#637): `leading` (the sidebar toggle, the branch chip, Back and
 * Forward), then the three daily sync actions with the current
 * branch's ahead/behind counts against its (last fetched) upstream. Fetch falls back to the
 * only/first remote when the branch has no upstream; its chevron menu adds "Fetch all remotes"
 * and the Prune toggle. Pull needs an upstream; Push without one opens the Push dialog to
 * publish the branch (`onPublish`). Push with an upstream goes where `push.default` resolves;
 * when it resolves nothing, `onPush` opens the Push dialog instead (see `pushCurrentBranch`).
 * Then Stash (`stash`), `trailing` (Undo/Redo, Refresh) and the last fetch. Pull waits while a
 * diverged pull's prompt is open (`pullPending`, #636).
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
  trailing,
  stash,
  pullPending = false,
  ariaLabel,
  lastFetchedAt = null,
  autoFetchMinutes = 0,
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
  trailing?: ReactNode;
  /** Stash, with "Stash all" and "Stash…" in its menu; `reason` (`stashBlockReason`) disables it. */
  stash?: { reason: string | null; onStash: () => void; onStashWithOptions: () => void };
  /** A diverged pull waits for Merge, Rebase or Cancel: Pull is disabled meanwhile. */
  pullPending?: boolean;
  /** Names the row as a group ("Repository"). */
  ariaLabel?: string;
  /** When this repository was last fetched in this session (ms since the epoch), if at all. */
  lastFetchedAt?: number | null;
  /** The effective "Fetch automatically" interval, for the last-fetch tooltip; 0 is off. */
  autoFetchMinutes?: number;
}) {
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
  const pushBranch = upstream === null ? null : upstreamPushBranch(upstream);
  const pushTitle = detached
    ? detachedTitle
    : upstream !== null
      ? (busyTitle ??
        (pushBranch === null
          ? `Choose where to push ${upstream.localBranch} on ${upstream.remoteName}…`
          : `Push to ${upstream.remoteName}/${pushBranch}`))
      : (busyTitle ?? (canPublish ? "Push this branch to a remote and set its upstream…" : "No remote configured."));

  return (
    <div className={styles.bar} role={ariaLabel === undefined ? undefined : "group"} aria-label={ariaLabel}>
      {leading}
      <Toolbar aria-label="Sync">
        <SplitButton
          label="Fetch"
          disabled={fetchDisabled}
          title={fetchTitle}
          menuTitle={busyTitle}
          onClick={() => fetchRemote !== null && void onFetch(fetchRemote)}
          menuLabel="More fetch actions"
          chevronClassName={styles.split}
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
        <button
          type="button"
          disabled={operationDisabled || detached || upstream === null || pullPending}
          title={
            detached
              ? detachedTitle
              : pullPending && !operationDisabled
                ? "Choose how to finish the diverged pull first."
                : upstreamTitle("Pull from")
          }
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
      {stash !== undefined && (
        // Stashing mid-rebase is destructive in a way nothing else undoes: a paused step's
        // resolved/amended content lives in the working tree, so stashing it away and continuing
        // lands an empty (or wrong) commit. Disabled for the whole pause by the stash rule the
        // file menu and the command palette share (`stashBlockReason`, #495).
        <SplitButton
          label="Stash"
          onClick={stash.onStash}
          disabled={stash.reason !== null}
          title={stash.reason ?? "Stash all changes"}
          menuLabel="More stash options"
          chevronClassName={styles.split}
          items={[
            { label: "Stash all", onSelect: () => stash.onStash() },
            { label: "Stash…", onSelect: () => stash.onStashWithOptions() },
          ]}
        />
      )}
      {trailing}
      {lastFetchedAt !== null && <LastFetch at={lastFetchedAt} autoFetchMinutes={autoFetchMinutes} />}
    </div>
  );
}

/** "Fetched 5 minutes ago", refreshed every 30 seconds. */
function LastFetch({ at, autoFetchMinutes }: { at: number; autoFetchMinutes: number }) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 30_000);
    return () => clearInterval(timer);
  }, []);
  // A newer fetch than the last tick (`now - at` < 0) is "just now" too.
  const label = now - at < 60_000 ? "just now" : formatRelativeDate(at / 1000, now, "en");
  const when = `Last fetch: ${new Date(at).toLocaleString()}`;
  const title = autoFetchMinutes > 0 ? `${when}. Fetches automatically ${autoFetchLabel(autoFetchMinutes).toLowerCase()}.` : when;
  return (
    <span className={styles.lastFetch} title={title}>
      Fetched {label}
    </span>
  );
}

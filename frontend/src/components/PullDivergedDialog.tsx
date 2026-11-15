import { useId, useState } from "react";
import { describedBy } from "../lib/ariaDescribedBy";
import { shortRefName } from "../lib/shortRefName";
import { BlockedReason } from "./primitives/BlockedReason";
import { DialogActions } from "./primitives/DialogActions";
import { FormDialog } from "./primitives/FormDialog";
import styles from "./PullDivergedDialog.module.css";

const commits = (count: number) => `${count} ${count === 1 ? "commit" : "commits"}`;

/**
 * A pull whose branch and upstream have diverged and no strategy says how to combine them: "Pull
 * with merge", "Pull with rebase" (a plain rebase, #652) or Cancel, named as in the Pull menu, with
 * "Always do this for <repository>" (#1019). Shown when neither a remembered choice nor
 * `pull.rebase` decides, or when `pull.ff=only` (or the remembered "Fast-forward only") stopped
 * the pull. Rendered by `WorkspaceDialogs`, not the sidebar: the Branches section mounts its
 * content only while open, and the sidebar is `hidden` while collapsed, where a modal would be
 * invisible (#636).
 */
export function PullDivergedDialog({
  upstreamRef,
  branchName,
  repoName = null,
  ahead = null,
  behind = null,
  fastForwardOnly = false,
  operationDisabled,
  operationDisabledReason,
  onMerge,
  onRebase,
  onCancel,
}: {
  upstreamRef: string;
  /** The current branch; `null` reads "Your branch". */
  branchName?: string | null;
  /** The repository's folder name, for "Always do this for …"; `null` reads "this repository". */
  repoName?: string | null;
  /** Commits on the branch and not on the upstream, and the other way round; `null` when unknown. */
  ahead?: number | null;
  behind?: number | null;
  /** The pull stopped because only a fast-forward is allowed. */
  fastForwardOnly?: boolean;
  operationDisabled: boolean;
  operationDisabledReason: string | null;
  /** `remember`: keep this choice as the repository's pull strategy. */
  onMerge: (upstreamRef: string, remember: boolean) => void;
  onRebase: (upstreamRef: string, remember: boolean) => void;
  onCancel: () => void;
}) {
  const [remember, setRemember] = useState(false);
  const reasonId = useId();
  // Neither choice is the obvious one, so neither button is marked primary.
  const reason = operationDisabled ? operationDisabledReason : null;
  const title = operationDisabled ? (operationDisabledReason ?? undefined) : undefined;
  const upstream = shortRefName(upstreamRef);
  const branch = branchName ?? "Your branch";
  const local = branchName ?? "your branch";
  const counts =
    ahead !== null && behind !== null && ahead > 0 && behind > 0
      ? `${branch} has ${commits(ahead)} that ${upstream} doesn't, and ${upstream} has ${behind} that ${local} doesn't.`
      : null;
  return (
    <FormDialog title={`${branch} and ${upstream} have diverged`} description={counts ?? undefined} onCancel={onCancel}>
      <div className={styles.body}>
        <p className={styles.message}>
          {fastForwardOnly && `${branch} can't be fast-forwarded to ${upstream}: both have commits the other doesn't. `}
          Pull with merge combines them in a merge commit; Pull with rebase replays the commits of {local} on top
          of {upstream}.
        </p>
        <label className={styles.remember}>
          <input type="checkbox" checked={remember} onChange={(event) => setRemember(event.target.checked)} />
          {`Always do this for ${repoName ?? "this repository"} (change it in the Pull menu)`}
        </label>
        <BlockedReason id={reasonId} reason={reason} />
        <DialogActions>
          <button
            type="button"
            disabled={operationDisabled}
            title={title}
            aria-describedby={describedBy(reasonId, reason)}
            onClick={() => onMerge(upstreamRef, remember)}
          >
            Pull with merge
          </button>
          <button
            type="button"
            disabled={operationDisabled}
            title={title}
            aria-describedby={describedBy(reasonId, reason)}
            onClick={() => onRebase(upstreamRef, remember)}
          >
            Pull with rebase
          </button>
          <button type="button" data-autofocus onClick={onCancel}>
            Cancel
          </button>
        </DialogActions>
      </div>
    </FormDialog>
  );
}

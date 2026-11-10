import { useState } from "react";
import type { PullStrategy } from "../ipc/RepoClient";
import { FormDialog } from "./primitives/FormDialog";
import styles from "./PullDivergedDialog.module.css";

/** `refs/remotes/origin/main` → `origin/main`, as the branch list shows it. */
export function shortRefName(ref: string): string {
  return ref.replace(/^refs\/(remotes|heads)\//, "");
}

/**
 * A pull whose branch and upstream have diverged and no strategy says how to combine them: Merge,
 * Rebase (a plain rebase, #652) or Cancel, with "Remember for this repository". Shown when
 * neither a remembered choice nor `pull.rebase` decides, or when `pull.ff=only` (or the remembered
 * "Fast-forward only") stopped the pull. Rendered by `WorkspaceDialogs`, not the sidebar: the
 * Branches section mounts its content only while open, and the sidebar is `hidden` while
 * collapsed, where a modal would be invisible (#636).
 */
export function PullDivergedDialog({
  upstreamRef,
  branchName,
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
  const title = operationDisabled ? (operationDisabledReason ?? undefined) : undefined;
  const upstream = shortRefName(upstreamRef);
  const branch = branchName ?? "Your branch";
  return (
    <FormDialog ariaLabel="Pull has diverged" onCancel={onCancel}>
      <div className={styles.body}>
        <p className={styles.message}>
          {fastForwardOnly
            ? `${branch} can't be fast-forwarded to ${upstream}: both have commits the other doesn't.`
            : `${branch} and ${upstream} have both changed.`}{" "}
          Merge {upstream} into it, or rebase its commits onto {upstream}?
        </p>
        <label className={styles.remember}>
          <input type="checkbox" checked={remember} onChange={(event) => setRemember(event.target.checked)} />
          Remember for this repository
        </label>
        <div className={styles.actions}>
          <button
            type="button"
            disabled={operationDisabled}
            title={title}
            onClick={() => onMerge(upstreamRef, remember)}
          >
            Merge
          </button>
          <button
            type="button"
            disabled={operationDisabled}
            title={title}
            onClick={() => onRebase(upstreamRef, remember)}
          >
            Rebase
          </button>
          <button type="button" data-autofocus onClick={onCancel}>
            Cancel
          </button>
        </div>
      </div>
    </FormDialog>
  );
}

/** The Pull menu's names for the strategies (#652). */
export const PULL_STRATEGY_LABELS: Record<PullStrategy, string> = {
  Merge: "Merge",
  Rebase: "Rebase",
  FastForwardOnly: "Fast-forward only",
};

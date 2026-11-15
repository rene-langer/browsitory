import { useEffect, useState, useId } from "react";
import type { FastForwardMode, MergeOptions } from "../ipc/RepoClient";
import { describedBy } from "../lib/ariaDescribedBy";
import { BlockedReason } from "./primitives/BlockedReason";
import { DialogActions } from "./primitives/DialogActions";
import { FormDialog } from "./primitives/FormDialog";
import styles from "./MergeDialog.module.css";

const FAST_FORWARD_CHOICES: Array<{ mode: FastForwardMode; label: string }> = [
  { mode: "Allow", label: "Fast-forward if possible" },
  { mode: "Never", label: "Always create a merge commit" },
  { mode: "Only", label: "Fast-forward only" },
];

const FALLBACK: MergeOptions = { fastForward: "Allow", squash: false };

/**
 * "Merge into current branch…": how to merge `branchName` into the current branch. Preselects
 * `loadOptions()` (the choice remembered for this repo, else `merge.ff`); Merge stays disabled
 * until it resolves. Squash ignores the fast-forward choice, so the radios are disabled then and
 * the hint under Squash says so (#1018).
 * `blockedReason` (`operationBlockers`) disables Merge while it applies: the repository can
 * become busy after the dialog opened (#514).
 */
export function MergeDialog({
  branchName,
  currentBranchName,
  loadOptions,
  onMerge,
  onCancel,
  blockedReason = null,
}: {
  branchName: string;
  currentBranchName: string;
  loadOptions: () => Promise<MergeOptions>;
  onMerge: (options: MergeOptions) => void;
  onCancel: () => void;
  /** Why repository operations are blocked now, or `null`. */
  blockedReason?: string | null;
}) {
  // The visible reason the submit button is disabled is its description (#664).
  const reasonId = useId();
  const squashHintId = useId();
  const [options, setOptions] = useState<MergeOptions | null>(null);

  useEffect(() => {
    let ignore = false;
    loadOptions().then(
      (loaded) => {
        if (!ignore) setOptions(loaded);
      },
      () => {
        if (!ignore) setOptions(FALLBACK);
      },
    );
    return () => {
      ignore = true;
    };
  }, [loadOptions]);

  const current = options ?? FALLBACK;

  return (
    <FormDialog title={`Merge ${branchName} into ${currentBranchName}`} onCancel={onCancel}>
      <form
        className={styles.form}
        onSubmit={(event) => {
          event.preventDefault();
          if (options !== null && blockedReason === null) onMerge(options);
        }}
      >
        <fieldset className={styles.choices} disabled={options === null || current.squash}>
          <legend>How to merge</legend>
          {FAST_FORWARD_CHOICES.map((choice) => (
            <label key={choice.mode} className={styles.choice}>
              <input
                type="radio"
                name="fast-forward"
                value={choice.mode}
                checked={current.fastForward === choice.mode}
                onChange={() => setOptions({ ...current, fastForward: choice.mode })}
              />
              {choice.label}
            </label>
          ))}
        </fieldset>
        {/* The hint sits right under its checkbox, indented to its text (#1018). */}
        <div className={styles.squash}>
          <label className={styles.choice}>
            <input
              type="checkbox"
              checked={current.squash}
              disabled={options === null}
              aria-describedby={squashHintId}
              onChange={(event) => setOptions({ ...current, squash: event.target.checked })}
            />
            Squash
          </label>
          <p id={squashHintId} className={styles.hint}>
            Stages the combined changes; you write the commit. The choices above don&apos;t apply.
          </p>
        </div>
        <BlockedReason id={reasonId} reason={blockedReason} />
        <DialogActions>
          <button
            type="submit"
            data-tone="primary"
            disabled={options === null || blockedReason !== null}
            title={blockedReason ?? undefined}
            aria-describedby={describedBy(reasonId, blockedReason)}
          >
            Merge
          </button>
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
        </DialogActions>
      </form>
    </FormDialog>
  );
}

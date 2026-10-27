import { useEffect, useState } from "react";
import type { FastForwardMode, MergeOptions } from "../ipc/RepoClient";
import { FormDialog } from "./primitives/FormDialog";
import { Toolbar } from "./primitives/Toolbar";
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
 * until it resolves. Squash ignores the fast-forward choice, so the radios are disabled then.
 */
export function MergeDialog({
  branchName,
  currentBranchName,
  loadOptions,
  onMerge,
  onCancel,
}: {
  branchName: string;
  currentBranchName: string;
  loadOptions: () => Promise<MergeOptions>;
  onMerge: (options: MergeOptions) => void;
  onCancel: () => void;
}) {
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
    <FormDialog ariaLabel={`Merge ${branchName}`} onCancel={onCancel}>
      <form
        className={styles.form}
        onSubmit={(event) => {
          event.preventDefault();
          if (options !== null) onMerge(options);
        }}
      >
        <h2 className={styles.heading}>
          Merge {branchName} into {currentBranchName}
        </h2>
        <fieldset className={styles.choices} disabled={options === null || current.squash}>
          <legend>Fast-forward</legend>
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
        <label className={styles.choice}>
          <input
            type="checkbox"
            checked={current.squash}
            disabled={options === null}
            aria-describedby="merge-dialog-squash-hint"
            onChange={(event) => setOptions({ ...current, squash: event.target.checked })}
          />
          Squash
        </label>
        <p id="merge-dialog-squash-hint" className={styles.hint}>
          Stage the combined changes; you write the commit.
        </p>
        <Toolbar>
          <button type="submit" disabled={options === null}>
            Merge
          </button>
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
        </Toolbar>
      </form>
    </FormDialog>
  );
}

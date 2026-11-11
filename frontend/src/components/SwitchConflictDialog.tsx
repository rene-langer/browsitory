import { useId, useState } from "react";
import { FormDialog } from "./primitives/FormDialog";
import styles from "./SwitchConflictDialog.module.css";
import { describedBy } from "../lib/ariaDescribedBy";

/** How many of the files in the way the dialog lists before "and N more". */
const SHOWN_FILES = 8;

/**
 * A switch that local changes refused (#653): names the files in the way and offers "Stash and
 * switch", which stashes every local change, switches, and with "Re-apply" (on by default) puts
 * the changes back on the new branch. One undo step goes back to the old branch with the
 * changes; a stash it left behind stays in the stash list.
 */
export function SwitchConflictDialog({
  branchName,
  currentBranch,
  files,
  operationDisabled,
  operationDisabledReason,
  onStashAndSwitch,
  onCancel,
}: {
  branchName: string;
  currentBranch: string | null;
  files: string[];
  operationDisabled: boolean;
  operationDisabledReason: string | null;
  onStashAndSwitch: (reapply: boolean) => void;
  onCancel: () => void;
}) {
  const reasonId = useId();
  const busyReason = operationDisabled ? operationDisabledReason : null;
  const [reapply, setReapply] = useState(true);
  const shown = files.slice(0, SHOWN_FILES);
  const more = files.length - shown.length;
  return (
    <FormDialog ariaLabel="Local changes are in the way" onCancel={onCancel}>
      <div className={styles.body}>
        <p className={styles.message}>
          {files.length === 1
            ? `Switching to ${branchName} would overwrite your changes to this file:`
            : files.length > 1
              ? `Switching to ${branchName} would overwrite your changes to these files:`
              : `Switching to ${branchName} would overwrite your local changes.`}
        </p>
        {shown.length > 0 && (
          <ul className={styles.files} aria-label="Files in the way">
            {shown.map((file) => (
              <li key={file}>{file}</li>
            ))}
            {more > 0 && <li className={styles.more}>and {more} more</li>}
          </ul>
        )}
        <label className={styles.choice}>
          <input type="checkbox" checked={reapply} onChange={(event) => setReapply(event.target.checked)} />
          Re-apply the changes on {branchName}
        </label>
        <p className={styles.hint}>
          {reapply
            ? "Changes that conflict there are left to resolve, and stay in the stash list too."
            : "The changes stay in the stash list, to apply later."}{" "}
          Undo goes back to {currentBranch ?? "the current branch"} with them.
        </p>
        {busyReason !== null && (
          <p id={reasonId} className={styles.hint}>
            {busyReason}
          </p>
        )}
        <div className={styles.actions}>
          <button
            type="button"
            disabled={operationDisabled}
            title={busyReason ?? undefined}
            aria-describedby={describedBy(reasonId, busyReason)}
            onClick={() => onStashAndSwitch(reapply)}
          >
            Stash and switch
          </button>
          <button type="button" data-autofocus onClick={onCancel}>
            Cancel
          </button>
        </div>
      </div>
    </FormDialog>
  );
}

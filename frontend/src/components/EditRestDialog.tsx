import { useId } from "react";
import type { EditRest } from "../ipc/RepoClient";
import { describedBy } from "../lib/ariaDescribedBy";
import { FormDialog } from "./primitives/FormDialog";
import styles from "./SwitchConflictDialog.module.css";

/** How many of the files the dialog lists before "and N more". */
const SHOWN_FILES = 8;

/**
 * Continue at an edit stop with changes not staged (#884): names the files and asks whether to
 * commit them with the commit being edited (the last part of a split), discard them (Undo of
 * the rebase brings them back), or go back and stage what belongs where.
 */
export function EditRestDialog({
  files,
  operationDisabled,
  operationDisabledReason,
  onChoose,
  onCancel,
}: {
  files: string[];
  operationDisabled: boolean;
  operationDisabledReason: string | null;
  onChoose: (rest: EditRest) => void;
  onCancel: () => void;
}) {
  const reasonId = useId();
  const busyReason = operationDisabled ? operationDisabledReason : null;
  const shown = files.slice(0, SHOWN_FILES);
  const more = files.length - shown.length;
  const button = (rest: EditRest, label: string) => (
    <button
      type="button"
      disabled={operationDisabled}
      title={busyReason ?? undefined}
      aria-describedby={describedBy(reasonId, busyReason)}
      onClick={() => onChoose(rest)}
    >
      {label}
    </button>
  );
  return (
    <FormDialog ariaLabel="Changes not staged" onCancel={onCancel}>
      <div className={styles.body}>
        <p className={styles.message}>
          {files.length === 1
            ? "This file has changes that aren't staged or committed:"
            : files.length > 1
              ? "These files have changes that aren't staged or committed:"
              : "Some changes aren't staged or committed."}
        </p>
        {shown.length > 0 && (
          <ul className={styles.files} aria-label="Files with changes not staged">
            {shown.map((file) => (
              <li key={file}>{file}</li>
            ))}
            {more > 0 && <li className={styles.more}>and {more} more</li>}
          </ul>
        )}
        <p className={styles.hint}>
          Commit them with this commit, or discard them; Undo of the rebase brings them back.
        </p>
        {busyReason !== null && (
          <p id={reasonId} className={styles.hint}>
            {busyReason}
          </p>
        )}
        <div className={styles.actions}>
          {button("Commit", "Commit the rest")}
          {button("Discard", "Discard the rest")}
          <button type="button" data-autofocus onClick={onCancel}>
            Cancel
          </button>
        </div>
      </div>
    </FormDialog>
  );
}

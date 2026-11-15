import { useId, useState } from "react";
import { branchNameProblem } from "../lib/refName";
import { refNameClash, takenNameMessage } from "../lib/takenNames";
import { describedBy } from "../lib/ariaDescribedBy";
import { BlockedReason } from "./primitives/BlockedReason";
import { DialogActions } from "./primitives/DialogActions";
import { Field } from "./primitives/Field";
import { FormDialog } from "./primitives/FormDialog";
import { InlineError } from "./primitives/InlineError";
import styles from "./PushBranchDialog.module.css";

/**
 * Renames a local branch (#1002): the branch menu's "Rename…", like Rename tab. The name is
 * checked as you type, as in the New branch form (#418): an invalid name or one another branch
 * has (also as a folder) never reaches the backend. An unchanged name just closes. A failure
 * stays in the dialog with what was typed (#422).
 */
export function RenameBranchDialog({
  name,
  branchNames,
  disabled,
  disabledReason,
  onRename,
  onCancel,
}: {
  /** The branch being renamed. */
  name: string;
  /** Every local branch's name, for the taken-name check. */
  branchNames: readonly string[];
  /** A repository operation is running: renaming waits until it ends. */
  disabled: boolean;
  disabledReason: string | null;
  /** Resolves to `null` once renamed (the dialog closes), or to the failure, shown inline. */
  onRename: (newName: string) => Promise<string | null>;
  onCancel: () => void;
}) {
  const reasonId = useId();
  const [value, setValue] = useState(name);
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const busyReason = disabled ? disabledReason : null;

  const newName = value.trim();
  const clash =
    newName === "" || newName === name
      ? null
      : refNameClash(
          branchNames.filter((candidate) => candidate !== name),
          newName,
        );
  const problem =
    newName === "" ? null : (branchNameProblem(newName) ?? (clash === null ? null : takenNameMessage("branch", newName, clash)));
  const blocked = disabled || submitting || newName === "" || problem !== null;
  // Why Rename is off, in view under the form (#1014). A name problem shows at its field.
  const reason = busyReason ?? (newName === "" ? "Enter a branch name." : null);

  return (
    <FormDialog title="Rename branch" description={<code>{name}</code>} onCancel={onCancel}>
      <form
        className={styles.form}
        onSubmit={(event) => {
          event.preventDefault();
          if (blocked) return;
          if (newName === name) {
            onCancel();
            return;
          }
          setSubmitting(true);
          void onRename(newName).then((failure) => {
            setSubmitting(false);
            setError(failure);
          });
        }}
      >
        <Field label="New name" error={problem}>
          {(props) => (
            <input
              {...props}
              data-autofocus
              value={value}
              autoComplete="off"
              spellCheck={false}
              onFocus={(event) => event.currentTarget.select()}
              onChange={(event) => {
                setValue(event.target.value);
                setError(null);
              }}
            />
          )}
        </Field>
        {error !== null && <InlineError message={error} onDismiss={() => setError(null)} />}
        <BlockedReason id={reasonId} reason={reason} />
        <DialogActions>
          <button
            type="submit"
            data-tone="primary"
            disabled={blocked}
            title={busyReason ?? undefined}
            aria-describedby={describedBy(reasonId, reason)}
          >
            Rename
          </button>
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
        </DialogActions>
      </form>
    </FormDialog>
  );
}

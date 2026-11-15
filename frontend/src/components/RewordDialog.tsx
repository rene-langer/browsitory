import { useEffect, useId, useState } from "react";
import { Field } from "./primitives/Field";
import { DialogActions } from "./primitives/DialogActions";
import { FormDialog } from "./primitives/FormDialog";
import { InlineError } from "./primitives/InlineError";
import styles from "./PushBranchDialog.module.css";
import { BlockedReason } from "./primitives/BlockedReason";
import { describedBy } from "../lib/ariaDescribedBy";

/**
 * "Reword…" of a commit (#442): edits its full message, then the caller runs it as a one-step
 * rebase, or for HEAD (`head`) as an amend of the message only that leaves staged changes
 * staged (#981).
 */
export function RewordDialog({
  shortId,
  head = false,
  loadMessage,
  disabled,
  disabledReason,
  onReword,
  onCancel,
}: {
  shortId: string;
  /** The latest commit: only its message changes, nothing after it is rewritten (#981). */
  head?: boolean;
  /** The commit's full message, to start from. */
  loadMessage: () => Promise<string>;
  /** A repository operation is running: rewording is blocked until it ends. */
  disabled: boolean;
  disabledReason: string | null;
  /** Called with the new message, trailing whitespace trimmed and one final newline. */
  onReword: (message: string) => void;
  onCancel: () => void;
}) {
  const reasonId = useId();
  const busyReason = disabled ? disabledReason : null;
  const [original, setOriginal] = useState<string | null>(null);
  const [message, setMessage] = useState("");
  const [loadError, setLoadError] = useState<string | null>(null);

  useEffect(() => {
    let ignore = false;
    loadMessage().then(
      (loaded) => {
        if (ignore) return;
        setOriginal(loaded.trimEnd());
        setMessage(loaded.trimEnd());
      },
      (error: unknown) => {
        if (!ignore) setLoadError(error instanceof Error ? error.message : String(error));
      },
    );
    return () => {
      ignore = true;
    };
  }, [loadMessage]);

  const next = message.trimEnd();
  const blocked = disabled || original === null || next.trim() === "" || next === original;

  return (
    <FormDialog
      title={`Reword commit ${shortId}`}
      description={
        head
          ? "Only the message changes: staged changes stay staged and out of the commit."
          : "The commits after it are rewritten too."
      }
      onCancel={onCancel}
    >
      <form
        className={styles.form}
        onSubmit={(event) => {
          event.preventDefault();
          if (!blocked) onReword(`${next}\n`);
        }}
      >
        <Field label="Commit message">
          {(props) => (
            <textarea
              {...props}
              data-autofocus
              rows={8}
              value={message}
              // Read-only, not disabled, while loading: the dialog focuses it when it opens.
              readOnly={original === null}
              spellCheck
              onChange={(event) => setMessage(event.target.value)}
            />
          )}
        </Field>
        {loadError !== null && <InlineError message={loadError} onDismiss={() => setLoadError(null)} />}
        <BlockedReason id={reasonId} reason={busyReason} />
        <DialogActions>
          <button
            type="submit"
            data-tone="primary"
            disabled={blocked}
            title={busyReason ?? undefined}
            aria-describedby={describedBy(reasonId, busyReason)}
          >
            Reword
          </button>
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
        </DialogActions>
      </form>
    </FormDialog>
  );
}

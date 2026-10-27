import type { HookFailure } from "../lib/hookFailure";
import { hookFailedMessage, retryLabel } from "../lib/hookFailure";
import { ConfirmDialog } from "./primitives/ConfirmDialog";
import styles from "./HookFailureDialog.module.css";

/** A failed gate hook's output, with a retry that skips the hooks `--no-verify` skips. */
export function HookFailureDialog({
  failure,
  onRetry,
  onClose,
}: {
  failure: HookFailure;
  onRetry: () => void;
  onClose: () => void;
}) {
  const label = retryLabel(failure);
  return (
    <ConfirmDialog
      ariaLabel={`${failure.hook} hook failed`}
      message={
        <>
          <p>{hookFailedMessage(failure.hook)}</p>
          <pre className={styles.output} aria-label="Hook output" tabIndex={0}>
            {failure.output === "" ? "(no output)" : failure.output}
          </pre>
        </>
      }
      // A hook --no-verify cannot skip (prepare-commit-msg) gets no retry: it would fail again.
      confirmLabel={label ?? ""}
      hideConfirm={label === null}
      cancelLabel="Close"
      onConfirm={onRetry}
      onCancel={onClose}
    />
  );
}

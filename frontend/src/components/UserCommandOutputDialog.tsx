import type { UserCommandRun } from "../ipc/RepoClient";
import type { UserCommandOutput } from "../state/useUserCommandRunner";
import { DialogActions } from "./primitives/DialogActions";
import { FormDialog } from "./primitives/FormDialog";
import styles from "./UserCommandOutputDialog.module.css";

function statusText(result: UserCommandRun): string {
  if (result.cancelled) return "Stopped";
  if (result.timedOut) return "Took too long and was stopped";
  if (result.success) return "Finished";
  return result.exitCode === null ? "Failed" : `Failed with exit code ${result.exitCode}`;
}

/**
 * A user command's output (#895): "Running…" with Stop while it runs, then how it ended and the
 * end of what it printed (stdout and stderr together), or why it couldn't start.
 */
export function UserCommandOutputDialog({
  output,
  onStop,
  onClose,
}: {
  output: UserCommandOutput;
  onStop: () => void;
  onClose: () => void;
}) {
  const { command, result, error } = output;
  const running = result === null && error === null;
  return (
    <FormDialog title={command.name} onCancel={onClose} className={styles.dialog}>
      {error !== null ? (
        <p role="alert" className={styles.problem}>
          {error}
        </p>
      ) : (
        <p role="status" className={styles.status}>
          {result === null ? "Running…" : statusText(result)}
        </p>
      )}
      {result !== null &&
        (result.output === "" ? (
          <p className={styles.muted}>No output</p>
        ) : (
          <pre className={styles.output} tabIndex={0}>
            {result.output}
          </pre>
        ))}
      <DialogActions>
        {running && (
          <button type="button" onClick={onStop}>
            Stop
          </button>
        )}
        <button type="button" data-autofocus onClick={onClose}>
          Close
        </button>
      </DialogActions>
    </FormDialog>
  );
}

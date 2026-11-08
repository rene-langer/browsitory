import { useState } from "react";
import type { RemoteInfo } from "../ipc/RepoClient";
import { Field } from "./primitives/Field";
import { FormDialog } from "./primitives/FormDialog";
import confirmStyles from "./primitives/ConfirmDialog.module.css";
import styles from "./PushBranchDialog.module.css";

/**
 * "Delete from remote…" for a tag (Tags panel, tag badge menu): picks the remote when there are
 * several, and can delete the local tag as well. The remote delete is not undoable, like every
 * network operation; the local delete (done only once the remote one landed) is.
 */
export function DeleteRemoteTagDialog({
  tagName,
  remotes,
  disabled,
  disabledReason,
  onDelete,
  onCancel,
}: {
  tagName: string;
  remotes: RemoteInfo[];
  disabled: boolean;
  disabledReason: string | null;
  onDelete: (remoteName: string, deleteLocal: boolean) => void;
  onCancel: () => void;
}) {
  const [remoteName, setRemoteName] = useState(() =>
    remotes.some((remote) => remote.name === "origin") ? "origin" : (remotes[0]?.name ?? ""),
  );
  const [deleteLocal, setDeleteLocal] = useState(false);

  return (
    <FormDialog ariaLabel={`Delete tag ${tagName} from remote`} onCancel={onCancel}>
      <div className={styles.form}>
        {remotes.length > 1 && (
          <Field label="Remote">
            {(props) => (
              <select {...props} value={remoteName} onChange={(event) => setRemoteName(event.target.value)}>
                {remotes.map((remote) => (
                  <option key={remote.name} value={remote.name}>
                    {remote.name}
                  </option>
                ))}
              </select>
            )}
          </Field>
        )}
        {remotes.length === 0 ? (
          <p className={styles.problem} role="alert">
            No remote configured.
          </p>
        ) : (
          <p className={styles.message}>
            Delete tag {tagName} on {remoteName}? This can&apos;t be undone.
          </p>
        )}
        <label className={styles.checkbox}>
          <input type="checkbox" checked={deleteLocal} onChange={(event) => setDeleteLocal(event.target.checked)} />
          Also delete the local tag
        </label>
        <div className={styles.actions}>
          <button
            type="button"
            className={confirmStyles.confirmButton}
            disabled={disabled || remoteName === ""}
            title={disabled ? (disabledReason ?? undefined) : undefined}
            onClick={() => onDelete(remoteName, deleteLocal)}
          >
            Delete from remote
          </button>
          <button type="button" data-autofocus onClick={onCancel}>
            Cancel
          </button>
        </div>
      </div>
    </FormDialog>
  );
}

import { useEffect, useState } from "react";
import type { RemoteInfo } from "../ipc/RepoClient";
import { Field } from "./primitives/Field";
import { FormDialog } from "./primitives/FormDialog";
import confirmStyles from "./primitives/ConfirmDialog.module.css";
import styles from "./PushBranchDialog.module.css";

/**
 * Deletes a branch on a remote. With `initialRemote` and `initialBranch` both set (a remote
 * branch's own menu) it is a plain confirmation; otherwise (the palette) it first lets the user
 * pick a remote and one of its branches. Not undoable, like every network operation.
 */
export function DeleteRemoteBranchDialog({
  remotes,
  initialRemote,
  initialBranch,
  onListRemoteBranches,
  onDelete,
  onCancel,
}: {
  remotes: RemoteInfo[];
  initialRemote: string | null;
  initialBranch: string | null;
  onListRemoteBranches: (remoteName: string) => Promise<string[]>;
  onDelete: (remoteName: string, branchName: string) => void;
  onCancel: () => void;
}) {
  const preset = initialRemote !== null && initialBranch !== null;
  const [remoteName, setRemoteName] = useState(initialRemote ?? remotes[0]?.name ?? "");
  const [branchName, setBranchName] = useState(initialBranch ?? "");
  const [options, setOptions] = useState<string[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);

  useEffect(() => {
    if (preset || remoteName === "") return;
    let cancelled = false;
    // State is only set in the promise callbacks, never synchronously in the effect.
    onListRemoteBranches(remoteName).then(
      (names) => {
        if (cancelled) return;
        setOptions(names);
        setBranchName(names[0] ?? "");
        setLoadError(null);
      },
      () => {
        if (cancelled) return;
        setLoadError(`Could not list branches for ${remoteName}.`);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [preset, remoteName, onListRemoteBranches]);

  return (
    <FormDialog ariaLabel="Delete remote branch" onCancel={onCancel}>
      <div className={styles.form}>
        {!preset && (
          <>
            <Field label="Remote">
              {(props) => (
                <select
                  {...props}
                  data-autofocus
                  value={remoteName}
                  onChange={(event) => {
                    setOptions(null);
                    setBranchName("");
                    setLoadError(null);
                    setRemoteName(event.target.value);
                  }}
                >
                  {remotes.map((remote) => (
                    <option key={remote.name} value={remote.name}>
                      {remote.name}
                    </option>
                  ))}
                </select>
              )}
            </Field>
            <Field label="Branch">
              {(props) => (
                <select
                  {...props}
                  value={branchName}
                  disabled={options === null}
                  onChange={(event) => setBranchName(event.target.value)}
                >
                  {(options ?? []).map((name) => (
                    <option key={name} value={name}>
                      {name}
                    </option>
                  ))}
                </select>
              )}
            </Field>
            {loadError !== null && (
              <p className={styles.problem} role="alert">
                {loadError}
              </p>
            )}
          </>
        )}
        {branchName !== "" && (
          <p className={styles.message}>
            Delete {remoteName}/{branchName} on the remote? This can&apos;t be undone.
          </p>
        )}
        <div className={styles.actions}>
          <button
            type="button"
            className={confirmStyles.confirmButton}
            disabled={branchName === "" || remoteName === ""}
            onClick={() => onDelete(remoteName, branchName)}
          >
            Delete remote branch
          </button>
          <button type="button" data-autofocus={preset ? true : undefined} onClick={onCancel}>
            Cancel
          </button>
        </div>
      </div>
    </FormDialog>
  );
}

import { useState } from "react";
import type { RemoteInfo } from "../ipc/RepoClient";
import { FormDialog } from "./primitives/FormDialog";
import formStyles from "./primitives/FormDialog.module.css";
import styles from "./BranchTree.module.css";

/**
 * "Set upstream…" for the current branch: a remote and a branch on it, the branch field suggesting
 * the remote's branches once one is chosen. Mounted while open, so each opening starts empty.
 */
export function SetUpstreamDialog({
  currentBranchName,
  remotes,
  operationDisabled,
  busyReason,
  onListRemoteBranches,
  onSetUpstream,
  onClose,
}: {
  currentBranchName: string;
  remotes: RemoteInfo[];
  operationDisabled: boolean;
  /** Why the dialog can't act now (#521), shown in it; `null` when nothing blocks. */
  busyReason: string | null;
  onListRemoteBranches: (remoteName: string) => Promise<string[]>;
  onSetUpstream: (remoteName: string, remoteBranch: string) => Promise<void>;
  onClose: () => void;
}) {
  const [upstreamRemoteField, setUpstreamRemoteField] = useState("");
  const [upstreamBranchField, setUpstreamBranchField] = useState("");
  const [remoteBranchOptions, setRemoteBranchOptions] = useState<string[]>([]);

  return (
    <FormDialog ariaLabel={`Set upstream for ${currentBranchName}`} onCancel={onClose}>
      <form
        onSubmit={async (event) => {
          event.preventDefault();
          const branch = upstreamBranchField.trim();
          if (upstreamRemoteField === "" || branch === "" || operationDisabled) return;
          await onSetUpstream(upstreamRemoteField, branch);
          onClose();
        }}
      >
        <label className={styles.label}>
          Upstream remote
          <select
            value={upstreamRemoteField}
            onChange={(event) => {
              const remoteName = event.target.value;
              setUpstreamRemoteField(remoteName);
              setRemoteBranchOptions([]);
              if (remoteName !== "") {
                void onListRemoteBranches(remoteName).then(setRemoteBranchOptions).catch(() => setRemoteBranchOptions([]));
              }
            }}
          >
            <option value="">Choose a remote</option>
            {remotes.map((remote) => (
              <option key={remote.name} value={remote.name}>
                {remote.name}
              </option>
            ))}
          </select>
        </label>
        <label className={styles.label}>
          Upstream branch
          <input
            list="upstream-branch-options"
            value={upstreamBranchField}
            onChange={(event) => setUpstreamBranchField(event.target.value)}
          />
        </label>
        <datalist id="upstream-branch-options">
          {remoteBranchOptions.map((name) => (
            <option key={name} value={name} />
          ))}
        </datalist>
        {busyReason !== null && <p className={formStyles.blocked}>{busyReason}</p>}
        <button type="submit" disabled={operationDisabled} title={busyReason ?? undefined}>
          Set upstream
        </button>
        <button type="button" onClick={onClose}>
          Cancel
        </button>
      </form>
    </FormDialog>
  );
}

import { useEffect, useState, useId } from "react";
import type { RemoteInfo } from "../ipc/RepoClient";
import { describedBy } from "../lib/ariaDescribedBy";
import { BlockedReason } from "./primitives/BlockedReason";
import { DialogActions } from "./primitives/DialogActions";
import { FormDialog } from "./primitives/FormDialog";
import styles from "./BranchTree.module.css";

/** The only remote, else `origin` if there is one, else none (#1014): the remote to preselect. */
function preselectedRemote(remotes: RemoteInfo[]): string {
  if (remotes.length === 1) return remotes[0].name;
  return remotes.some((remote) => remote.name === "origin") ? "origin" : "";
}

/**
 * "Set upstream…" for the current branch: a remote and a branch on it, the branch field suggesting
 * the remote's branches once one is chosen. It opens with the only remote (or `origin`) and the
 * branch's own name filled in (#1014), and says why Set upstream is off while a field is empty.
 * Mounted while open, so each opening starts afresh.
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
  // The visible reason the submit button is disabled is its description (#664).
  const reasonId = useId();
  const optionsId = useId();
  const [upstreamRemoteField, setUpstreamRemoteField] = useState(() => preselectedRemote(remotes));
  const [upstreamBranchField, setUpstreamBranchField] = useState(currentBranchName);
  const [remoteBranchOptions, setRemoteBranchOptions] = useState<string[]>([]);
  const branch = upstreamBranchField.trim();
  const reason =
    operationDisabled && busyReason !== null
      ? busyReason
      : upstreamRemoteField === ""
        ? "Choose a remote."
        : branch === ""
          ? "Enter the upstream branch."
          : null;
  const blocked = operationDisabled || reason !== null;

  const listBranches = (remoteName: string) => {
    void onListRemoteBranches(remoteName)
      .then(setRemoteBranchOptions)
      .catch(() => setRemoteBranchOptions([]));
  };

  // The preselected remote's branches, once, as choosing it would list them.
  useEffect(() => {
    if (upstreamRemoteField !== "") listBranches(upstreamRemoteField);
    // Only the remote the dialog opened with.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <FormDialog title={`Set upstream for ${currentBranchName}`} onCancel={onClose}>
      <form
        onSubmit={async (event) => {
          event.preventDefault();
          if (blocked) return;
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
              if (remoteName !== "") listBranches(remoteName);
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
            list={optionsId}
            value={upstreamBranchField}
            onChange={(event) => setUpstreamBranchField(event.target.value)}
          />
        </label>
        <datalist id={optionsId}>
          {remoteBranchOptions.map((name) => (
            <option key={name} value={name} />
          ))}
        </datalist>
        <BlockedReason id={reasonId} reason={reason} />
        <DialogActions>
          <button
            type="submit"
            data-tone="primary"
            disabled={blocked}
            title={reason ?? undefined}
            aria-describedby={describedBy(reasonId, reason)}
          >
            Set upstream
          </button>
          <button type="button" onClick={onClose}>
            Cancel
          </button>
        </DialogActions>
      </form>
    </FormDialog>
  );
}

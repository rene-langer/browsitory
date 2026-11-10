import { useState } from "react";
import { Copy } from "lucide-react";
import type { RemoteInfo } from "../ipc/RepoClient";
import { remoteNameProblem } from "../lib/refName";
import { FormDialog } from "./primitives/FormDialog";
import formStyles from "./primitives/FormDialog.module.css";
import styles from "./BranchTree.module.css";

/**
 * Rename a remote and change its URLs. Opened from a remote folder's menu in `BranchTree` or the
 * palette's "Edit remote <name>…" (via `AppState.dialog`). The fields start from `remote`
 * when the dialog mounts. `blockedReason` (`operationBlockers`) disables Save while it applies:
 * the repository can become busy after the dialog opened (#561).
 */
export function EditRemoteDialog({
  remote,
  onRename,
  onUpdateUrls,
  onClose,
  blockedReason = null,
}: {
  remote: RemoteInfo;
  /** Resolves `false` when the rename failed; the dialog then stays open and keeps the URLs. */
  onRename: (oldName: string, newName: string) => Promise<boolean>;
  onUpdateUrls: (name: string, fetchUrl: string, pushUrl: string | null) => Promise<void>;
  onClose: () => void;
  /** Why repository operations are blocked now, or `null`. */
  blockedReason?: string | null;
}) {
  const [name, setName] = useState(remote.name);
  const [fetchUrl, setFetchUrl] = useState(remote.fetchUrl);
  const [pushUrl, setPushUrl] = useState(remote.pushUrl ?? "");
  // The same check and message as the Add remote form (#418).
  const nameProblem = remoteNameProblem(name.trim());

  return (
    <FormDialog ariaLabel={`Edit ${remote.name}`} onCancel={onClose}>
      <form
        onSubmit={async (event) => {
          event.preventDefault();
          const nextName = name.trim();
          const nextFetchUrl = fetchUrl.trim();
          const nextPushUrl = pushUrl.trim() || null;
          if (nextName === "" || nextFetchUrl === "" || nameProblem !== null || blockedReason !== null) return;
          // Nothing changed: Save is a Cancel, with no backend call.
          if (nextName === remote.name && nextFetchUrl === remote.fetchUrl && nextPushUrl === remote.pushUrl) {
            onClose();
            return;
          }
          if (nextName !== remote.name && !(await onRename(remote.name, nextName))) return;
          await onUpdateUrls(nextName, nextFetchUrl, nextPushUrl);
          onClose();
        }}
      >
        <label className={styles.label}>
          Remote name
          <input
            value={name}
            aria-invalid={nameProblem !== null}
            aria-describedby={nameProblem !== null ? "edit-remote-name-problem" : undefined}
            onChange={(event) => setName(event.target.value)}
          />
        </label>
        {nameProblem !== null && (
          <p id="edit-remote-name-problem" className={styles.draftProblem}>
            {nameProblem}
          </p>
        )}
        <label className={styles.label}>
          Fetch URL
          <input value={fetchUrl} onChange={(event) => setFetchUrl(event.target.value)} />
        </label>
        <button
          type="button"
          className={styles.iconButton}
          aria-label={`Copy fetch URL for ${remote.name}`}
          onClick={() => {
            void navigator.clipboard.writeText(remote.fetchUrl);
          }}
        >
          <Copy size={12} aria-hidden="true" />
        </button>
        <label className={styles.label}>
          Push URL
          <input value={pushUrl} onChange={(event) => setPushUrl(event.target.value)} />
        </label>
        {remote.pushUrl !== null && (
          <button
            type="button"
            className={styles.iconButton}
            aria-label={`Copy push URL for ${remote.name}`}
            onClick={() => {
              void navigator.clipboard.writeText(remote.pushUrl!);
            }}
          >
            <Copy size={12} aria-hidden="true" />
          </button>
        )}
        {blockedReason !== null && <p className={formStyles.blocked}>{blockedReason}</p>}
        <button type="submit" disabled={nameProblem !== null || blockedReason !== null} title={blockedReason ?? undefined}>
          Save remote
        </button>
        <button type="button" onClick={onClose}>
          Cancel
        </button>
      </form>
    </FormDialog>
  );
}

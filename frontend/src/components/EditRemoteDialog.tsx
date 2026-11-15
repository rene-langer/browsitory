import { useState, useId } from "react";
import { Copy } from "lucide-react";
import type { RemoteInfo } from "../ipc/RepoClient";
import { remoteNameProblem } from "../lib/refName";
import { useCopyToClipboard } from "../state/useCopyToClipboard";
import { describedBy } from "../lib/ariaDescribedBy";
import { BlockedReason } from "./primitives/BlockedReason";
import { DialogActions } from "./primitives/DialogActions";
import { FormDialog } from "./primitives/FormDialog";
import styles from "./BranchTree.module.css";
import ownStyles from "./EditRemoteDialog.module.css";

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
  // The visible reason the submit button is disabled is its description (#664).
  const reasonId = useId();
  const [name, setName] = useState(remote.name);
  const [fetchUrl, setFetchUrl] = useState(remote.fetchUrl);
  const [pushUrl, setPushUrl] = useState(remote.pushUrl ?? "");
  const copyToClipboard = useCopyToClipboard();
  // The same check and message as the Add remote form (#418).
  const nameProblem = remoteNameProblem(name.trim());

  return (
    <FormDialog title={`Edit ${remote.name}`} onCancel={onClose}>
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
        {/* Each URL with a labelled button that copies the saved URL to the clipboard (#1014). */}
        <div className={ownStyles.urlRow}>
          <label className={styles.label}>
            Fetch URL
            <input value={fetchUrl} onChange={(event) => setFetchUrl(event.target.value)} />
          </label>
          <button
            type="button"
            className={ownStyles.copy}
            aria-label={`Copy fetch URL for ${remote.name}`}
            title="Copy the saved fetch URL"
            onClick={() => {
              copyToClipboard(remote.fetchUrl, "fetch URL");
            }}
          >
            <Copy size={12} aria-hidden="true" />
            Copy
          </button>
        </div>
        <div className={ownStyles.urlRow}>
          <label className={styles.label}>
            Push URL
            <input
              value={pushUrl}
              placeholder="Same as fetch URL"
              onChange={(event) => setPushUrl(event.target.value)}
            />
          </label>
          {remote.pushUrl !== null && (
            <button
              type="button"
              className={ownStyles.copy}
              aria-label={`Copy push URL for ${remote.name}`}
              title="Copy the saved push URL"
              onClick={() => {
                copyToClipboard(remote.pushUrl!, "push URL");
              }}
            >
              <Copy size={12} aria-hidden="true" />
              Copy
            </button>
          )}
        </div>
        <BlockedReason id={reasonId} reason={blockedReason} />
        <DialogActions>
          <button
            type="submit"
            data-tone="primary"
            disabled={nameProblem !== null || blockedReason !== null}
            title={blockedReason ?? undefined}
            aria-describedby={describedBy(reasonId, blockedReason)}
          >
            Save remote
          </button>
          <button type="button" onClick={onClose}>
            Cancel
          </button>
        </DialogActions>
      </form>
    </FormDialog>
  );
}

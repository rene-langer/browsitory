import { useState } from "react";
import { Copy } from "lucide-react";
import type { RemoteInfo } from "../ipc/RepoClient";
import { FormDialog } from "./primitives/FormDialog";
import styles from "./BranchTree.module.css";

/**
 * Rename a remote and change its URLs. Opened from a remote folder's menu in `BranchTree` or the
 * palette's "Edit remote <name>…" (via `AppState.remoteDialog`). The fields start from `remote`
 * when the dialog mounts.
 */
export function EditRemoteDialog({
  remote,
  onRename,
  onUpdateUrls,
  onClose,
}: {
  remote: RemoteInfo;
  /** Resolves `false` when the rename failed; the dialog then stays open and keeps the URLs. */
  onRename: (oldName: string, newName: string) => Promise<boolean>;
  onUpdateUrls: (name: string, fetchUrl: string, pushUrl: string | null) => Promise<void>;
  onClose: () => void;
}) {
  const [name, setName] = useState(remote.name);
  const [fetchUrl, setFetchUrl] = useState(remote.fetchUrl);
  const [pushUrl, setPushUrl] = useState(remote.pushUrl ?? "");

  return (
    <FormDialog ariaLabel={`Edit ${remote.name}`} onCancel={onClose}>
      <form
        onSubmit={async (event) => {
          event.preventDefault();
          const nextName = name.trim();
          const nextFetchUrl = fetchUrl.trim();
          if (nextName === "" || nextFetchUrl === "") return;
          if (nextName !== remote.name && !(await onRename(remote.name, nextName))) return;
          await onUpdateUrls(nextName, nextFetchUrl, pushUrl.trim() || null);
          onClose();
        }}
      >
        <label className={styles.label}>
          Remote name
          <input value={name} onChange={(event) => setName(event.target.value)} />
        </label>
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
        <button type="submit">Save remote</button>
        <button type="button" onClick={onClose}>
          Cancel
        </button>
      </form>
    </FormDialog>
  );
}

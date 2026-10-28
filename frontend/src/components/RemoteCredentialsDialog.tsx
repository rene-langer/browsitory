import { useRef, useState } from "react";
import type { RemoteAuthMode, RemoteInfo } from "../ipc/RepoClient";
import { FormDialog } from "./primitives/FormDialog";
import styles from "./BranchTree.module.css";

/**
 * Choose how a remote authenticates (HTTPS token or SSH agent) and save or forget its HTTPS
 * credential. Opened from a remote folder's menu in `BranchTree` or the palette's "Manage
 * credentials for <name>…" (via `AppState.remoteDialog`). The fields start from `remote` when the
 * dialog mounts; the token is never held in React state and is cleared after every attempt.
 */
export function RemoteCredentialsDialog({
  remote,
  onSetAuthMode,
  onSaveHttpsCredential,
  onForgetHttpsCredential,
  onClose,
}: {
  remote: RemoteInfo;
  onSetAuthMode: (remoteName: string, mode: RemoteAuthMode, username: string | null) => Promise<boolean>;
  onSaveHttpsCredential: (remoteName: string, username: string, token: string) => Promise<void>;
  onForgetHttpsCredential: (remoteName: string) => Promise<void>;
  onClose: () => void;
}) {
  const [mode, setMode] = useState<RemoteAuthMode>(remote.authMode ?? "HttpsToken");
  const [username, setUsername] = useState(remote.authUsername ?? "");
  const accessTokenRef = useRef<HTMLInputElement>(null);
  const remoteName = remote.name;

  return (
    <FormDialog ariaLabel={`Credentials for ${remoteName}`} onCancel={onClose}>
      <form
        onSubmit={async (event) => {
          event.preventDefault();
          const trimmedUsername = username.trim();
          const token = accessTokenRef.current?.value ?? "";
          try {
            if (mode === "SshAgent") {
              await onSetAuthMode(remoteName, "SshAgent", null);
              onClose();
            } else if (trimmedUsername !== "" && token !== "") {
              const configured = await onSetAuthMode(remoteName, "HttpsToken", trimmedUsername);
              if (configured) {
                await onSaveHttpsCredential(remoteName, trimmedUsername, token);
                onClose();
              }
            }
          } catch {
            // The application state owns remediation messages for failed credential operations.
          } finally {
            if (accessTokenRef.current !== null) accessTokenRef.current.value = "";
          }
        }}
      >
        <label className={styles.label}>
          Authentication for {remoteName}
          <select value={mode} onChange={(event) => setMode(event.target.value as RemoteAuthMode)}>
            <option value="HttpsToken">HTTPS token</option>
            <option value="SshAgent">SSH agent</option>
          </select>
        </label>
        {mode === "HttpsToken" ? (
          <>
            <label className={styles.label}>
              HTTPS username
              <input value={username} onChange={(event) => setUsername(event.target.value)} autoComplete="off" />
            </label>
            <label className={styles.label}>
              Access token
              <input ref={accessTokenRef} type="password" autoComplete="off" />
            </label>
            <button type="submit">Save HTTPS credential</button>
            <button type="button" onClick={() => void onForgetHttpsCredential(remoteName)}>
              Forget HTTPS credential
            </button>
          </>
        ) : (
          <>
            <p className={styles.helperText}>
              Uses your system's SSH agent to authenticate — make sure one is running (for example via{" "}
              <code>ssh-add</code>) before fetching or pushing.
            </p>
            <button type="submit">Use SSH agent</button>
          </>
        )}
        <button type="button" onClick={onClose}>
          Cancel credentials
        </button>
      </form>
    </FormDialog>
  );
}

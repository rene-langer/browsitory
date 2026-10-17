import { useEffect, useRef, useState } from "react";
import type { CloneOutcome, RepoClient } from "../ipc/RepoClient";
import { CloneError } from "../ipc/RepoClient";
import { deriveRepoName, joinPath } from "../lib/cloneNames";
import { FormDialog } from "./primitives/FormDialog";
import { InlineError } from "./primitives/InlineError";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./CloneDialog.module.css";

type Progress = { phase: "Receiving"; current: number; total: number } | { phase: "Updating" } | null;

// https only: the backend refuses to send a token over plain http.
const isHttpsUrl = (url: string) => /^https:\/\//i.test(url.trim());

// `/…`, `\\…` (UNC) or a drive root like `C:\` / `C:/`; `~` and relative paths are not.
const isAbsolutePath = (path: string) => /^([/\\]|[A-Za-z]:[/\\])/.test(path);

function messageFor(error: CloneError, dest: string, branch: string, https: boolean): string | null {
  switch (error.kind) {
    case "Cancelled":
      return null;
    case "DestNotEmpty":
      return `${dest} already exists and isn't empty.`;
    case "BranchNotFound":
      return `Branch ${branch} not found on the remote.`;
    case "InvalidUrl":
      return "Unsupported URL. Use https://, ssh://, git@host:path or a local path.";
    case "CredentialBearingUrl":
      return "Remove the password or token from the URL and put it in the token field.";
    case "MissingCredential":
      return https ? "This repository needs a username and token." : "Use an https:// URL to sign in with a token.";
    case "SshAgentFailure":
      return "SSH agent unavailable or has no matching key.";
    default:
      return error.message;
  }
}

export function CloneDialog({
  client,
  onCloned,
  onCancel,
  onRunningChange,
}: {
  client: RepoClient;
  onCloned: (outcome: CloneOutcome, name: string) => void;
  onCancel: () => void;
  /** Lets the host refuse to dismiss the dialog while a clone is in flight. */
  onRunningChange?: (running: boolean) => void;
}) {
  const [url, setUrl] = useState("");
  const [parent, setParent] = useState("");
  const [name, setName] = useState("");
  const [nameEdited, setNameEdited] = useState(false);
  const [branch, setBranch] = useState("");
  const [recurseSubmodules, setRecurseSubmodules] = useState(true);
  const [httpsOpen, setHttpsOpen] = useState(false);
  const [username, setUsername] = useState("");
  const [token, setToken] = useState("");
  const [running, setRunning] = useState(false);
  const [progress, setProgress] = useState<Progress>(null);
  const [error, setError] = useState<string | null>(null);
  const operationId = useRef<string | null>(null);

  const parentTrimmed = parent.trim();
  const parentRelative = parentTrimmed !== "" && !isAbsolutePath(parentTrimmed);
  const dest = parentTrimmed !== "" && !parentRelative && name.trim() !== "" ? joinPath(parent.trim(), name.trim()) : "";
  const showHttps = isHttpsUrl(url);

  useEffect(
    () =>
      client.subscribeTransferProgress((event) => {
        if (event.operationId !== operationId.current) return;
        if (event.phase === "Receiving") setProgress({ phase: "Receiving", current: event.current, total: event.total });
        else if (event.phase === "Updating") setProgress({ phase: "Updating" });
      }),
    [client],
  );

  const changeUrl = (value: string) => {
    setUrl(value);
    if (!nameEdited) setName(deriveRepoName(value));
  };

  const browse = () => {
    void client.pickRepoFolder().then((path) => {
      if (path !== null) setParent(path);
    });
  };

  const submit = () => {
    const id = crypto.randomUUID();
    operationId.current = id;
    setRunning(true);
    onRunningChange?.(true);
    setProgress(null);
    setError(null);
    const trimmedBranch = branch.trim();
    const https = showHttps && username.trim() !== "" && token !== "" ? { username: username.trim(), token } : null;
    client
      .cloneRepo(id, { url: url.trim(), dest, branch: trimmedBranch === "" ? null : trimmedBranch, recurseSubmodules, https })
      .then((outcome) => onCloned(outcome, name.trim()))
      .catch((raw: unknown) => {
        const failure = raw instanceof CloneError ? raw : new CloneError("TransferFailed", String(raw));
        if (failure.kind === "MissingCredential" || failure.kind === "CredentialBearingUrl") setHttpsOpen(true);
        setError(messageFor(failure, dest, trimmedBranch, showHttps));
      })
      .finally(() => {
        operationId.current = null;
        setRunning(false);
        onRunningChange?.(false);
        setProgress(null);
      });
  };

  return (
    <FormDialog ariaLabel="Clone repository" onCancel={running ? () => {} : onCancel}>
      <form
        className={styles.form}
        onSubmit={(event) => {
          event.preventDefault();
          if (!running && url.trim() !== "" && dest !== "") submit();
        }}
      >
        <label>
          Repository URL
          <input value={url} disabled={running} onChange={(e) => changeUrl(e.target.value)} autoFocus />
        </label>
        <div className={styles.row}>
          <label className={styles.grow}>
            Parent folder
            <input value={parent} disabled={running} onChange={(e) => setParent(e.target.value)} />
          </label>
          <button type="button" disabled={running} onClick={browse}>
            Browse…
          </button>
        </div>
        {parentRelative && <p className={styles.hint}>Use an absolute folder path, or Browse…</p>}
        <label>
          Folder name
          <input
            value={name}
            disabled={running}
            onChange={(e) => {
              setName(e.target.value);
              setNameEdited(true);
            }}
          />
        </label>
        {dest !== "" && <p className={styles.dest}>{dest}</p>}
        <label>
          Branch (optional)
          <input value={branch} disabled={running} onChange={(e) => setBranch(e.target.value)} />
        </label>
        <label className={styles.check}>
          <input
            type="checkbox"
            checked={recurseSubmodules}
            disabled={running}
            onChange={(e) => setRecurseSubmodules(e.target.checked)}
          />
          Include submodules
        </label>
        {showHttps && (
          <section className={styles.https}>
            <button type="button" aria-expanded={httpsOpen} onClick={() => setHttpsOpen((open) => !open)}>
              HTTPS credentials (optional)
            </button>
            {httpsOpen && (
              <>
                <label>
                  Username
                  <input value={username} disabled={running} onChange={(e) => setUsername(e.target.value)} autoComplete="off" />
                </label>
                <label>
                  Token
                  <input type="password" value={token} disabled={running} onChange={(e) => setToken(e.target.value)} autoComplete="off" />
                </label>
                <p className={styles.hint}>Saved to your system keychain only if the clone succeeds.</p>
              </>
            )}
          </section>
        )}
        {error !== null && <InlineError message={error} onDismiss={() => setError(null)} />}
        {running && (
          <p className={styles.progress} aria-live="polite">
            {progress?.phase === "Updating"
              ? "Checking out…"
              : progress?.phase === "Receiving" && progress.total > 0
                ? `Receiving objects ${progress.current}/${progress.total}`
                : "Connecting…"}
          </p>
        )}
        <Toolbar>
          {running ? (
            <button type="button" onClick={() => operationId.current !== null && void client.cancelClone(operationId.current)}>
              Cancel clone
            </button>
          ) : (
            <>
              <button type="submit" disabled={url.trim() === "" || dest === ""}>
                Clone
              </button>
              <button type="button" onClick={onCancel}>
                Cancel
              </button>
            </>
          )}
        </Toolbar>
      </form>
    </FormDialog>
  );
}

import { useEffect, useRef, useState } from "react";
import { Cloud, RefreshCw, Upload, Pencil, KeyRound, Trash2, Copy } from "lucide-react";
import type { PullOutcome, RemoteAuthMode, RemoteInfo, UpstreamInfo } from "../ipc/RepoClient";
import { AccordionSection } from "./primitives/AccordionSection";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./RemotePanel.module.css";

function deriveRemoteName(fetchUrl: string, existingNames: string[]): string {
  if (!existingNames.includes("origin")) return "origin";
  const withoutGitSuffix = fetchUrl.replace(/\.git\/?$/, "");
  const slug = withoutGitSuffix
    .split(/[/:]/)
    .filter((part) => part !== "")
    .pop();
  return slug ?? "";
}

export function RemotePanel({
  remotes,
  upstream,
  remoteUpstreams,
  onAddRemote,
  onRenameRemote,
  onUpdateRemoteUrls,
  onRemoveRemote,
  onSaveHttpsCredential,
  onForgetHttpsCredential,
  onSetRemoteAuthMode,
  onSetUpstream,
  onClearUpstream,
  onFetchRemote,
  fetchDisabled,
  onPushCurrentBranch,
  pushDisabled,
  onPull,
  pullDisabled,
  pendingPull,
  pullOutcome,
  onMergePull,
  onRebasePull,
  onCancelPull,
}: {
  remotes: RemoteInfo[];
  upstream: UpstreamInfo | null;
  remoteUpstreams: Record<string, UpstreamInfo[]>;
  // `null` = added; a string = the failure message to show inline. See `useAppState.ts`'s
  // `addRemote` for why this isn't a rejecting promise.
  onAddRemote: (name: string, fetchUrl: string, pushUrl: string | null) => Promise<string | null>;
  onRenameRemote: (oldName: string, newName: string) => Promise<boolean>;
  onUpdateRemoteUrls: (name: string, fetchUrl: string, pushUrl: string | null) => Promise<void>;
  onRemoveRemote: (name: string, clearUpstreams: boolean) => Promise<void>;
  onSaveHttpsCredential: (remoteName: string, username: string, token: string) => Promise<void>;
  onForgetHttpsCredential: (remoteName: string) => Promise<void>;
  onSetRemoteAuthMode: (remoteName: string, mode: RemoteAuthMode, username: string | null) => Promise<boolean>;
  onSetUpstream: (remoteName: string, remoteBranch: string) => Promise<void>;
  onClearUpstream: () => Promise<void>;
  onFetchRemote: (remoteName: string) => Promise<void>;
  fetchDisabled: boolean;
  onPushCurrentBranch: (remoteName: string) => Promise<void>;
  pushDisabled: boolean;
  onPull: () => Promise<void>;
  pullDisabled: boolean;
  pendingPull: { upstreamRef: string } | null;
  pullOutcome: PullOutcome | null;
  onMergePull: (upstreamRef: string) => Promise<void>;
  onRebasePull: (upstreamRef: string) => void;
  onCancelPull: () => void;
}) {
  const pullDialogRef = useRef<HTMLDialogElement>(null);
  const removeDialogRef = useRef<HTMLDialogElement>(null);
  const accessTokenRef = useRef<HTMLInputElement>(null);
  const [newName, setNewName] = useState("");
  const [newFetchUrl, setNewFetchUrl] = useState("");
  const [newPushUrl, setNewPushUrl] = useState("");
  const [addError, setAddError] = useState<string | null>(null);
  // Whether the user has typed in the Remote-name field themselves. Auto-derivation is gated on
  // this rather than on `newName === ""`: real typing fires one `onChange` per keystroke, so the
  // first character of the URL would derive a one-character name and every later keystroke would
  // then see a non-empty `newName` and stop deriving. Only a paste (a single `change` carrying
  // the whole URL) ever worked under the old guard.
  const [nameTouched, setNameTouched] = useState(false);
  const [showPushUrl, setShowPushUrl] = useState(false);
  const [showAddForm, setShowAddForm] = useState(false);
  const [editing, setEditing] = useState<string | null>(null);
  const [editName, setEditName] = useState("");
  const [editFetchUrl, setEditFetchUrl] = useState("");
  const [editPushUrl, setEditPushUrl] = useState("");
  const [upstreamRemote, setUpstreamRemote] = useState("");
  const [upstreamBranch, setUpstreamBranch] = useState("");
  const [removeConfirmation, setRemoveConfirmation] = useState<string | null>(null);
  const [credentialRemote, setCredentialRemote] = useState<string | null>(null);
  const [credentialMode, setCredentialMode] = useState<RemoteAuthMode>("HttpsToken");
  const [credentialUsername, setCredentialUsername] = useState("");

  const submitAdd = async (event: React.FormEvent) => {
    event.preventDefault();
    setAddError(null);
    const fetchUrl = newFetchUrl.trim();
    const name = (newName.trim() || deriveRemoteName(fetchUrl, remotes.map((remote) => remote.name))).trim();
    if (name === "" || fetchUrl === "") return;
    // Branching on the resolved value, not a `catch`: `onAddRemote` is `useAppState`'s
    // `addRemote`, which never rejects (it reports failure by resolving to the message).
    const failure = await onAddRemote(name, fetchUrl, newPushUrl.trim() || null);
    if (failure !== null) {
      setAddError(failure);
      return;
    }
    setNewName("");
    setNewFetchUrl("");
    setNewPushUrl("");
    setNameTouched(false);
    setShowPushUrl(false);
  };

  const closeAddForm = () => {
    setShowAddForm(false);
    setNewName("");
    setNewFetchUrl("");
    setNewPushUrl("");
    setNameTouched(false);
    setShowPushUrl(false);
    setAddError(null);
  };

  const beginEdit = (remote: RemoteInfo) => {
    setEditing(remote.name);
    setEditName(remote.name);
    setEditFetchUrl(remote.fetchUrl);
    setEditPushUrl(remote.pushUrl ?? "");
  };

  const submitEdit = async (event: React.FormEvent, oldName: string) => {
    event.preventDefault();
    const name = editName.trim();
    const fetchUrl = editFetchUrl.trim();
    if (name === "" || fetchUrl === "") return;
    if (name !== oldName && !(await onRenameRemote(oldName, name))) return;
    await onUpdateRemoteUrls(name, fetchUrl, editPushUrl.trim() || null);
    setEditing(null);
  };

  const submitUpstream = async (event: React.FormEvent) => {
    event.preventDefault();
    const branch = upstreamBranch.trim();
    if (upstreamRemote === "" || branch === "") return;
    await onSetUpstream(upstreamRemote, branch);
    setUpstreamBranch("");
  };

  const requestRemove = (remote: RemoteInfo) => {
    if (remoteUpstreams[remote.name].length > 0) {
      setRemoveConfirmation(`clear:${remote.name}`);
    } else {
      setRemoveConfirmation(remote.name);
    }
  };

  const beginCredentialEdit = (remote: RemoteInfo) => {
    setCredentialRemote(remote.name);
    setCredentialMode(remote.authMode ?? "HttpsToken");
    setCredentialUsername(remote.authUsername ?? "");
    if (accessTokenRef.current !== null) accessTokenRef.current.value = "";
  };

  const submitCredential = async (event: React.FormEvent) => {
    event.preventDefault();
    if (credentialRemote === null) return;
    const username = credentialUsername.trim();
    const token = accessTokenRef.current?.value ?? "";
    try {
      if (credentialMode === "SshAgent") {
        await onSetRemoteAuthMode(credentialRemote, "SshAgent", null);
      } else if (username !== "" && token !== "") {
        const configured = await onSetRemoteAuthMode(credentialRemote, "HttpsToken", username);
        if (configured) await onSaveHttpsCredential(credentialRemote, username, token);
      }
    } catch {
      // The application state owns remediation messages for failed credential operations.
    } finally {
      if (accessTokenRef.current !== null) accessTokenRef.current.value = "";
    }
  };

  useEffect(() => {
    const dialog = pullDialogRef.current;
    if (pendingPull === null || dialog === null) return;
    if (!dialog.open && typeof dialog.showModal === "function") {
      dialog.showModal();
    } else if (!dialog.open) {
      dialog.setAttribute("open", "");
    }
    dialog.querySelector<HTMLButtonElement>("[data-autofocus]")?.focus();
  }, [pendingPull]);

  useEffect(() => {
    const dialog = removeDialogRef.current;
    if (removeConfirmation === null || dialog === null) return;
    if (!dialog.open && typeof dialog.showModal === "function") {
      dialog.showModal();
    } else if (!dialog.open) {
      dialog.setAttribute("open", "");
    }
    dialog.querySelector<HTMLButtonElement>("[data-autofocus]")?.focus();
  }, [removeConfirmation]);

  return (
    <AccordionSection title="Remotes" storageKey="sidebar-remotes" icon={Cloud} count={remotes.length}>
      {remotes.length === 0 ? (
        <p className={styles.emptyState}>Add a remote below to push and pull.</p>
      ) : (
        <ul className={styles.list}>
          {remotes.map((remote) => {
            const pushUrl = remote.pushUrl;
            return (
              <li key={remote.name}>
              {editing === remote.name ? (
                <form className={styles.form} onSubmit={(event) => submitEdit(event, remote.name)} aria-label={`Edit ${remote.name}`}>
                  <label className={styles.label}>
                    Remote name
                    <input value={editName} onChange={(event) => setEditName(event.target.value)} />
                  </label>
                  <label className={styles.label}>
                    Fetch URL
                    <input value={editFetchUrl} onChange={(event) => setEditFetchUrl(event.target.value)} />
                  </label>
                  <label className={styles.label}>
                    Push URL (optional)
                    <input value={editPushUrl} onChange={(event) => setEditPushUrl(event.target.value)} />
                  </label>
                  <button type="submit">Save remote</button>
                  <button type="button" onClick={() => setEditing(null)}>Cancel</button>
                </form>
              ) : credentialRemote === remote.name ? (
                <form className={styles.form} onSubmit={submitCredential} aria-label={`Credentials for ${remote.name}`}>
                  <h3 className={styles.formHeading}>Credentials for {remote.name}</h3>
                  <label className={styles.label}>
                    Authentication for {remote.name}
                    <select
                      value={credentialMode}
                      onChange={(event) => setCredentialMode(event.target.value as RemoteAuthMode)}
                    >
                      <option value="HttpsToken">HTTPS token</option>
                      <option value="SshAgent">SSH agent</option>
                    </select>
                  </label>
                  {credentialMode === "HttpsToken" ? (
                    <>
                      <label className={styles.label}>HTTPS username<input value={credentialUsername} onChange={(event) => setCredentialUsername(event.target.value)} autoComplete="off" /></label>
                      <label className={styles.label}>Access token<input ref={accessTokenRef} type="password" autoComplete="off" /></label>
                      <button type="submit">Save HTTPS credential</button>
                      <button type="button" onClick={() => void onForgetHttpsCredential(remote.name)}>Forget HTTPS credential</button>
                    </>
                  ) : (
                    <button type="submit">Use SSH agent</button>
                  )}
                  <button type="button" onClick={() => { if (accessTokenRef.current !== null) accessTokenRef.current.value = ""; setCredentialRemote(null); }}>Cancel credentials</button>
                </form>
              ) : (
                <>
                  <strong>{remote.name}</strong>
                  <span>Fetch: {remote.fetchUrl}</span>
                  <button
                    type="button"
                    className={styles.iconButton}
                    title={`Copy fetch URL for ${remote.name}`}
                    aria-label={`Copy fetch URL for ${remote.name}`}
                    onClick={() => { void navigator.clipboard.writeText(remote.fetchUrl); }}
                  >
                    <Copy size={12} aria-hidden="true" />
                  </button>
                  {pushUrl !== null && (
                    <>
                      <span>Push: {pushUrl}</span>
                      <button
                        type="button"
                        className={styles.iconButton}
                        title={`Copy push URL for ${remote.name}`}
                        aria-label={`Copy push URL for ${remote.name}`}
                        onClick={() => { void navigator.clipboard.writeText(pushUrl); }}
                      >
                        <Copy size={12} aria-hidden="true" />
                      </button>
                    </>
                  )}
                  <Toolbar>
                    <button
                      type="button"
                      className={styles.iconButton}
                      disabled={fetchDisabled}
                      title={`Fetch ${remote.name}`}
                      aria-label={`Fetch ${remote.name}`}
                      onClick={() => void onFetchRemote(remote.name)}
                    >
                      <RefreshCw size={14} aria-hidden="true" />
                    </button>
                    <button
                      type="button"
                      className={styles.iconButton}
                      disabled={pushDisabled}
                      title={`Push branch to ${remote.name}`}
                      aria-label={`Push branch to ${remote.name}`}
                      onClick={() => void onPushCurrentBranch(remote.name)}
                    >
                      <Upload size={14} aria-hidden="true" />
                    </button>
                    <button
                      type="button"
                      className={styles.iconButton}
                      title={`Edit ${remote.name}`}
                      aria-label={`Edit ${remote.name}`}
                      onClick={() => beginEdit(remote)}
                    >
                      <Pencil size={14} aria-hidden="true" />
                    </button>
                    <button
                      type="button"
                      className={styles.iconButton}
                      title={`Credentials for ${remote.name}`}
                      aria-label={`Credentials for ${remote.name}`}
                      onClick={() => beginCredentialEdit(remote)}
                    >
                      <KeyRound size={14} aria-hidden="true" />
                    </button>
                    <button
                      type="button"
                      className={`${styles.iconButton} ${styles.dangerButton} danger`}
                      title={`Remove ${remote.name}`}
                      aria-label={`Remove ${remote.name}`}
                      onClick={() => requestRemove(remote)}
                    >
                      <Trash2 size={14} aria-hidden="true" />
                    </button>
                  </Toolbar>
                </>
              )}
              </li>
            );
          })}
        </ul>
      )}

      {removeConfirmation !== null && (
        <dialog
          ref={removeDialogRef}
          aria-label="Remove remote confirmation"
          onCancel={(event) => {
            event.preventDefault();
            setRemoveConfirmation(null);
          }}
        >
          {removeConfirmation.startsWith("clear:") ? (
            <>
              <p>Remove {removeConfirmation.slice(6)} and clear upstreams for {remoteUpstreams[removeConfirmation.slice(6)].map((item) => item.localBranch).join(", ")}?</p>
              <button type="button" onClick={() => { void onRemoveRemote(removeConfirmation.slice(6), true).then(() => setRemoveConfirmation(null)); }}>Confirm remove</button>
            </>
          ) : (
            <>
              <p>Remove remote {removeConfirmation}?</p>
              <button type="button" onClick={() => { void onRemoveRemote(removeConfirmation, false).then(() => setRemoveConfirmation(null)); }}>Confirm remove</button>
            </>
          )}
          <button type="button" data-autofocus onClick={() => setRemoveConfirmation(null)}>Cancel</button>
        </dialog>
      )}

      {showAddForm ? (
        <form className={styles.form} onSubmit={submitAdd} aria-label="Add remote">
          <h3 className={styles.formHeading}>Add remote</h3>
          <label className={styles.label}>
            Remote name
            <input
              placeholder="origin"
              value={newName}
              onChange={(event) => {
                setNameTouched(true);
                setNewName(event.target.value);
              }}
            />
          </label>
          <label className={styles.label}>
            Fetch URL
            <input
              data-testid="add-remote-fetch-url"
              placeholder="git@github.com:user/repo.git"
              value={newFetchUrl}
              onChange={(event) => {
                const value = event.target.value;
                setNewFetchUrl(value);
                setAddError(null);
                if (!nameTouched) {
                  setNewName(deriveRemoteName(value, remotes.map((remote) => remote.name)));
                }
              }}
            />
          </label>
          {addError !== null && (
            <p role="alert" className={styles.fieldError}>
              {addError}
            </p>
          )}
          <details
            className={styles.disclosure}
            open={showPushUrl}
            onToggle={(event) => setShowPushUrl(event.currentTarget.open)}
          >
            <summary
              onClick={(event) => {
                event.preventDefault();
                setShowPushUrl((open) => !open);
              }}
            >
              Push URL (optional)
            </summary>
            {showPushUrl && (
              <label className={styles.label}>
                Push URL
                <input
                  placeholder="git@github.com:user/repo.git"
                  value={newPushUrl}
                  onChange={(event) => setNewPushUrl(event.target.value)}
                />
              </label>
            )}
          </details>
          <button type="submit" className={`${styles.primaryButton} primary`} disabled={fetchDisabled}>
            Add remote
          </button>
          <button type="button" onClick={closeAddForm}>
            Cancel
          </button>
        </form>
      ) : (
        <Toolbar>
          <button type="button" disabled={fetchDisabled} onClick={() => setShowAddForm(true)}>
            Add remote
          </button>
        </Toolbar>
      )}

      <section aria-labelledby="upstream-heading">
        <h3 id="upstream-heading" className={styles.sectionHeading}>Upstream</h3>
        {upstream === null ? <p>No upstream for the current branch.</p> : <p>{upstream.localBranch} tracks {upstream.remoteName}/{upstream.remoteBranch}.</p>}
        <button type="button" disabled={pullDisabled || upstream === null || pendingPull !== null} onClick={() => void onPull()}>
          Pull
        </button>
        {pullOutcome?.kind === "UpToDate" && <p role="status">Already up to date.</p>}
        <form className={styles.form} onSubmit={submitUpstream} aria-label="Set upstream">
          <label className={styles.label}>
            Upstream remote
            <select value={upstreamRemote} onChange={(event) => setUpstreamRemote(event.target.value)}>
              <option value="">Choose a remote</option>
              {remotes.map((remote) => <option key={remote.name} value={remote.name}>{remote.name}</option>)}
            </select>
          </label>
          <label className={styles.label}>Upstream branch<input value={upstreamBranch} onChange={(event) => setUpstreamBranch(event.target.value)} /></label>
          <button type="submit">Set upstream</button>
        </form>
        {upstream !== null && <button type="button" onClick={() => void onClearUpstream()}>Clear upstream</button>}
      </section>
      {pendingPull !== null && (
        <dialog
          ref={pullDialogRef}
          aria-label="Pull has diverged"
          onCancel={(event) => {
            event.preventDefault();
            onCancelPull();
          }}
        >
          <p>The pull has diverged from {pendingPull.upstreamRef}.</p>
          <button type="button" disabled={pullDisabled} onClick={() => void onMergePull(pendingPull.upstreamRef)}>Merge</button>
          <button type="button" disabled={pullDisabled} onClick={() => onRebasePull(pendingPull.upstreamRef)}>Rebase</button>
          <button type="button" data-autofocus onClick={onCancelPull}>Cancel</button>
        </dialog>
      )}
    </AccordionSection>
  );
}

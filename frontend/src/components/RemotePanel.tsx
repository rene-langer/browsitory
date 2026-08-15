import { useEffect, useRef, useState } from "react";
import type { PullOutcome, RemoteAuthMode, RemoteInfo, UpstreamInfo } from "../ipc/RepoClient";

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
  onAddRemote: (name: string, fetchUrl: string, pushUrl: string | null) => Promise<void>;
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
  const accessTokenRef = useRef<HTMLInputElement>(null);
  const [newName, setNewName] = useState("");
  const [newFetchUrl, setNewFetchUrl] = useState("");
  const [newPushUrl, setNewPushUrl] = useState("");
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
    const name = newName.trim();
    const fetchUrl = newFetchUrl.trim();
    if (name === "" || fetchUrl === "") return;
    await onAddRemote(name, fetchUrl, newPushUrl.trim() || null);
    setNewName("");
    setNewFetchUrl("");
    setNewPushUrl("");
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

  return (
    <section className="remote-panel" aria-labelledby="remote-panel-heading">
      <h2 id="remote-panel-heading">Remotes</h2>
      {remotes.length === 0 ? (
        <p>No remotes configured.</p>
      ) : (
        <ul className="remote-list">
          {remotes.map((remote) => (
            <li key={remote.name}>
              {editing === remote.name ? (
                <form onSubmit={(event) => submitEdit(event, remote.name)} aria-label={`Edit ${remote.name}`}>
                  <label>
                    Remote name
                    <input value={editName} onChange={(event) => setEditName(event.target.value)} />
                  </label>
                  <label>
                    Fetch URL
                    <input value={editFetchUrl} onChange={(event) => setEditFetchUrl(event.target.value)} />
                  </label>
                  <label>
                    Push URL (optional)
                    <input value={editPushUrl} onChange={(event) => setEditPushUrl(event.target.value)} />
                  </label>
                  <button type="submit">Save remote</button>
                  <button type="button" onClick={() => setEditing(null)}>Cancel</button>
                </form>
              ) : (
                <>
                  <strong>{remote.name}</strong>
                  <span>Fetch: {remote.fetchUrl}</span>
                  {remote.pushUrl !== null && <span>Push: {remote.pushUrl}</span>}
                  <button type="button" disabled={fetchDisabled} onClick={() => void onFetchRemote(remote.name)}>Fetch {remote.name}</button>
                  <button type="button" disabled={pushDisabled} onClick={() => void onPushCurrentBranch(remote.name)}>Push branch to {remote.name}</button>
                  <button type="button" onClick={() => beginEdit(remote)}>Edit {remote.name}</button>
                  <button type="button" onClick={() => beginCredentialEdit(remote)}>Credentials for {remote.name}</button>
                  <button type="button" onClick={() => requestRemove(remote)}>Remove {remote.name}</button>
                </>
              )}
            </li>
          ))}
        </ul>
      )}

      {credentialRemote !== null && (
        <form onSubmit={submitCredential} aria-label={`Credentials for ${credentialRemote}`}>
          <h3>Credentials for {credentialRemote}</h3>
          <label>
            Authentication for {credentialRemote}
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
              <label>HTTPS username<input value={credentialUsername} onChange={(event) => setCredentialUsername(event.target.value)} autoComplete="off" /></label>
              <label>Access token<input ref={accessTokenRef} type="password" autoComplete="off" /></label>
              <button type="submit">Save HTTPS credential</button>
              <button type="button" onClick={() => void onForgetHttpsCredential(credentialRemote)}>Forget HTTPS credential</button>
            </>
          ) : (
            <button type="submit">Use SSH agent</button>
          )}
          <button type="button" onClick={() => { if (accessTokenRef.current !== null) accessTokenRef.current.value = ""; setCredentialRemote(null); }}>Cancel credentials</button>
        </form>
      )}

      {removeConfirmation !== null && (
        <div role="alertdialog" aria-label="Remove remote confirmation">
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
          <button type="button" onClick={() => setRemoveConfirmation(null)}>Cancel</button>
        </div>
      )}

      <form onSubmit={submitAdd} aria-label="Add remote">
        <h3>Add remote</h3>
        <label>Remote name<input value={newName} onChange={(event) => setNewName(event.target.value)} /></label>
        <label>Fetch URL<input data-testid="add-remote-fetch-url" value={newFetchUrl} onChange={(event) => setNewFetchUrl(event.target.value)} /></label>
        <label>Push URL (optional)<input value={newPushUrl} onChange={(event) => setNewPushUrl(event.target.value)} /></label>
        <button type="submit" disabled={fetchDisabled}>Add remote</button>
      </form>

      <section aria-labelledby="upstream-heading">
        <h3 id="upstream-heading">Upstream</h3>
        {upstream === null ? <p>No upstream for the current branch.</p> : <p>{upstream.localBranch} tracks {upstream.remoteName}/{upstream.remoteBranch}.</p>}
        <button type="button" disabled={pullDisabled || upstream === null || pendingPull !== null} onClick={() => void onPull()}>
          Pull
        </button>
        {pullOutcome?.kind === "UpToDate" && <p role="status">Already up to date.</p>}
        <form onSubmit={submitUpstream} aria-label="Set upstream">
          <label>
            Upstream remote
            <select value={upstreamRemote} onChange={(event) => setUpstreamRemote(event.target.value)}>
              <option value="">Choose a remote</option>
              {remotes.map((remote) => <option key={remote.name} value={remote.name}>{remote.name}</option>)}
            </select>
          </label>
          <label>Upstream branch<input value={upstreamBranch} onChange={(event) => setUpstreamBranch(event.target.value)} /></label>
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
    </section>
  );
}

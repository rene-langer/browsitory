import { useState } from "react";
import type { RemoteInfo, UpstreamInfo } from "../ipc/RepoClient";

export function RemotePanel({
  remotes,
  upstream,
  onAddRemote,
  onRenameRemote,
  onUpdateRemoteUrls,
  onRemoveRemote,
  onSetUpstream,
  onClearUpstream,
}: {
  remotes: RemoteInfo[];
  upstream: UpstreamInfo | null;
  onAddRemote: (name: string, fetchUrl: string, pushUrl: string | null) => Promise<void>;
  onRenameRemote: (oldName: string, newName: string) => Promise<void>;
  onUpdateRemoteUrls: (name: string, fetchUrl: string, pushUrl: string | null) => Promise<void>;
  onRemoveRemote: (name: string) => Promise<void>;
  onSetUpstream: (remoteName: string, remoteBranch: string) => Promise<void>;
  onClearUpstream: () => Promise<void>;
}) {
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
    if (name !== oldName) await onRenameRemote(oldName, name);
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
    if (upstream?.remoteName === remote.name) {
      setRemoveConfirmation(`clear:${remote.name}`);
    } else {
      setRemoveConfirmation(remote.name);
    }
  };

  const clearUpstreamForRemoval = async () => {
    await onClearUpstream();
    setRemoveConfirmation(null);
  };

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
                  <button type="button" onClick={() => beginEdit(remote)}>Edit {remote.name}</button>
                  <button type="button" onClick={() => requestRemove(remote)}>Remove {remote.name}</button>
                </>
              )}
            </li>
          ))}
        </ul>
      )}

      {removeConfirmation !== null && (
        <div role="alertdialog" aria-label="Remove remote confirmation">
          {removeConfirmation.startsWith("clear:") ? (
            <>
              <p>Clear {upstream?.localBranch}'s upstream before removing {removeConfirmation.slice(6)}.</p>
              <button type="button" onClick={() => void clearUpstreamForRemoval()}>Clear upstream</button>
            </>
          ) : (
            <>
              <p>Remove remote {removeConfirmation}?</p>
              <button type="button" onClick={() => { void onRemoveRemote(removeConfirmation); setRemoveConfirmation(null); }}>Confirm remove</button>
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
        <button type="submit">Add remote</button>
      </form>

      <section aria-labelledby="upstream-heading">
        <h3 id="upstream-heading">Upstream</h3>
        {upstream === null ? <p>No upstream for the current branch.</p> : <p>{upstream.localBranch} tracks {upstream.remoteName}/{upstream.remoteBranch}.</p>}
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
    </section>
  );
}

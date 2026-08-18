import { useState } from "react";
import type { RemoteInfo, TagInfo } from "../ipc/RepoClient";
import { Panel } from "./primitives/Panel";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./TagPanel.module.css";

export function TagPanel({
  tags,
  remotes,
  onCreate,
  onDelete,
  onPush,
  pushDisabled,
}: {
  tags: TagInfo[];
  remotes: RemoteInfo[];
  onCreate: (name: string, message: string | null) => Promise<void>;
  onDelete: (name: string) => Promise<void>;
  onPush: (remoteName: string, names: string[]) => Promise<void>;
  pushDisabled: boolean;
}) {
  const [name, setName] = useState("");
  const [kind, setKind] = useState<"lightweight" | "annotated">("lightweight");
  const [message, setMessage] = useState("");
  const [remoteName, setRemoteName] = useState(remotes[0]?.name ?? "");
  const [selected, setSelected] = useState<string[]>([]);
  const [previousRemotes, setPreviousRemotes] = useState(remotes);
  const [previousTags, setPreviousTags] = useState(tags);
  const [deleteConfirmation, setDeleteConfirmation] = useState<string | null>(null);

  if (previousRemotes !== remotes) {
    setPreviousRemotes(remotes);
    if (!remotes.some((remote) => remote.name === remoteName)) {
      setRemoteName(remotes[0]?.name ?? "");
    }
  }

  if (previousTags !== tags) {
    setPreviousTags(tags);
    setSelected(selected.filter((name) => tags.some((tag) => tag.name === name)));
  }

  const createTag = async (event: React.FormEvent) => {
    event.preventDefault();
    const trimmedName = name.trim();
    const trimmedMessage = message.trim();
    if (trimmedName === "" || (kind === "annotated" && trimmedMessage === "")) return;
    await onCreate(trimmedName, kind === "annotated" ? trimmedMessage : null);
    setName("");
    setMessage("");
  };

  const toggleTag = (tagName: string) => {
    setSelected((current) => current.includes(tagName)
      ? current.filter((item) => item !== tagName)
      : [...current, tagName]);
  };

  return (
    <Panel title="Tags">
      <form className={styles.form} onSubmit={createTag} aria-label="Create tag">
        <label className={styles.label}>Tag name<input value={name} onChange={(event) => setName(event.target.value)} /></label>
        <label><input type="radio" name="tag-kind" checked={kind === "lightweight"} onChange={() => setKind("lightweight")} />Lightweight tag</label>
        <label><input type="radio" name="tag-kind" checked={kind === "annotated"} onChange={() => setKind("annotated")} />Annotated tag</label>
        {kind === "annotated" && <label className={styles.label}>Tag message<textarea value={message} onChange={(event) => setMessage(event.target.value)} /></label>}
        <button type="submit" disabled={pushDisabled}>Create tag</button>
      </form>

      {tags.length === 0 ? <p>No local tags.</p> : (
        <ul className={styles.list}>
          {tags.map((tag) => (
            <li key={tag.name}>
              <label><input type="checkbox" checked={selected.includes(tag.name)} onChange={() => toggleTag(tag.name)} aria-label={`Select ${tag.name}`} />{tag.name}</label>
              <span>{tag.annotated ? "Annotated" : "Lightweight"}</span>
              <Toolbar>
                <button type="button" disabled={pushDisabled} onClick={() => setDeleteConfirmation(tag.name)}>Delete {tag.name}</button>
              </Toolbar>
            </li>
          ))}
        </ul>
      )}

      <section aria-labelledby="push-tags-heading">
        <h3 id="push-tags-heading">Push tags</h3>
        <label className={styles.label}>
          Remote
          <select value={remoteName} onChange={(event) => setRemoteName(event.target.value)} disabled={pushDisabled || remotes.length === 0}>
            {remotes.map((remote) => <option key={remote.name} value={remote.name}>{remote.name}</option>)}
          </select>
        </label>
        <Toolbar>
          <button type="button" disabled={pushDisabled || remoteName === "" || selected.length === 0} onClick={() => void onPush(remoteName, selected)}>Push selected tags</button>
          <button type="button" disabled={pushDisabled || remoteName === ""} onClick={() => void onPush(remoteName, [])}>Push all tags</button>
        </Toolbar>
      </section>

      {deleteConfirmation !== null && (
        <dialog open aria-label={`Delete local tag ${deleteConfirmation}`}>
          <p>Delete local tag {deleteConfirmation}?</p>
          <button type="button" disabled={pushDisabled} onClick={() => void onDelete(deleteConfirmation).then(() => setDeleteConfirmation(null))}>Delete tag</button>
          <button type="button" onClick={() => setDeleteConfirmation(null)}>Cancel</button>
        </dialog>
      )}
    </Panel>
  );
}

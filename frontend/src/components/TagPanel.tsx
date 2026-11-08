import { useState } from "react";
import { Tag } from "lucide-react";
import type { RemoteInfo, TagInfo } from "../ipc/RepoClient";
import { AccordionSection } from "./primitives/AccordionSection";
import { ConfirmDialog } from "./primitives/ConfirmDialog";
import { ListRow } from "./primitives/ListRow";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./TagPanel.module.css";

/**
 * The list of every local tag: filter by name, push selected or all tags, delete a tag locally or
 * on a remote. Tags are created from the graph (a commit's "Create tag…") or the palette ("Create
 * tag at HEAD…"), not here (#438). Each row is one tag; its name sits in its own element.
 */
export function TagPanel({
  tags,
  remotes,
  onDelete,
  onDeleteFromRemote,
  onPush,
  pushDisabled,
  operationDisabledReason,
}: {
  tags: TagInfo[];
  remotes: RemoteInfo[];
  onDelete: (name: string) => Promise<void>;
  /** Opens the "Delete from remote" dialog for that tag. */
  onDeleteFromRemote: (name: string) => void;
  onPush: (remoteName: string, names: string[]) => Promise<void>;
  pushDisabled: boolean;
  // Human-readable reason `pushDisabled` is true, shown as a `title` on the buttons it disables
  // (issue #31/UX-003). `null` when nothing is blocking.
  operationDisabledReason: string | null;
}) {
  const [remoteName, setRemoteName] = useState(remotes[0]?.name ?? "");
  const [selected, setSelected] = useState<string[]>([]);
  const [filter, setFilter] = useState("");
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

  const toggleTag = (tagName: string) => {
    setSelected((current) => current.includes(tagName)
      ? current.filter((item) => item !== tagName)
      : [...current, tagName]);
  };

  const needle = filter.trim().toLowerCase();
  const shown = needle === "" ? tags : tags.filter((tag) => tag.name.toLowerCase().includes(needle));
  const disabledTitle = pushDisabled ? (operationDisabledReason ?? undefined) : undefined;
  const noRemote = remotes.length === 0;

  return (
    <AccordionSection title="Tags" storageKey="sidebar-tags" icon={Tag} count={tags.length}>
      {tags.length === 0 ? (
        <p className={styles.empty}>
          No local tags. To tag a commit, choose Create tag… in its menu in the graph, or Create tag at HEAD… in the
          command palette.
        </p>
      ) : (
        <>
          <label className={styles.label}>
            Filter tags
            <input
              type="search"
              value={filter}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => setFilter(event.target.value)}
            />
          </label>
          {shown.length === 0 ? (
            <p className={styles.empty}>No tags match “{filter.trim()}”.</p>
          ) : (
            <ul className={styles.list} aria-label="Tags">
              {shown.map((tag) => (
                <ListRow key={tag.name}>
                  <Tag size={14} aria-hidden="true" className={styles.rowIcon} />
                  <label className={styles.inlineLabel}>
                    <input
                      type="checkbox"
                      checked={selected.includes(tag.name)}
                      onChange={() => toggleTag(tag.name)}
                      aria-label={`Select ${tag.name}`}
                    />
                    <span className={styles.tagName}>{tag.name}</span>
                  </label>
                  <span>{tag.annotated ? "Annotated" : "Lightweight"}</span>
                  <Toolbar>
                    <button
                      type="button"
                      disabled={pushDisabled || noRemote}
                      title={noRemote ? "No remote configured." : disabledTitle}
                      onClick={() => onDeleteFromRemote(tag.name)}
                    >
                      Delete {tag.name} from remote…
                    </button>
                    <button
                      type="button"
                      disabled={pushDisabled}
                      title={disabledTitle}
                      onClick={() => setDeleteConfirmation(tag.name)}
                    >
                      Delete {tag.name}
                    </button>
                  </Toolbar>
                </ListRow>
              ))}
            </ul>
          )}
        </>
      )}

      <section aria-labelledby="push-tags-heading">
        <h3 id="push-tags-heading" className={styles.sectionHeading}>Push tags</h3>
        <label className={styles.label}>
          Remote
          <select
            value={remoteName}
            onChange={(event) => setRemoteName(event.target.value)}
            disabled={pushDisabled || noRemote}
            title={disabledTitle}
          >
            {remotes.map((remote) => <option key={remote.name} value={remote.name}>{remote.name}</option>)}
          </select>
        </label>
        <Toolbar>
          <button
            type="button"
            disabled={pushDisabled || remoteName === "" || selected.length === 0}
            title={disabledTitle}
            onClick={() => void onPush(remoteName, selected)}
          >
            Push selected tags
          </button>
          <button
            type="button"
            disabled={pushDisabled || remoteName === ""}
            title={disabledTitle}
            onClick={() => void onPush(remoteName, [])}
          >
            Push all tags
          </button>
        </Toolbar>
      </section>

      {deleteConfirmation !== null && (
        <ConfirmDialog
          ariaLabel={`Delete local tag ${deleteConfirmation}`}
          message={`Delete local tag ${deleteConfirmation}?`}
          confirmLabel="Delete tag"
          confirmDisabled={pushDisabled}
          confirmTitle={disabledTitle}
          onConfirm={() => void onDelete(deleteConfirmation).then(() => setDeleteConfirmation(null))}
          onCancel={() => setDeleteConfirmation(null)}
        />
      )}
    </AccordionSection>
  );
}

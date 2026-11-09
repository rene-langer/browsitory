import { useState, type KeyboardEvent } from "react";
import { MoreHorizontal, Tag } from "lucide-react";
import type { RemoteInfo, TagInfo } from "../ipc/RepoClient";
import { isMenuKey, rectAnchor } from "../lib/menuKeys";
import { AccordionSection } from "./primitives/AccordionSection";
import { ConfirmDialog } from "./primitives/ConfirmDialog";
import formStyles from "./primitives/FormDialog.module.css";
import { ContextMenu } from "./primitives/ContextMenu";
import { ListRow } from "./primitives/ListRow";
import { Toolbar } from "./primitives/Toolbar";
import { tagMenuItems } from "./tagMenuItems";
import styles from "./TagPanel.module.css";

// `returnFocusTo`: the row's "…" for a pointer (WebKit focuses neither the row nor the button);
// `undefined` for a key, so `ContextMenu` returns focus to what had it (as StashPanel).
type TagMenu = { name: string; x: number; y: number; returnFocusTo?: HTMLElement };

/**
 * The list of every local tag: one line per tag (push-selection checkbox, name, kind), its
 * actions in a menu (the "…" button, right-click, or the Menu key / Shift+F10 on the row's
 * checkbox or "…"), the same items as the tag badge menu in the graph (`tagMenuItems`). Above
 * the list a name filter, below it push selected / push all. Tags are created from the graph
 * (a commit's "Create tag…") or the palette ("Create tag at HEAD…"), not here (#438).
 * Clicking a row (or Enter / Space on it) selects the tag's commit in the graph, like the
 * palette's "Go to tag…" (#441).
 */
export function TagPanel({
  tags,
  remotes,
  onDelete,
  onDeleteFromRemote,
  onPush,
  onPushTag,
  onBranchFromCommit,
  onCheckoutCommit,
  onGoToTag,
  pushDisabled,
  operationDisabledReason,
}: {
  tags: TagInfo[];
  remotes: RemoteInfo[];
  onDelete: (name: string) => Promise<void>;
  /** Opens the "Delete from remote" dialog for that tag. */
  onDeleteFromRemote: (name: string) => void;
  /** Push selected (`names`) or all (`[]`) tags. */
  onPush: (remoteName: string, names: string[]) => Promise<void>;
  /** A row menu's "Push <tag> to <remote>". */
  onPushTag: (remoteName: string, name: string) => void;
  onBranchFromCommit: (commitId: string) => void;
  onCheckoutCommit: (commitId: string) => void;
  /** Selects and scrolls to the tag's commit in the graph (#441). */
  onGoToTag: (tag: TagInfo) => void;
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
  const [menu, setMenu] = useState<TagMenu | null>(null);

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

  // The menu acts on the tag it was opened for; a refresh that removed it closes the menu.
  const menuTag = menu === null ? undefined : tags.find((tag) => tag.name === menu.name);
  if (menu !== null && menuTag === undefined) setMenu(null);

  const toggleTag = (tagName: string) => {
    setSelected((current) => current.includes(tagName)
      ? current.filter((item) => item !== tagName)
      : [...current, tagName]);
  };

  // The row (a focus stop) and its "…" get the Menu key and Shift+F10 from `ListRow`, which
  // leaves them to a text field or checkbox: the checkbox opens the menu itself.
  const openFromKey = (event: KeyboardEvent<HTMLElement>, name: string) => {
    if (!isMenuKey(event)) return;
    event.preventDefault();
    const row = event.currentTarget.closest("li");
    setMenu({ name, ...rectAnchor(row ?? event.currentTarget) });
  };

  const needle = filter.trim().toLowerCase();
  const shown = needle === "" ? tags : tags.filter((tag) => tag.name.toLowerCase().includes(needle));
  const disabledTitle = pushDisabled ? (operationDisabledReason ?? undefined) : undefined;

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
                <ListRow
                  key={tag.name}
                  onClick={() => onGoToTag(tag)}
                  onOpenMenu={(anchor, trigger) =>
                    setMenu({
                      name: tag.name,
                      ...anchor,
                      returnFocusTo: trigger?.querySelector<HTMLElement>('button[aria-haspopup="menu"]') ?? undefined,
                    })
                  }
                >
                  <input
                    type="checkbox"
                    className={styles.select}
                    checked={selected.includes(tag.name)}
                    onChange={() => toggleTag(tag.name)}
                    // Ticking the box selects the tag for pushing, not its commit.
                    onClick={(event) => event.stopPropagation()}
                    onKeyDown={(event) => openFromKey(event, tag.name)}
                    aria-label={`Select ${tag.name}`}
                    aria-keyshortcuts="Shift+F10 ContextMenu"
                  />
                  <Tag size={14} aria-hidden="true" className={styles.rowIcon} />
                  <span className={styles.tagName} title={tag.name}>
                    {tag.name}
                  </span>
                  <span className={styles.kind}>{tag.annotated ? "Annotated" : "Lightweight"}</span>
                  <button
                    type="button"
                    className={styles.iconButton}
                    aria-label={`Actions for ${tag.name}`}
                    aria-haspopup="menu"
                    aria-expanded={menu?.name === tag.name}
                    data-menu-key={tag.name}
                    onClick={(event) => {
                      event.stopPropagation();
                      const trigger = event.currentTarget;
                      if (menu?.name === tag.name) {
                        // A second click closes it; focus stays on the button.
                        trigger.focus({ preventScroll: true });
                        setMenu(null);
                      } else {
                        setMenu({ name: tag.name, ...rectAnchor(trigger), returnFocusTo: trigger });
                      }
                    }}
                  >
                    <MoreHorizontal size={14} aria-hidden="true" />
                  </button>
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
            disabled={pushDisabled || remotes.length === 0}
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

      {menu !== null && menuTag !== undefined && (
        <ContextMenu
          key={menuTag.name}
          x={menu.x}
          y={menu.y}
          returnFocusTo={menu.returnFocusTo}
          isTrigger={(button) => button.getAttribute("data-menu-key") === menuTag.name}
          onClose={() => setMenu(null)}
          items={tagMenuItems({
            tag: menuTag.name,
            commitId: menuTag.commitId,
            remotes,
            disabled: pushDisabled,
            disabledReason: operationDisabledReason,
            onPushTag,
            onBranchFromCommit,
            onCheckoutCommit,
            onDeleteFromRemote,
            onDelete: setDeleteConfirmation,
          })}
        />
      )}

      {deleteConfirmation !== null && (
        <ConfirmDialog
          ariaLabel={`Delete local tag ${deleteConfirmation}`}
          message={
            <>
              <p>Delete local tag {deleteConfirmation}?</p>
              {/* #521: the repository can become busy while the confirmation is open. */}
              {disabledTitle !== undefined && <p className={formStyles.blocked}>{disabledTitle}</p>}
            </>
          }
          confirmLabel="Delete tag"
          confirmDisabled={pushDisabled}
          confirmTitle={disabledTitle}
          onConfirm={() => {
            if (pushDisabled) return;
            void onDelete(deleteConfirmation).then(() => setDeleteConfirmation(null));
          }}
          onCancel={() => setDeleteConfirmation(null)}
        />
      )}
    </AccordionSection>
  );
}

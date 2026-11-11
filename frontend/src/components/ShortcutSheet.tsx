import { shortcutGroups } from "../lib/shortcuts";
import styles from "./ShortcutSheet.module.css";

/** The keyboard shortcuts, written for the platform (#683): ⌘ on Apple platforms, Ctrl elsewhere. */
export function ShortcutSheet({ onClose, apple }: { onClose: () => void; apple?: boolean }) {
  const groups = shortcutGroups(apple);
  return (
    <div className={styles.sheet} aria-labelledby="shortcut-sheet-title" role="group">
      <h2 id="shortcut-sheet-title">Keyboard shortcuts</h2>
      {groups.map((group) => (
        <section key={group.title}>
          <h3>{group.title}</h3>
          <dl className={styles.list}>
            {group.shortcuts.map((shortcut) => (
              <div key={shortcut.keys + shortcut.description} className={styles.row}>
                <dt>
                  <kbd>{shortcut.keys}</kbd>
                </dt>
                <dd>{shortcut.description}</dd>
              </div>
            ))}
          </dl>
        </section>
      ))}
      <button type="button" onClick={onClose}>
        Close
      </button>
    </div>
  );
}

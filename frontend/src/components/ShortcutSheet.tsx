import { useContext } from "react";
import { useKeymap } from "../lib/keymap";
import { isVscodeHost } from "../lib/platform";
import { OpenPreferencesContext } from "../state/openPreferences";
import { shortcutGroups } from "../lib/shortcuts";
import styles from "./ShortcutSheet.module.css";

/** The keyboard shortcuts, written for the platform (#683): ⌘ on Apple platforms, Ctrl elsewhere. */
export function ShortcutSheet({
  onClose,
  apple,
  vscode = isVscodeHost(),
}: {
  onClose: () => void;
  apple?: boolean;
  /** In VSCode's webview, whose keybindings run some of these keys (#679). */
  vscode?: boolean;
}) {
  useKeymap();
  const openPreferences = useContext(OpenPreferencesContext);
  const groups = shortcutGroups(apple, vscode);
  return (
    <div className={styles.sheet} aria-labelledby="shortcut-sheet-title" role="group">
      <h2 id="shortcut-sheet-title">Keyboard shortcuts</h2>
      {vscode && (
        <p className={styles.note}>
          In VSCode, F5, F6, the tab keys and the others VSCode uses itself are VSCode keybindings while
          Browsitory has the focus. To change them, search for &quot;Browsitory&quot; in VSCode&apos;s Keyboard
          Shortcuts.
        </p>
      )}
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
      <div className={styles.actions}>
        {!vscode && openPreferences !== null && (
          // Preferences > Keyboard (#899).
          <button
            type="button"
            onClick={() => {
              onClose();
              openPreferences("keyboard");
            }}
          >
            Change shortcuts…
          </button>
        )}
        <button type="button" onClick={onClose}>
          Close
        </button>
      </div>
    </div>
  );
}

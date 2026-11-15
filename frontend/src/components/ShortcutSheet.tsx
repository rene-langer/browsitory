import { useContext } from "react";
import { useKeymap } from "../lib/keymap";
import { isVscodeHost } from "../lib/platform";
import { OpenPreferencesContext } from "../state/openPreferences";
import { shortcutGroups } from "../lib/shortcuts";
import { Panel } from "./primitives/Panel";
import styles from "./ShortcutSheet.module.css";

/**
 * The keyboard shortcuts, written for the platform (#683): ⌘ on Apple platforms, Ctrl elsewhere.
 * A `Panel` with a footer, like the operation log (#1016): the list scrolls, "Change shortcuts…"
 * and Close stay in view.
 */
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
    <Panel
      title="Keyboard shortcuts"
      footer={
        <div className={styles.actions}>
          {openPreferences !== null && (
            // Preferences > Keyboard (#899), which in VSCode links to its Keyboard Shortcuts (#900).
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
      }
    >
      {vscode && (
        <p className={styles.note}>
          In VSCode, F5, F6, the tab keys and the others VSCode uses itself are VSCode keybindings while
          Browsitory has the focus. To change them, search for &quot;Browsitory&quot; in VSCode&apos;s Keyboard
          Shortcuts.
        </p>
      )}
      {groups.map((group) => (
        <section key={group.title} className={styles.group}>
          <h3>{group.title}</h3>
          <dl className={styles.list}>
            {group.shortcuts.map((shortcut) => (
              <div key={shortcut.keys + shortcut.description} className={styles.row}>
                <dt>
                  <kbd className={styles.keys}>{shortcut.keys}</kbd>
                </dt>
                <dd>{shortcut.description}</dd>
              </div>
            ))}
          </dl>
        </section>
      ))}
    </Panel>
  );
}

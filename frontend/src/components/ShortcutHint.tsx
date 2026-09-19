import styles from "./ShortcutHint.module.css";

/** Passive header hint for the global command-palette shortcut (see `RepoWorkspace`'s keydown). */
export function ShortcutHint() {
  return (
    <span className={styles.hint} title="Open the command palette">
      <kbd>Ctrl/Cmd+K</kbd>
    </span>
  );
}

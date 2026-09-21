import styles from "./ShortcutHint.module.css";

function isApplePlatform(): boolean {
  if (typeof navigator === "undefined") return false;
  return /mac|iphone|ipad/i.test(`${navigator.platform ?? ""} ${navigator.userAgent ?? ""}`);
}

/** Passive header hint for the global command-palette shortcut (see `RepoWorkspace`'s keydown). */
export function ShortcutHint() {
  return (
    <span className={styles.hint} title="Open the command palette">
      <kbd>{isApplePlatform() ? "⌘K" : "Ctrl+K"}</kbd>
    </span>
  );
}

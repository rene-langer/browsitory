import { isApplePlatform } from "../lib/navigationKeys";
import { requestWorkspace } from "../lib/workspaceRequests";
import styles from "./CommandsButton.module.css";

/**
 * The header's "Commands" button (#647): opens the active workspace's command palette, the same
 * as its shortcut (see `useWorkspaceShortcuts`), which it shows.
 */
export function CommandsButton() {
  const apple = isApplePlatform();
  return (
    <button
      type="button"
      className={styles.button}
      title="Open the command palette"
      aria-keyshortcuts={apple ? "Meta+K" : "Control+K"}
      onClick={() => requestWorkspace("palette")}
    >
      Commands <kbd>{apple ? "⌘K" : "Ctrl+K"}</kbd>
    </button>
  );
}

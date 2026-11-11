import { ariaKeyShortcuts, shortcutLabel } from "../lib/keybindings";
import { requestWorkspace } from "../lib/workspaceRequests";
import styles from "./CommandsButton.module.css";

/**
 * The header's "Commands" button (#647): opens the active workspace's command palette, the same
 * as its shortcut (see `useWorkspaceShortcuts`), which it shows.
 */
export function CommandsButton() {
  return (
    <button
      type="button"
      className={styles.button}
      title="Open the command palette"
      aria-keyshortcuts={ariaKeyShortcuts("palette")}
      onClick={() => requestWorkspace("palette")}
    >
      Commands <kbd>{shortcutLabel("palette")}</kbd>
    </button>
  );
}

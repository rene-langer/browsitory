import { withShortcutLabel } from "../lib/keybindings";
import { useKeymap } from "../lib/keymap";
import { navigationKeyLabels } from "../lib/navigationKeys";
import { Toolbar } from "./primitives/Toolbar";

/** Back and Forward through the commits this tab visited (#618); disabled at the ends. */
export function NavigationButtons({
  canGoBack,
  canGoForward,
  onGoBack,
  onGoForward,
}: {
  canGoBack: boolean;
  canGoForward: boolean;
  onGoBack: () => void;
  onGoForward: () => void;
}) {
  useKeymap();
  const keys = navigationKeyLabels();
  return (
    <Toolbar aria-label="Navigation">
      <button
        type="button"
        aria-label="Back"
        title={withShortcutLabel("Back to the previously selected commit", keys.back)}
        disabled={!canGoBack}
        onClick={onGoBack}
      >
        ←
      </button>
      <button
        type="button"
        aria-label="Forward"
        title={withShortcutLabel("Forward again", keys.forward)}
        disabled={!canGoForward}
        onClick={onGoForward}
      >
        →
      </button>
    </Toolbar>
  );
}

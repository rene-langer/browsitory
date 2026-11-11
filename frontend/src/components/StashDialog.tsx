import { useState, useId } from "react";
import type { StashOptions } from "../ipc/RepoClient";
import { Field } from "./primitives/Field";
import { FormDialog } from "./primitives/FormDialog";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./StashDialog.module.css";

/**
 * "Stash…" (everything) or "Stash file…" (`paths` = that file): an optional message and the
 * keep-staged / include-untracked switches. Nothing here is remembered between openings. For an
 * untracked file (`untrackedOnly`) untracked files are always included: leaving them out would
 * leave nothing to stash. `blockedReason` (`stashBlockReason`) disables Stash while it applies:
 * a rebase or cherry-pick/revert sequence can start after the dialog opened (#508).
 */
export function StashDialog({
  paths,
  untrackedOnly = false,
  blockedReason = null,
  onStash,
  onCancel,
}: {
  paths: string[];
  untrackedOnly?: boolean;
  /** Why stashing is unavailable now, or `null`. */
  blockedReason?: string | null;
  onStash: (options: StashOptions) => void;
  onCancel: () => void;
}) {
  // The visible reason the submit button is disabled is its description (#664).
  const reasonId = useId();
  const [message, setMessage] = useState("");
  const [keepIndex, setKeepIndex] = useState(false);
  const [includeUntracked, setIncludeUntracked] = useState(true);

  return (
    <FormDialog ariaLabel="Stash changes" onCancel={onCancel}>
      <form
        className={styles.form}
        onSubmit={(event) => {
          event.preventDefault();
          // Enter in the message field submits even with the button disabled.
          if (blockedReason !== null) return;
          const trimmed = message.trim();
          onStash({
            message: trimmed === "" ? null : trimmed,
            keepIndex,
            includeUntracked: untrackedOnly || includeUntracked,
            paths,
          });
        }}
      >
        {paths.length > 0 && <p className={styles.scope}>Only: {paths.join(", ")}</p>}
        <Field label="Message">
          {(controlProps) => (
            <input
              {...controlProps}
              type="text"
              aria-label="Stash message"
              placeholder="Optional"
              value={message}
              onChange={(event) => setMessage(event.target.value)}
            />
          )}
        </Field>
        <label className={styles.option}>
          <input type="checkbox" checked={keepIndex} onChange={(event) => setKeepIndex(event.target.checked)} />
          Keep staged changes
        </label>
        {!untrackedOnly && (
          <label className={styles.option}>
            <input
              type="checkbox"
              checked={includeUntracked}
              onChange={(event) => setIncludeUntracked(event.target.checked)}
            />
            Include untracked files
          </label>
        )}
        {blockedReason !== null && <p id={reasonId} className={styles.blocked}>{blockedReason}</p>}
        <Toolbar>
          <button type="submit" disabled={blockedReason !== null} title={blockedReason ?? undefined} aria-describedby={blockedReason !== null ? reasonId : undefined}>
            Stash
          </button>
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
        </Toolbar>
      </form>
    </FormDialog>
  );
}

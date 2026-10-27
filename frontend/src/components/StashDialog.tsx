import { useState } from "react";
import type { StashOptions } from "../ipc/RepoClient";
import { Field } from "./primitives/Field";
import { FormDialog } from "./primitives/FormDialog";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./StashDialog.module.css";

/**
 * "Stash…" (everything) or "Stash file…" (`paths` = that file): an optional message and the
 * keep-staged / include-untracked switches. Nothing here is remembered between openings.
 */
export function StashDialog({
  paths,
  onStash,
  onCancel,
}: {
  paths: string[];
  onStash: (options: StashOptions) => void;
  onCancel: () => void;
}) {
  const [message, setMessage] = useState("");
  const [keepIndex, setKeepIndex] = useState(false);
  const [includeUntracked, setIncludeUntracked] = useState(true);

  return (
    <FormDialog ariaLabel="Stash changes" onCancel={onCancel}>
      <form
        className={styles.form}
        onSubmit={(event) => {
          event.preventDefault();
          const trimmed = message.trim();
          onStash({ message: trimmed === "" ? null : trimmed, keepIndex, includeUntracked, paths });
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
        <label className={styles.option}>
          <input
            type="checkbox"
            checked={includeUntracked}
            onChange={(event) => setIncludeUntracked(event.target.checked)}
          />
          Include untracked files
        </label>
        <Toolbar>
          <button type="submit">Stash</button>
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
        </Toolbar>
      </form>
    </FormDialog>
  );
}

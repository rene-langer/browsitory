import { useState, useId } from "react";
import type { StashOptions, StatusEntry } from "../ipc/RepoClient";
import { Field } from "./primitives/Field";
import { describedBy } from "../lib/ariaDescribedBy";
import { BlockedReason } from "./primitives/BlockedReason";
import { DialogActions } from "./primitives/DialogActions";
import { FormDialog } from "./primitives/FormDialog";
import { leftOut, leftOutNotice } from "../lib/externalFilters";
import styles from "./StashDialog.module.css";

/**
 * "Stash…" (everything) or "Stash file…" (`paths` = that file): an optional message and the
 * keep-staged / include-untracked switches. Nothing here is remembered between openings. For an
 * untracked file (`untrackedOnly`) untracked files are always included: leaving them out would
 * leave nothing to stash. `blockedReason` (`stashBlockReason`) disables Stash while it applies:
 * a rebase or cherry-pick/revert sequence can start after the dialog opened (#508). The changed
 * files in `status` Browsitory can't filter or convert are left out, of `paths` or of everything,
 * and named; with only such files picked, nothing is stashed (#1107).
 */
export function StashDialog({
  paths: picked,
  status = [],
  untrackedOnly = false,
  blockedReason: busyReason = null,
  onStash,
  onCancel,
}: {
  paths: string[];
  /** The repository's changes, to leave out the files Browsitory can't filter or convert (#1107). */
  status?: readonly StatusEntry[];
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
  const left = leftOut(status).filter((entry) => picked.length === 0 || picked.includes(entry.path));
  const leaveOut = left.map((entry) => entry.path);
  const paths = picked.filter((path) => !leaveOut.includes(path));
  const notice = left.length === 0 ? null : leftOutNotice(left);
  const blockedReason = busyReason ?? (picked.length > 0 && paths.length === 0 ? notice : null);

  return (
    <FormDialog title="Stash changes" onCancel={onCancel}>
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
            ...(leaveOut.length === 0 ? {} : { leaveOut }),
          });
        }}
      >
        {paths.length > 0 && <p className={styles.scope}>Only: {paths.join(", ")}</p>}
        {notice !== null && notice !== blockedReason && <p className={styles.scope}>{notice}</p>}
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
        <BlockedReason id={reasonId} reason={blockedReason} />
        <DialogActions>
          <button
            type="submit"
            data-tone="primary"
            disabled={blockedReason !== null}
            title={blockedReason ?? undefined}
            aria-describedby={describedBy(reasonId, blockedReason)}
          >
            Stash
          </button>
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
        </DialogActions>
      </form>
    </FormDialog>
  );
}

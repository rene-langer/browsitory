import { useState, useId } from "react";
import type { ResetMode } from "../ipc/RepoClient";
import { describedBy } from "../lib/ariaDescribedBy";
import { BlockedReason } from "./primitives/BlockedReason";
import { ConfirmDialog } from "./primitives/ConfirmDialog";
import { DialogActions } from "./primitives/DialogActions";
import { FormDialog } from "./primitives/FormDialog";
import styles from "./ResetDialog.module.css";

const MODES: Array<{ mode: ResetMode; label: string; detail: string }> = [
  { mode: "soft", label: "Soft", detail: "Keep all changes staged." },
  { mode: "mixed", label: "Mixed", detail: "Keep changes in the working tree, unstaged." },
  { mode: "hard", label: "Hard", detail: "Discard all changes to tracked files." },
];

/**
 * Reset the current branch to a commit: soft, mixed or hard (hard asks again). The button names
 * the mode ("Reset hard"), and a hard reset is a danger (#988). `blockedReason`
 * (`operationBlockers`) disables Reset while it applies: the repository can become busy after
 * the dialog opened, e.g. a rebase started in a terminal and picked up by a refresh (#514).
 */
export function ResetDialog({
  branch,
  shortId,
  onReset,
  onCancel,
  returnFocusTo,
  blockedReason = null,
}: {
  branch: string;
  shortId: string;
  onReset: (mode: ResetMode) => void;
  onCancel: () => void;
  /** `FormDialog`'s: where focus goes on close. */
  returnFocusTo?: HTMLElement | null;
  /** Why repository operations are blocked now, or `null`. */
  blockedReason?: string | null;
}) {
  // The visible reason the submit button is disabled is its description (#664).
  const reasonId = useId();
  const [mode, setMode] = useState<ResetMode>("mixed");
  const [confirmingHard, setConfirmingHard] = useState(false);

  if (confirmingHard) {
    return (
      <ConfirmDialog
        title={`Reset hard to ${shortId}`}
        message={<p>Discards all uncommitted changes. Undo can bring them back.</p>}
        confirmLabel="Reset hard"
        tone="danger"
        confirmReason={blockedReason}
        onConfirm={() => {
          if (blockedReason === null) onReset("hard");
        }}
        onCancel={() => setConfirmingHard(false)}
      />
    );
  }

  const label = MODES.find((option) => option.mode === mode)?.label.toLowerCase() ?? mode;
  return (
    <FormDialog title={`Reset ${branch} to ${shortId}`} onCancel={onCancel} returnFocusTo={returnFocusTo}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          // Enter on a radio submits even with the button disabled.
          if (blockedReason !== null) return;
          if (mode === "hard") setConfirmingHard(true);
          else onReset(mode);
        }}
      >
        <fieldset className={styles.modes}>
          <legend>What happens to your changes</legend>
          {MODES.map((option) => (
            <label key={option.mode} className={styles.mode}>
              <input
                type="radio"
                name="reset-mode"
                value={option.mode}
                checked={mode === option.mode}
                onChange={() => setMode(option.mode)}
              />
              <span className={styles.text}>
                <strong>{option.label}</strong>
                <span className={styles.detail}>{option.detail}</span>
              </span>
            </label>
          ))}
        </fieldset>
        <BlockedReason id={reasonId} reason={blockedReason} />
        <DialogActions>
          <button
            type="submit"
            data-tone={mode === "hard" ? "danger" : "primary"}
            disabled={blockedReason !== null}
            title={blockedReason ?? undefined}
            aria-describedby={describedBy(reasonId, blockedReason)}
          >
            {`Reset ${label}`}
          </button>
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
        </DialogActions>
      </form>
    </FormDialog>
  );
}

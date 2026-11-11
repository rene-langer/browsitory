import { useState, useId } from "react";
import type { ResetMode } from "../ipc/RepoClient";
import { ConfirmDialog } from "./primitives/ConfirmDialog";
import { FormDialog } from "./primitives/FormDialog";
import { Toolbar } from "./primitives/Toolbar";
import formStyles from "./primitives/FormDialog.module.css";
import styles from "./ResetDialog.module.css";

const MODES: Array<{ mode: ResetMode; label: string; detail: string }> = [
  { mode: "soft", label: "Soft", detail: "Keep all changes staged." },
  { mode: "mixed", label: "Mixed", detail: "Keep changes in the working tree, unstaged." },
  { mode: "hard", label: "Hard", detail: "Discard all changes to tracked files." },
];

/**
 * Reset the current branch to a commit: soft, mixed or hard (hard asks again). `blockedReason`
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
        ariaLabel="Confirm hard reset"
        message={
          <>
            <p>Discards all uncommitted changes. Undo can bring them back.</p>
          </>
        }
        confirmLabel="Reset hard"
        confirmReason={blockedReason}
        onConfirm={() => {
          if (blockedReason === null) onReset("hard");
        }}
        onCancel={() => setConfirmingHard(false)}
      />
    );
  }

  return (
    <FormDialog ariaLabel="Reset branch" onCancel={onCancel} returnFocusTo={returnFocusTo}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          // Enter on a radio submits even with the button disabled.
          if (blockedReason !== null) return;
          if (mode === "hard") setConfirmingHard(true);
          else onReset(mode);
        }}
      >
        <p>
          Reset {branch} to {shortId}
        </p>
        <fieldset className={styles.modes}>
          <legend>Mode</legend>
          {MODES.map((option) => (
            <label key={option.mode} className={styles.mode}>
              <input
                type="radio"
                name="reset-mode"
                value={option.mode}
                checked={mode === option.mode}
                onChange={() => setMode(option.mode)}
              />
              <span>
                <strong>{option.label}</strong> — {option.detail}
              </span>
            </label>
          ))}
        </fieldset>
        {blockedReason !== null && <p id={reasonId} className={formStyles.blocked}>{blockedReason}</p>}
        <Toolbar>
          <button type="submit" disabled={blockedReason !== null} title={blockedReason ?? undefined} aria-describedby={blockedReason !== null ? reasonId : undefined}>
            Reset
          </button>
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
        </Toolbar>
      </form>
    </FormDialog>
  );
}

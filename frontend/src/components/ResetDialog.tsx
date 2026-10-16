import { useState } from "react";
import type { ResetMode } from "../ipc/RepoClient";
import { ConfirmDialog } from "./primitives/ConfirmDialog";
import { FormDialog } from "./primitives/FormDialog";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./ResetDialog.module.css";

const MODES: Array<{ mode: ResetMode; label: string; detail: string }> = [
  { mode: "soft", label: "Soft", detail: "Keep all changes staged." },
  { mode: "mixed", label: "Mixed", detail: "Keep changes in the working tree, unstaged." },
  { mode: "hard", label: "Hard", detail: "Discard all changes to tracked files." },
];

export function ResetDialog({
  branch,
  shortId,
  onReset,
  onCancel,
}: {
  branch: string;
  shortId: string;
  onReset: (mode: ResetMode) => void;
  onCancel: () => void;
}) {
  const [mode, setMode] = useState<ResetMode>("mixed");
  const [confirmingHard, setConfirmingHard] = useState(false);

  if (confirmingHard) {
    return (
      <ConfirmDialog
        ariaLabel="Confirm hard reset"
        message={<p>Discards all uncommitted changes. Cannot be undone.</p>}
        confirmLabel="Reset hard"
        onConfirm={() => onReset("hard")}
        onCancel={() => setConfirmingHard(false)}
      />
    );
  }

  return (
    <FormDialog ariaLabel="Reset branch" onCancel={onCancel}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
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
        <Toolbar>
          <button type="submit">Reset</button>
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
        </Toolbar>
      </form>
    </FormDialog>
  );
}

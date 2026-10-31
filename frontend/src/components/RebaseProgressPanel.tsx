import { HookStatus } from "./HookStatus";
import { Panel } from "./primitives/Panel";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./RebaseProgressPanel.module.css";

export function RebaseProgressPanel({
  currentStep,
  totalSteps,
  disabled,
  disabledReason,
  onContinue,
  onAbort,
  title = "Rebase in progress",
  continueLabel = "Continue rebase",
  abortLabel = "Abort rebase",
  abortDisabled = false,
  hooks,
}: {
  currentStep: number;
  totalSteps: number;
  disabled: boolean;
  /** Visible explanation shown while Continue is disabled. */
  disabledReason?: string;
  onContinue: () => void;
  onAbort: () => void;
  /** Also used for a cherry-pick/revert sequence ("Cherry-pick in progress", ...). */
  title?: string;
  continueLabel?: string;
  abortLabel?: string;
  abortDisabled?: boolean;
  /** A sequence's Continue runs the commit hooks: the running hook's status line and Cancel. */
  hooks?: { running: { operationId: string; hook: string } | null; onCancel: () => void };
}) {
  return (
    <Panel title={title}>
      <p>
        Step {currentStep} of {totalSteps}
      </p>
      {disabled && disabledReason !== undefined && (
        <p id="rebase-continue-reason" className={styles.reason}>
          {disabledReason}
        </p>
      )}
      {hooks?.running != null && <HookStatus running={hooks.running} onCancel={hooks.onCancel} />}
      <Toolbar>
        <button
          onClick={onContinue}
          disabled={disabled}
          aria-describedby={disabled && disabledReason !== undefined ? "rebase-continue-reason" : undefined}
        >
          {continueLabel}
        </button>
        <button onClick={onAbort} disabled={abortDisabled}>
          {abortLabel}
        </button>
      </Toolbar>
    </Panel>
  );
}

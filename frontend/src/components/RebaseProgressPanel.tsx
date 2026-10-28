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
      <Toolbar>
        <button
          onClick={onContinue}
          disabled={disabled}
          aria-describedby={disabled && disabledReason !== undefined ? "rebase-continue-reason" : undefined}
        >
          {continueLabel}
        </button>
        <button onClick={onAbort}>{abortLabel}</button>
      </Toolbar>
    </Panel>
  );
}

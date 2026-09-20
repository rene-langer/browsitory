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
}: {
  currentStep: number;
  totalSteps: number;
  disabled: boolean;
  /** Visible explanation shown while Continue is disabled. */
  disabledReason?: string;
  onContinue: () => void;
  onAbort: () => void;
}) {
  return (
    <Panel title="Rebase in progress">
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
          Continue Rebase
        </button>
        <button onClick={onAbort}>Abort Rebase</button>
      </Toolbar>
    </Panel>
  );
}

import { Panel } from "./primitives/Panel";
import { Toolbar } from "./primitives/Toolbar";

export function RebaseProgressPanel({
  currentStep,
  totalSteps,
  disabled,
  onContinue,
  onAbort,
}: {
  currentStep: number;
  totalSteps: number;
  disabled: boolean;
  onContinue: () => void;
  onAbort: () => void;
}) {
  return (
    <Panel title="Rebase in progress">
      <p>
        Step {currentStep} of {totalSteps}
      </p>
      <Toolbar>
        <button onClick={onContinue} disabled={disabled}>
          Continue Rebase
        </button>
        <button onClick={onAbort}>Abort Rebase</button>
      </Toolbar>
    </Panel>
  );
}

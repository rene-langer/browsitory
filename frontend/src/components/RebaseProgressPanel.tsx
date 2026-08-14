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
    <div>
      <p>
        Step {currentStep} of {totalSteps}
      </p>
      <button onClick={onContinue} disabled={disabled}>
        Continue Rebase
      </button>
      <button onClick={onAbort}>Abort Rebase</button>
    </div>
  );
}

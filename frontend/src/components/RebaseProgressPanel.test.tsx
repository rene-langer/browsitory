import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { RebaseProgressPanel } from "./RebaseProgressPanel";

describe("RebaseProgressPanel", () => {
  it("shows the current step out of the total", () => {
    render(
      <RebaseProgressPanel
        currentStep={2}
        totalSteps={5}
        disabled={false}
        onContinue={vi.fn()}
        onAbort={vi.fn()}
      />,
    );

    expect(screen.getByText(/Step 2 of 5/)).toBeInTheDocument();
  });

  it("Continue rebase calls onContinue", () => {
    const onContinue = vi.fn();
    render(
      <RebaseProgressPanel
        currentStep={1}
        totalSteps={3}
        disabled={false}
        onContinue={onContinue}
        onAbort={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText("Continue rebase"));

    expect(onContinue).toHaveBeenCalled();
  });

  it("Continue rebase is disabled while a conflict is unresolved", () => {
    render(
      <RebaseProgressPanel
        currentStep={1}
        totalSteps={3}
        disabled={true}
        onContinue={vi.fn()}
        onAbort={vi.fn()}
      />,
    );

    expect(screen.getByText("Continue rebase").closest("button")).toBeDisabled();
  });

  it("Abort rebase calls onAbort", () => {
    const onAbort = vi.fn();
    render(
      <RebaseProgressPanel
        currentStep={1}
        totalSteps={3}
        disabled={false}
        onContinue={vi.fn()}
        onAbort={onAbort}
      />,
    );

    fireEvent.click(screen.getByText("Abort rebase"));

    expect(onAbort).toHaveBeenCalled();
  });

  it("shows why Continue is disabled and ties it to the button", () => {
    render(
      <RebaseProgressPanel
        currentStep={1}
        totalSteps={3}
        disabled
        disabledReason="Resolve 2 conflicts to continue"
        onContinue={vi.fn()}
        onAbort={vi.fn()}
      />,
    );
    const button = screen.getByRole("button", { name: "Continue rebase" });
    expect(button).toBeDisabled();
    expect(button).toHaveAccessibleDescription("Resolve 2 conflicts to continue");
  });

  it("takes custom labels for a cherry-pick or revert sequence", () => {
    const onContinue = vi.fn();
    const onAbort = vi.fn();
    render(
      <RebaseProgressPanel
        currentStep={2}
        totalSteps={5}
        disabled={false}
        onContinue={onContinue}
        onAbort={onAbort}
        title="Cherry-pick in progress"
        continueLabel="Continue cherry-pick"
        abortLabel="Abort cherry-pick"
      />,
    );
    expect(screen.getByText("Cherry-pick in progress")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Continue cherry-pick" }));
    fireEvent.click(screen.getByRole("button", { name: "Abort cherry-pick" }));
    expect(onContinue).toHaveBeenCalled();
    expect(onAbort).toHaveBeenCalled();
    expect(screen.queryByText("Rebase in progress")).toBeNull();
  });

  it("shows the running hook with a Cancel button that calls onCancel", () => {
    const onCancel = vi.fn();
    render(
      <RebaseProgressPanel
        currentStep={2}
        totalSteps={3}
        disabled={false}
        onContinue={() => {}}
        onAbort={() => {}}
        title="Cherry-pick in progress"
        continueLabel="Continue cherry-pick"
        abortLabel="Abort cherry-pick"
        hooks={{ running: { operationId: "continue-1", hook: "pre-commit" }, onCancel }}
      />,
    );
    expect(screen.getByRole("status")).toHaveTextContent("Running pre-commit hook…");
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: "Cancelling…" })).toBeDisabled();
  });
});

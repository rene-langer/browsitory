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

  it("Continue Rebase calls onContinue", () => {
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

    fireEvent.click(screen.getByText("Continue Rebase"));

    expect(onContinue).toHaveBeenCalled();
  });

  it("Continue Rebase is disabled while a conflict is unresolved", () => {
    render(
      <RebaseProgressPanel
        currentStep={1}
        totalSteps={3}
        disabled={true}
        onContinue={vi.fn()}
        onAbort={vi.fn()}
      />,
    );

    expect(screen.getByText("Continue Rebase").closest("button")).toBeDisabled();
  });

  it("Abort Rebase calls onAbort", () => {
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

    fireEvent.click(screen.getByText("Abort Rebase"));

    expect(onAbort).toHaveBeenCalled();
  });
});

import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { OperationStatusStrip } from "./OperationStatusStrip";

const noop = () => {};

describe("OperationStatusStrip", () => {
  it("renders nothing when idle", () => {
    const { container } = render(
      <OperationStatusStrip merging={false} operation={null} rebaseProgress={null} conflictCount={0} onAbortMerge={noop} onAbortRebase={noop} />,
    );
    expect(container).toBeEmptyDOMElement();
  });

  it("shows merge state with conflict reason and abort", () => {
    const onAbortMerge = vi.fn();
    render(
      <OperationStatusStrip merging operation="Merge" rebaseProgress={null} conflictCount={1} onAbortMerge={onAbortMerge} onAbortRebase={noop} />,
    );
    expect(screen.getByRole("status")).toHaveTextContent("Merging");
    expect(screen.getByText("Resolve 1 conflict to continue")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Abort merge" }));
    expect(onAbortMerge).toHaveBeenCalled();
  });

  it("shows rebase step and plural conflicts", () => {
    const onAbortRebase = vi.fn();
    render(
      <OperationStatusStrip
        merging={false}
        operation={null}
        rebaseProgress={{ currentStep: 2, totalSteps: 5 }}
        conflictCount={3}
        onAbortMerge={noop}
        onAbortRebase={onAbortRebase}
      />,
    );
    expect(screen.getByRole("status")).toHaveTextContent("Rebasing, step 2 of 5");
    expect(screen.getByText("Resolve 3 conflicts to continue")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Abort rebase" }));
    expect(onAbortRebase).toHaveBeenCalled();
  });

  it.each([
    ["CherryPick", "Cherry-picking", "Abort cherry-pick"],
    ["Revert", "Reverting", "Abort revert"],
    ["Merge", "Merging", "Abort merge"],
  ] as const)("labels a paused %s", (operation, label, abort) => {
    const onAbortMerge = vi.fn();
    render(
      <OperationStatusStrip
        merging
        operation={operation}
        rebaseProgress={null}
        conflictCount={1}
        onAbortMerge={onAbortMerge}
        onAbortRebase={() => {}}
      />,
    );
    expect(screen.getByText(label)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: abort }));
    expect(onAbortMerge).toHaveBeenCalled();
  });

  it("shows sequence progress with Continue and Abort", () => {
    const onContinueSequence = vi.fn();
    const onAbortSequence = vi.fn();
    const onAbortMerge = vi.fn();
    render(
      <OperationStatusStrip
        merging
        operation="CherryPick"
        rebaseProgress={null}
        sequence={{ kind: "CherryPick", currentStep: 2, totalSteps: 5, currentCommit: "c" }}
        conflictCount={1}
        onAbortMerge={onAbortMerge}
        onAbortRebase={noop}
        onContinueSequence={onContinueSequence}
        onAbortSequence={onAbortSequence}
      />,
    );
    const strip = screen.getByRole("status", { name: "Operation status" });
    expect(strip).toHaveTextContent("Cherry-picking 2/5");
    const cont = within(strip).getByRole("button", { name: "Continue" });
    expect(cont).toBeDisabled();
    expect(cont).toHaveAttribute("title", "Resolve 1 conflict to continue");
    fireEvent.click(within(strip).getByRole("button", { name: "Abort cherry-pick" }));
    expect(onAbortSequence).toHaveBeenCalled();
    expect(onAbortMerge).not.toHaveBeenCalled();
    expect(onContinueSequence).not.toHaveBeenCalled();
  });

  it("shows a sequence stopped by an error (no merge state) and enables Continue", () => {
    const onContinueSequence = vi.fn();
    render(
      <OperationStatusStrip
        merging={false}
        operation={null}
        rebaseProgress={null}
        sequence={{ kind: "Revert", currentStep: 3, totalSteps: 3, currentCommit: "c" }}
        conflictCount={0}
        onAbortMerge={noop}
        onAbortRebase={noop}
        onContinueSequence={onContinueSequence}
        onAbortSequence={noop}
      />,
    );
    expect(screen.getByRole("status", { name: "Operation status" })).toHaveTextContent("Reverting 3/3");
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(onContinueSequence).toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Abort revert" })).toBeInTheDocument();
  });
});

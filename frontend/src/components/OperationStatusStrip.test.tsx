import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { OperationStatusStrip } from "./OperationStatusStrip";

const noop = () => {};

describe("OperationStatusStrip", () => {
  it("renders nothing when idle", () => {
    const { container } = render(
      <OperationStatusStrip merging={false} rebaseProgress={null} conflictCount={0} onAbortMerge={noop} onAbortRebase={noop} />,
    );
    expect(container).toBeEmptyDOMElement();
  });

  it("shows merge state with conflict reason and abort", () => {
    const onAbortMerge = vi.fn();
    render(
      <OperationStatusStrip merging rebaseProgress={null} conflictCount={1} onAbortMerge={onAbortMerge} onAbortRebase={noop} />,
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
});

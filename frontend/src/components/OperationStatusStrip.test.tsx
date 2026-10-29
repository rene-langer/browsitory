import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RebaseInfo } from "../ipc/RepoClient";
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

  it("disables sequence Continue and Abort while an action is pending", () => {
    render(
      <OperationStatusStrip
        merging={false}
        operation={null}
        rebaseProgress={null}
        sequence={{ kind: "CherryPick", currentStep: 1, totalSteps: 2, currentCommit: "c" }}
        conflictCount={0}
        pending
        onAbortMerge={noop}
        onAbortRebase={noop}
        onContinueSequence={noop}
        onAbortSequence={noop}
      />,
    );
    expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Abort cherry-pick" })).toBeDisabled();
  });

  describe("a stored rebase", () => {
    const rebase = (health: RebaseInfo["health"]): RebaseInfo => ({ branch: "topic", onto: "o", currentStep: 1, totalSteps: 2, health });

    it("keeps \"Rebasing, step N of M\" while paused", () => {
      render(
        <OperationStatusStrip
          merging
          operation="CherryPick"
          rebaseProgress={{ currentStep: 1, totalSteps: 2 }}
          rebase={rebase({ kind: "Paused" })}
          conflictCount={0}
          onAbortMerge={noop}
          onAbortRebase={noop}
        />,
      );
      expect(screen.getByRole("status")).toHaveTextContent("Rebasing, step 1 of 2");
      expect(screen.queryByText(/interrupted/)).toBeNull();
    });

    it("shows the interrupted label and reason for a diverged rebase", () => {
      const onAbortRebase = vi.fn();
      const reasons = [
        ["HeadMoved", "HEAD was moved outside Browsitory."],
        ["BranchMoved", "topic was changed outside Browsitory."],
        ["BranchMissing", "topic was deleted outside Browsitory."],
        ["OtherOperation", "Another git operation is in progress."],
      ] as const;
      for (const [reason, text] of reasons) {
        const { unmount } = render(
          <OperationStatusStrip
            merging={false}
            operation={null}
            rebaseProgress={{ currentStep: 1, totalSteps: 2 }}
            rebase={rebase({ kind: "Diverged", reason })}
            conflictCount={0}
            onAbortMerge={noop}
            onAbortRebase={onAbortRebase}
          />,
        );
        const strip = screen.getByRole("status");
        expect(strip).toHaveTextContent("Rebase of topic interrupted");
        expect(strip).toHaveTextContent(text);
        expect(within(strip).queryByRole("button", { name: /continue/i })).toBeNull();
        const abort = within(strip).getByRole("button", { name: "Abort rebase" });
        expect(abort).toHaveAttribute("title", "Stops the rebase. HEAD stays where it is.");
        fireEvent.click(abort);
        unmount();
      }
      expect(onAbortRebase).toHaveBeenCalledTimes(4);
    });
  });
});

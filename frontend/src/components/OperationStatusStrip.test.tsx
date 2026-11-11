import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RebaseInfo, TerminalRebase, TerminalSequence } from "../ipc/RepoClient";
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

  it.each([
    ["CherryPick", "Cherry-pick already applied", "This cherry-pick's", "Abort cherry-pick"],
    ["Revert", "Revert already applied", "This revert's", "Abort revert"],
    ["Merge", "Merge already applied", "This merge's", "Abort merge"],
  ] as const)("says a leftover %s is already applied (#240)", (operation, label, noun, abort) => {
    const onAbortMerge = vi.fn();
    render(
      <OperationStatusStrip
        merging
        operation={operation}
        operationLeftover
        rebaseProgress={null}
        conflictCount={0}
        onAbortMerge={onAbortMerge}
        onAbortRebase={noop}
      />,
    );
    const strip = screen.getByRole("status");
    expect(within(strip).getByText(label)).toBeInTheDocument();
    expect(strip).toHaveTextContent(
      `${noun} changes are already in the last commit. Git's state for it was left behind. Your next commit or amend clears it, and so does Abort.`,
    );
    fireEvent.click(within(strip).getByRole("button", { name: abort }));
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
    expect(cont).toHaveAttribute("aria-disabled", "true");
    expect(cont).toHaveAccessibleDescription("Resolve 1 conflict to continue");
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
    expect(screen.getByRole("button", { name: "Continue" })).toHaveAttribute("aria-disabled", "true");
    expect(screen.getByRole("button", { name: "Abort cherry-pick" })).toHaveAttribute("aria-disabled", "true");
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

  describe("a rebase or git am started in a terminal (#263)", () => {
    const rebase: TerminalRebase = { kind: "Rebase", branch: "topic", onto: "abc", currentStep: 2, totalSteps: 3 };

    it("shows a rebase paused on a conflict as a rebase, not a merge, with no buttons", () => {
      // git wrote MERGE_MSG for the conflicted step, so `merging` is true.
      render(
        <OperationStatusStrip
          merging
          operation={null}
          rebaseProgress={null}
          terminalRebase={rebase}
          conflictCount={1}
          onAbortMerge={noop}
          onAbortRebase={noop}
        />,
      );
      const strip = screen.getByRole("status");
      expect(strip).toHaveTextContent("Rebasing topic, step 2 of 3 (started in a terminal)");
      expect(strip).toHaveTextContent("Resolve 1 conflict, then run git rebase --continue in the terminal.");
      expect(strip).not.toHaveTextContent("Merging");
      expect(within(strip).queryByRole("button")).toBeNull();
    });

    it("shows a rebase stopped without conflicts, and git am", () => {
      const { rerender } = render(
        <OperationStatusStrip
          merging={false}
          operation={null}
          rebaseProgress={null}
          terminalRebase={rebase}
          bisect={{ startedFrom: "main" }}
          conflictCount={0}
          onAbortMerge={noop}
          onAbortRebase={noop}
        />,
      );
      expect(screen.getByRole("status")).toHaveTextContent(
        "Continue or abort it there with git rebase --continue or git rebase --abort.",
      );
      rerender(
        <OperationStatusStrip
          merging={false}
          operation={null}
          rebaseProgress={null}
          terminalRebase={{ kind: "Am", branch: "", onto: "", currentStep: 1, totalSteps: 2 }}
          conflictCount={0}
          onAbortMerge={noop}
          onAbortRebase={noop}
        />,
      );
      const strip = screen.getByRole("status");
      expect(strip).toHaveTextContent("Applying patches, 1 of 2 (started in a terminal)");
      expect(strip).toHaveTextContent("Continue or abort it there with git am --continue or git am --abort.");
      expect(within(strip).queryByRole("button")).toBeNull();
    });
  });

  describe("a bisect started in a terminal (#250)", () => {
    it("shows a bisect with no buttons", () => {
      render(
        <OperationStatusStrip
          merging={false}
          operation={null}
          rebaseProgress={null}
          bisect={{ startedFrom: "main" }}
          conflictCount={0}
          onAbortMerge={noop}
          onAbortRebase={noop}
        />,
      );
      const strip = screen.getByRole("status");
      expect(within(strip).getByText("Bisecting (started from main)")).toBeInTheDocument();
      expect(strip).toHaveTextContent("Use git bisect good, bad or reset in a terminal.");
      expect(within(strip).queryByRole("button")).toBeNull();
    });

    it("names a commit it started from by its short id", () => {
      render(
        <OperationStatusStrip
          merging={false}
          operation={null}
          rebaseProgress={null}
          bisect={{ startedFrom: "0123456789abcdef0123456789abcdef01234567" }}
          conflictCount={0}
          onAbortMerge={noop}
          onAbortRebase={noop}
        />,
      );
      expect(screen.getByText("Bisecting (started from 0123456)")).toBeInTheDocument();
    });

    it("says only Bisecting when the start is unknown", () => {
      render(
        <OperationStatusStrip
          merging={false}
          operation={null}
          rebaseProgress={null}
          bisect={{ startedFrom: "" }}
          conflictCount={0}
          onAbortMerge={noop}
          onAbortRebase={noop}
        />,
      );
      expect(screen.getByText("Bisecting")).toBeInTheDocument();
    });

    it("a merge during a bisect shows the merge", () => {
      const onAbortMerge = vi.fn();
      render(
        <OperationStatusStrip
          merging
          operation="Merge"
          rebaseProgress={null}
          bisect={{ startedFrom: "main" }}
          conflictCount={1}
          onAbortMerge={onAbortMerge}
          onAbortRebase={noop}
        />,
      );
      const strip = screen.getByRole("status");
      expect(strip).toHaveTextContent("Merging");
      expect(strip).not.toHaveTextContent("Bisecting");
      fireEvent.click(within(strip).getByRole("button", { name: "Abort merge" }));
      expect(onAbortMerge).toHaveBeenCalled();
    });
  });

  describe("a sequence started in a terminal (#258, #253)", () => {
    const terminal = (overrides: Partial<TerminalSequence> = {}): TerminalSequence => ({
      kind: "CherryPick",
      pausedOnConflict: false,
      startedAt: "abc",
      startedAtSummary: "X",
      rewinds: true,
      commitsToRemove: 1,
      ...overrides,
    });

    it("between steps: label, text and Abort", () => {
      const onAbortTerminalSequence = vi.fn();
      render(
        <OperationStatusStrip
          merging={false}
          operation={null}
          rebaseProgress={null}
          terminalSequence={terminal()}
          bisect={{ startedFrom: "main" }}
          conflictCount={0}
          onAbortMerge={noop}
          onAbortRebase={noop}
          onAbortTerminalSequence={onAbortTerminalSequence}
        />,
      );
      const strip = screen.getByRole("status");
      expect(within(strip).getByText("Cherry-pick paused in a terminal")).toBeInTheDocument();
      expect(strip).toHaveTextContent("Run git cherry-pick --continue there to apply the remaining commits.");
      expect(within(strip).queryByRole("button", { name: "Continue" })).toBeNull();
      fireEvent.click(within(strip).getByRole("button", { name: "Abort cherry-pick" }));
      expect(onAbortTerminalSequence).toHaveBeenCalledTimes(1);
    });

    it("a revert between steps says revert", () => {
      render(
        <OperationStatusStrip
          merging={false}
          operation={null}
          rebaseProgress={null}
          terminalSequence={terminal({ kind: "Revert" })}
          conflictCount={0}
          onAbortMerge={noop}
          onAbortRebase={noop}
          onAbortTerminalSequence={noop}
        />,
      );
      expect(screen.getByText("Revert paused in a terminal")).toBeInTheDocument();
      expect(screen.getByRole("status")).toHaveTextContent("Run git revert --continue there");
      expect(screen.getByRole("button", { name: "Abort revert" })).toBeInTheDocument();
    });

    it("paused on a conflict: the step's strip, its Abort ends the whole sequence", () => {
      const onAbortMerge = vi.fn();
      const onAbortTerminalSequence = vi.fn();
      render(
        <OperationStatusStrip
          merging
          operation="CherryPick"
          rebaseProgress={null}
          terminalSequence={terminal({ pausedOnConflict: true })}
          conflictCount={1}
          onAbortMerge={onAbortMerge}
          onAbortRebase={noop}
          onAbortTerminalSequence={onAbortTerminalSequence}
        />,
      );
      const strip = screen.getByRole("status");
      expect(within(strip).getByText("Cherry-picking (started in a terminal)")).toBeInTheDocument();
      expect(strip).toHaveTextContent("Resolve 1 conflict to continue");
      fireEvent.click(within(strip).getByRole("button", { name: "Abort cherry-pick" }));
      expect(onAbortTerminalSequence).toHaveBeenCalledTimes(1);
      expect(onAbortMerge).not.toHaveBeenCalled();
    });

    it("a merge during a terminal sequence shows the merge", () => {
      const onAbortMerge = vi.fn();
      render(
        <OperationStatusStrip
          merging
          operation="Merge"
          rebaseProgress={null}
          terminalSequence={terminal()}
          conflictCount={1}
          onAbortMerge={onAbortMerge}
          onAbortRebase={noop}
          onAbortTerminalSequence={noop}
        />,
      );
      const strip = screen.getByRole("status");
      expect(strip).toHaveTextContent("Merging");
      fireEvent.click(within(strip).getByRole("button", { name: "Abort merge" }));
      expect(onAbortMerge).toHaveBeenCalled();
    });

    it("a stored sequence still wins", () => {
      render(
        <OperationStatusStrip
          merging
          operation="CherryPick"
          rebaseProgress={null}
          sequence={{ kind: "CherryPick", currentStep: 2, totalSteps: 3, currentCommit: "c" }}
          terminalSequence={terminal({ pausedOnConflict: true })}
          conflictCount={0}
          onAbortMerge={noop}
          onAbortRebase={noop}
          onAbortTerminalSequence={noop}
        />,
      );
      expect(screen.getByText("Cherry-picking 2/3")).toBeInTheDocument();
      expect(screen.queryByText(/started in a terminal/)).toBeNull();
    });
  });

  describe("context and next steps (#665)", () => {
    it("names what a merge merges into which branch, with Go to conflict", () => {
      const onGoToConflict = vi.fn();
      render(
        <OperationStatusStrip
          merging
          operation="Merge"
          mergeMessage={"Merge branch 'feature/x'\n\n# Conflicts:\n#\ta.txt\n"}
          currentBranch="main"
          rebaseProgress={null}
          conflictCount={2}
          onGoToConflict={onGoToConflict}
          onAbortMerge={noop}
          onAbortRebase={noop}
        />,
      );
      expect(screen.getByText("Merging feature/x into main")).toBeInTheDocument();
      fireEvent.click(screen.getByRole("button", { name: "Go to conflict" }));
      expect(onGoToConflict).toHaveBeenCalledOnce();
    });

    it("says where a cherry-pick lands, and offers Go to conflict only while there are conflicts", () => {
      render(
        <OperationStatusStrip
          merging
          operation="CherryPick"
          currentBranch="main"
          rebaseProgress={null}
          conflictCount={0}
          onGoToConflict={noop}
          onAbortMerge={noop}
          onAbortRebase={noop}
        />,
      );
      expect(screen.getByText("Cherry-picking onto main")).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "Go to conflict" })).toBeNull();
    });

    it("offers Continue next to Abort rebase, unavailable with the reason while conflicts remain", () => {
      const onContinueRebase = vi.fn();
      const props = {
        merging: false,
        operation: null,
        rebaseProgress: { currentStep: 1, totalSteps: 2 },
        onContinueRebase,
        onAbortMerge: noop,
        onAbortRebase: noop,
      };
      const { rerender } = render(<OperationStatusStrip {...props} conflictCount={1} />);
      const blocked = screen.getByRole("button", { name: "Continue" });
      expect(blocked).toHaveAttribute("aria-disabled", "true");
      expect(blocked).toHaveAccessibleDescription("Resolve 1 conflict to continue");
      fireEvent.click(blocked);
      expect(onContinueRebase).not.toHaveBeenCalled();

      rerender(<OperationStatusStrip {...props} conflictCount={0} />);
      fireEvent.click(screen.getByRole("button", { name: "Continue" }));
      expect(onContinueRebase).toHaveBeenCalledOnce();
      expect(screen.getByRole("button", { name: "Abort rebase" })).toBeInTheDocument();
    });
  });
});

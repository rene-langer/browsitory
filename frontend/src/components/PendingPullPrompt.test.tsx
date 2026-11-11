import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { PullStrategy } from "../ipc/RepoClient";
import { PendingPullPrompt } from "./PendingPullPrompt";

function renderPrompt(strategy: PullStrategy | null | Error, props: Partial<Parameters<typeof PendingPullPrompt>[0]> = {}) {
  const handlers = {
    getPullStrategy: vi.fn(() => (strategy instanceof Error ? Promise.reject(strategy) : Promise.resolve(strategy))),
    setPullStrategy: vi.fn(async () => {}),
    onMerge: vi.fn(),
    onRebase: vi.fn(),
    onCancel: vi.fn(),
  };
  render(
    <PendingPullPrompt
      upstreamRef="refs/remotes/origin/main"
      branchName="main"
      operationDisabled={false}
      operationDisabledReason={null}
      {...handlers}
      {...props}
    />,
  );
  return handlers;
}

describe("PendingPullPrompt (#652)", () => {
  it("merges at once when the strategy is Merge, without asking", async () => {
    const { onMerge, onRebase } = renderPrompt("Merge");
    await waitFor(() => expect(onMerge).toHaveBeenCalledWith("refs/remotes/origin/main"));
    expect(onRebase).not.toHaveBeenCalled();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("rebases at once when the strategy is Rebase", async () => {
    const { onRebase } = renderPrompt("Rebase");
    await waitFor(() => expect(onRebase).toHaveBeenCalledWith("refs/remotes/origin/main"));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("asks when no strategy is set, and remembers the choice when asked to", async () => {
    const { onMerge, setPullStrategy } = renderPrompt(null);
    const dialog = await screen.findByRole("dialog", { name: "Pull has diverged" });
    expect(dialog).toHaveTextContent("have both changed");
    fireEvent.click(screen.getByRole("checkbox", { name: "Remember for this repository" }));
    fireEvent.click(screen.getByRole("button", { name: "Merge" }));
    expect(setPullStrategy).toHaveBeenCalledWith("Merge");
    expect(onMerge).toHaveBeenCalledWith("refs/remotes/origin/main");
  });

  it("asks, saying a fast-forward wasn't possible, for Fast-forward only", async () => {
    const { setPullStrategy, onRebase } = renderPrompt("FastForwardOnly");
    const dialog = await screen.findByRole("dialog", { name: "Pull has diverged" });
    expect(dialog).toHaveTextContent("can't be fast-forwarded");
    fireEvent.click(screen.getByRole("button", { name: "Rebase" }));
    expect(onRebase).toHaveBeenCalled();
    expect(setPullStrategy).not.toHaveBeenCalled();
  });

  it("asks when the strategy can't be read", async () => {
    renderPrompt(new Error("config unreadable"));
    expect(await screen.findByRole("dialog", { name: "Pull has diverged" })).toBeInTheDocument();
  });

  it("waits while the repository is busy, saying why, then merges once it is idle", async () => {
    const handlers = {
      getPullStrategy: vi.fn(async () => "Merge" as const),
      setPullStrategy: vi.fn(async () => {}),
      onMerge: vi.fn(),
      onRebase: vi.fn(),
      onCancel: vi.fn(),
    };
    const props = { upstreamRef: "refs/remotes/origin/main", branchName: "main", ...handlers };
    const { rerender } = render(
      <PendingPullPrompt {...props} operationDisabled operationDisabledReason="A transfer is in progress." />,
    );
    await screen.findByRole("dialog", { name: "Pull has diverged" });
    expect(screen.getByRole("button", { name: "Merge" })).toHaveAttribute("title", "A transfer is in progress.");
    expect(handlers.onMerge).not.toHaveBeenCalled();

    rerender(<PendingPullPrompt {...props} operationDisabled={false} operationDisabledReason={null} />);
    await waitFor(() => expect(handlers.onMerge).toHaveBeenCalledTimes(1));
    rerender(<PendingPullPrompt {...props} operationDisabled={false} operationDisabledReason={null} />);
    expect(handlers.onMerge).toHaveBeenCalledTimes(1);
  });
});

describe("PendingPullPrompt for a pull git config wants as a merge commit (#740)", () => {
  const noFastForward = { noFastForward: true };

  it.each([
    ["no strategy", null],
    ["Merge", "Merge" as const],
    ["an unreadable strategy", new Error("config unreadable")],
  ])("merges with a merge commit at once for %s, without asking", async (_, strategy) => {
    const { onMerge, onRebase } = renderPrompt(strategy, noFastForward);
    await waitFor(() => expect(onMerge).toHaveBeenCalledWith("refs/remotes/origin/main", "Never"));
    expect(onMerge).toHaveBeenCalledTimes(1);
    expect(onRebase).not.toHaveBeenCalled();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it.each([["Rebase" as const], ["FastForwardOnly" as const]])(
    "fast-forwards at once when the remembered strategy is %s",
    async (strategy) => {
      const { onMerge, onRebase } = renderPrompt(strategy, noFastForward);
      await waitFor(() => expect(onMerge).toHaveBeenCalledWith("refs/remotes/origin/main", "Only"));
      expect(onRebase).not.toHaveBeenCalled();
      expect(screen.queryByRole("dialog")).toBeNull();
    },
  );

  it("waits while the repository is busy, without the diverged dialog, then merges once", async () => {
    const handlers = {
      getPullStrategy: vi.fn(async () => null),
      setPullStrategy: vi.fn(async () => {}),
      onMerge: vi.fn(),
      onRebase: vi.fn(),
      onCancel: vi.fn(),
    };
    const props = { upstreamRef: "refs/remotes/origin/main", branchName: "main", noFastForward: true, ...handlers };
    const { rerender } = render(
      <PendingPullPrompt {...props} operationDisabled operationDisabledReason="A transfer is in progress." />,
    );
    await waitFor(() => expect(handlers.getPullStrategy).toHaveBeenCalled());
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(handlers.onMerge).not.toHaveBeenCalled();

    rerender(<PendingPullPrompt {...props} operationDisabled={false} operationDisabledReason={null} />);
    await waitFor(() => expect(handlers.onMerge).toHaveBeenCalledWith("refs/remotes/origin/main", "Never"));
    rerender(<PendingPullPrompt {...props} operationDisabled={false} operationDisabledReason={null} />);
    expect(handlers.onMerge).toHaveBeenCalledTimes(1);
  });
});

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

  it("asks instead of merging while the repository is busy", async () => {
    const { onMerge } = renderPrompt("Merge", { operationDisabled: true, operationDisabledReason: "A rebase is in progress." });
    await screen.findByRole("dialog", { name: "Pull has diverged" });
    expect(onMerge).not.toHaveBeenCalled();
  });
});

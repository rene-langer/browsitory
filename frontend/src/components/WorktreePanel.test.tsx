import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { BranchInfo, WorktreeInfo } from "../ipc/RepoClient";
import { WorktreePanel } from "./WorktreePanel";

const mainWorktree: WorktreeInfo = {
  name: "main",
  path: "/repos/project",
  head: "main",
  isMain: true,
  isLocked: false,
  isPrunable: false,
};

const linkedWorktree: WorktreeInfo = {
  name: "feature",
  path: "/repos/project-feature",
  head: "feature",
  isMain: false,
  isLocked: false,
  isPrunable: false,
};

const branches: BranchInfo[] = [
  { name: "main", isCurrent: true },
  { name: "feature", isCurrent: false },
];

function renderPanel(
  overrides: Partial<Parameters<typeof WorktreePanel>[0]> = {},
) {
  return render(
    <WorktreePanel
      worktrees={[mainWorktree, linkedWorktree]}
      branches={branches}
      onOpenWorktree={vi.fn()}
      onCreateWorktree={vi.fn().mockResolvedValue(undefined)}
      onRemoveWorktree={vi.fn().mockResolvedValue(undefined)}
      onPruneWorktrees={vi.fn().mockResolvedValue(undefined)}
      operationDisabled={false}
      {...overrides}
    />,
  );
}

describe("WorktreePanel", () => {
  it("renders main and linked worktree paths but disables removal for the main worktree", () => {
    renderPanel();

    expect(screen.getByText("/repos/project")).toBeInTheDocument();
    expect(screen.getByText("/repos/project-feature")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Remove /repos/project" }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "Remove /repos/project-feature" }),
    ).toBeEnabled();
  });

  it("forwards the name, path, branch, and start point supplied by the creation form", async () => {
    const onCreateWorktree = vi.fn().mockResolvedValue(undefined);
    renderPanel({ onCreateWorktree });

    fireEvent.change(screen.getByLabelText("Worktree name"), {
      target: { value: "feature-tree" },
    });
    fireEvent.change(screen.getByLabelText("Worktree path"), {
      target: { value: "/repos/feature-tree" },
    });
    fireEvent.change(screen.getByLabelText("Branch"), {
      target: { value: "feature-tree" },
    });
    fireEvent.change(screen.getByLabelText("Start point"), {
      target: { value: "main" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create worktree" }));

    expect(onCreateWorktree).toHaveBeenCalledWith(
      "feature-tree",
      "/repos/feature-tree",
      "feature-tree",
      "main",
    );
  });

  it("removes a linked worktree only after confirming the selected path", async () => {
    const onRemoveWorktree = vi.fn().mockResolvedValue(undefined);
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    renderPanel({ onRemoveWorktree });

    fireEvent.click(
      screen.getByRole("button", { name: "Remove /repos/project-feature" }),
    );

    expect(confirm).toHaveBeenCalledWith(
      "Remove worktree at /repos/project-feature?",
    );
    expect(onRemoveWorktree).not.toHaveBeenCalled();

    confirm.mockReturnValue(true);
    fireEvent.click(
      screen.getByRole("button", { name: "Remove /repos/project-feature" }),
    );

    expect(onRemoveWorktree).toHaveBeenCalledWith("feature");
    confirm.mockRestore();
  });
});

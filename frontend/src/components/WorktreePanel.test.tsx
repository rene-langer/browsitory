import { fireEvent, render, screen, within } from "@testing-library/react";
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
  localStorage.removeItem("sidebar-worktrees");
  const result = render(
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
  fireEvent.click(screen.getByRole("button", { name: "Worktrees" }));
  return result;
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

  it("requires an explicit dialog confirmation before removing the selected worktree", async () => {
    const onRemoveWorktree = vi.fn().mockResolvedValue(undefined);
    renderPanel({ onRemoveWorktree });

    fireEvent.click(screen.getByRole("button", { name: "Remove /repos/project-feature" }));
    const dialog = screen.getByRole("dialog", { name: "Remove worktree /repos/project-feature" });

    expect(within(dialog).getByText("Remove worktree at /repos/project-feature?")).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(onRemoveWorktree).not.toHaveBeenCalled();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Remove /repos/project-feature" }));
    fireEvent.click(
      within(screen.getByRole("dialog", { name: "Remove worktree /repos/project-feature" })).getByRole("button", { name: "Remove worktree" }),
    );

    expect(onRemoveWorktree).toHaveBeenCalledWith("feature");
  });
});

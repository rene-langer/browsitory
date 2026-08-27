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
      onCreateWorktree={vi.fn().mockResolvedValue(null)}
      onRemoveWorktree={vi.fn().mockResolvedValue(undefined)}
      onPruneWorktrees={vi.fn().mockResolvedValue(undefined)}
      operationDisabled={false}
      operationDisabledReason={null}
      {...overrides}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "Worktrees" }));
  return result;
}

describe("WorktreePanel", () => {
  it("shows an empty state when there are no worktrees", () => {
    renderPanel({ worktrees: [] });

    expect(screen.getByText(/No worktrees/)).toBeInTheDocument();
  });

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

  // Disabled buttons went inert with no explanation — issue #31/UX-003.
  it("explains why its controls are disabled via their title", () => {
    renderPanel({ operationDisabled: true, operationDisabledReason: "A merge is in progress." });

    expect(screen.getByRole("button", { name: "Create worktree" })).toHaveAttribute(
      "title",
      "A merge is in progress.",
    );
    expect(screen.getByRole("button", { name: "Open /repos/project" })).toHaveAttribute(
      "title",
      "A merge is in progress.",
    );
    expect(screen.getByRole("button", { name: "Prune worktrees" })).toHaveAttribute(
      "title",
      "A merge is in progress.",
    );
  });

  it("forwards the name, path, branch, and start point supplied by the creation form", async () => {
    const onCreateWorktree = vi.fn().mockResolvedValue(null);
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

  // `useAppState`'s `createWorktree` never rejects — it reports failure by resolving to the
  // message, the same contract `RemotePanel`'s `addRemote` established. See issue #30/UX-002.
  it("shows a failed create-worktree's message inline and keeps the entered values", async () => {
    const onCreateWorktree = vi.fn().mockResolvedValue("worktree path already exists");
    renderPanel({ onCreateWorktree });

    fireEvent.change(screen.getByLabelText("Worktree name"), { target: { value: "feature-tree" } });
    fireEvent.change(screen.getByLabelText("Worktree path"), { target: { value: "/repos/feature-tree" } });
    fireEvent.change(screen.getByLabelText("Branch"), { target: { value: "feature-tree" } });
    fireEvent.click(screen.getByRole("button", { name: "Create worktree" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("worktree path already exists");
    expect(screen.getByLabelText("Worktree name")).toHaveValue("feature-tree");
  });

  it("clears the create-worktree failure message once the name is edited again", async () => {
    const onCreateWorktree = vi.fn().mockResolvedValue("worktree path already exists");
    renderPanel({ onCreateWorktree });

    fireEvent.change(screen.getByLabelText("Worktree name"), { target: { value: "feature-tree" } });
    fireEvent.change(screen.getByLabelText("Worktree path"), { target: { value: "/repos/feature-tree" } });
    fireEvent.change(screen.getByLabelText("Branch"), { target: { value: "feature-tree" } });
    fireEvent.click(screen.getByRole("button", { name: "Create worktree" }));
    await screen.findByRole("alert");

    fireEvent.change(screen.getByLabelText("Worktree name"), { target: { value: "feature-tree-2" } });

    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
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

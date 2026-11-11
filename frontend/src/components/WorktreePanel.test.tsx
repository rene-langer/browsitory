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

  // `list_worktrees` lists an old entry whose folder was deleted and a newer one made there:
  // both rows render, with keys that stay unique (#413).
  it("renders two entries for one folder without a duplicate key", () => {
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
    const stale: WorktreeInfo = { ...linkedWorktree, name: "feature-old", head: "", isPrunable: true };
    try {
      renderPanel({ worktrees: [mainWorktree, stale, linkedWorktree] });

      expect(screen.getAllByText("/repos/project-feature")).toHaveLength(2);
      expect(consoleError.mock.calls.flat().join(" ")).not.toMatch(/same key/);
    } finally {
      consoleError.mockRestore();
    }
  });

  it("renders main and linked worktree paths but disables removal for the main worktree", () => {
    renderPanel();

    expect(screen.getByText("/repos/project")).toBeInTheDocument();
    expect(screen.getByText("/repos/project-feature")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Remove /repos/project" }),
    ).toHaveAttribute("aria-disabled", "true");
    expect(
      screen.getByRole("button", { name: "Remove /repos/project-feature" }),
    ).toBeEnabled();
  });

  // Disabled buttons went inert with no explanation — issue #31/UX-003.
  it("explains why its controls are disabled", () => {
    renderPanel({ operationDisabled: true, operationDisabledReason: "A merge is in progress." });

    expect(screen.getByRole("button", { name: "Create worktree" })).toHaveAttribute(
      "title",
      "A merge is in progress.",
    );
    // #748: the reason is the button's description, not only its title.
    const open = screen.getByRole("button", { name: "Open /repos/project" });
    expect(open).toHaveAttribute("aria-disabled", "true");
    expect(open).toHaveAccessibleDescription("A merge is in progress.");
    const prune = screen.getByRole("button", { name: "Prune worktrees" });
    expect(prune).toHaveAttribute("aria-disabled", "true");
    expect(prune).toHaveAccessibleDescription("A merge is in progress.");
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

  // #561: the repository can become busy while the confirmation is open.
  it("says why the confirmation can't remove once the repository became busy", () => {
    const onRemoveWorktree = vi.fn().mockResolvedValue(undefined);
    const { rerender } = renderPanel({ onRemoveWorktree });
    fireEvent.click(screen.getByRole("button", { name: "Remove /repos/project-feature" }));
    rerender(
      <WorktreePanel
        worktrees={[mainWorktree, linkedWorktree]}
        branches={branches}
        onOpenWorktree={vi.fn()}
        onCreateWorktree={vi.fn().mockResolvedValue(null)}
        onRemoveWorktree={onRemoveWorktree}
        onPruneWorktrees={vi.fn().mockResolvedValue(undefined)}
        operationDisabled
        operationDisabledReason="A merge is in progress."
      />,
    );
    const dialog = screen.getByRole("dialog", { name: "Remove worktree /repos/project-feature" });
    expect(within(dialog).getByText("A merge is in progress.")).toBeInTheDocument();
    const confirm = within(dialog).getByRole("button", { name: "Remove worktree" });
    expect(confirm).toBeDisabled();
    expect(confirm).toHaveAttribute("title", "A merge is in progress.");
    fireEvent.click(confirm);
    expect(onRemoveWorktree).not.toHaveBeenCalled();
  });
});

import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { BranchInfo, WorktreeInfo } from "../ipc/RepoClient";
import { WorktreePanel } from "./WorktreePanel";
import { describedByText } from "../test/describedBy";

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

/** Opens the ⋯ menu of the worktree at `path` (#1011). */
function openRowMenu(path: string) {
  fireEvent.click(screen.getByRole("button", { name: `Actions for ${path}` }));
}

/** Opens the New worktree dialog from the section's "+" (#1011). */
function openNewWorktreeDialog() {
  fireEvent.click(screen.getByRole("button", { name: "New worktree…" }));
  const dialog = screen.getByRole("dialog", { name: "New worktree", hidden: true });
  return { dialog, create: within(dialog).getByRole("button", { name: "Create worktree", hidden: true }) };
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

  it("renders main and linked worktree paths; the main worktree's menu has no Remove (#1011)", () => {
    renderPanel();

    expect(screen.getByText("/repos/project")).toBeInTheDocument();
    expect(screen.getByText("/repos/project-feature")).toBeInTheDocument();
    openRowMenu("/repos/project");
    expect(screen.getByRole("menuitem", { name: "Open in new tab" })).toBeEnabled();
    expect(screen.queryByRole("menuitem", { name: "Remove…" })).toBeNull();
    openRowMenu("/repos/project-feature");
    expect(screen.getByRole("menuitem", { name: "Remove…" })).toBeEnabled();
  });

  // Disabled buttons went inert with no explanation — issue #31/UX-003.
  it("explains why its controls are disabled", () => {
    renderPanel({ operationDisabled: true, operationDisabledReason: "A merge is in progress." });

    openRowMenu("/repos/project");
    const open = screen.getByRole("menuitem", { name: "Open in new tab" });
    expect(open).toBeDisabled();
    expect(open).toHaveAttribute("title", "A merge is in progress.");
    fireEvent.keyDown(open, { key: "Escape" });
    // #748: the reason is the button's description, not only its title.
    const prune = screen.getByRole("button", { name: "Prune worktrees" });
    expect(prune).toHaveAttribute("aria-disabled", "true");
    expect(prune).toHaveAccessibleDescription("A merge is in progress.");
    const { create } = openNewWorktreeDialog();
    expect(create).toHaveAttribute("title", "A merge is in progress.");
  });

  it("says why in visible text tied to the buttons (#748)", () => {
    const onPruneWorktrees = vi.fn().mockResolvedValue(undefined);
    renderPanel({ operationDisabled: true, operationDisabledReason: "A merge is in progress.", onPruneWorktrees });

    fireEvent.click(screen.getByRole("button", { name: "Prune worktrees" }));
    expect(onPruneWorktrees).not.toHaveBeenCalled();
    openRowMenu("/repos/project-feature");
    expect(screen.getByRole("menuitem", { name: "Remove…" })).toHaveAttribute("title", "A merge is in progress.");
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "Open in new tab" }), { key: "Escape" });
    const { dialog, create } = openNewWorktreeDialog();
    expect(describedByText(create)).toBe("A merge is in progress.");
    expect(dialog).toHaveTextContent("A merge is in progress.");
  });

  it("forwards the name, path, branch, and start point supplied by the creation form", async () => {
    const onCreateWorktree = vi.fn().mockResolvedValue(null);
    renderPanel({ onCreateWorktree });

    const { dialog, create } = openNewWorktreeDialog();
    fireEvent.change(within(dialog).getByLabelText("Worktree name"), {
      target: { value: "feature-tree" },
    });
    fireEvent.change(within(dialog).getByLabelText("Worktree path"), {
      target: { value: "/repos/feature-tree" },
    });
    fireEvent.change(within(dialog).getByLabelText("Branch"), {
      target: { value: "feature-tree" },
    });
    fireEvent.change(within(dialog).getByLabelText("Start point"), {
      target: { value: "main" },
    });
    fireEvent.click(create);

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

    const { dialog, create } = openNewWorktreeDialog();
    fireEvent.change(within(dialog).getByLabelText("Worktree name"), { target: { value: "feature-tree" } });
    fireEvent.change(within(dialog).getByLabelText("Worktree path"), { target: { value: "/repos/feature-tree" } });
    fireEvent.change(within(dialog).getByLabelText("Branch"), { target: { value: "feature-tree" } });
    fireEvent.click(create);

    expect(await within(dialog).findByRole("alert", { hidden: true })).toHaveTextContent("worktree path already exists");
    expect(within(dialog).getByLabelText("Worktree name")).toHaveValue("feature-tree");
  });

  it("clears the create-worktree failure message once the name is edited again", async () => {
    const onCreateWorktree = vi.fn().mockResolvedValue("worktree path already exists");
    renderPanel({ onCreateWorktree });

    const { dialog, create } = openNewWorktreeDialog();
    fireEvent.change(within(dialog).getByLabelText("Worktree name"), { target: { value: "feature-tree" } });
    fireEvent.change(within(dialog).getByLabelText("Worktree path"), { target: { value: "/repos/feature-tree" } });
    fireEvent.change(within(dialog).getByLabelText("Branch"), { target: { value: "feature-tree" } });
    fireEvent.click(create);
    await within(dialog).findByRole("alert", { hidden: true });

    fireEvent.change(within(dialog).getByLabelText("Worktree name"), { target: { value: "feature-tree-2" } });

    expect(within(dialog).queryByRole("alert", { hidden: true })).not.toBeInTheDocument();
  });

  it("lists the worktrees with no form fields in the section; + opens New worktree with hints (#1011)", () => {
    renderPanel();

    const section = screen.getByRole("region", { name: "Worktrees" });
    expect(within(section).queryAllByRole("textbox")).toHaveLength(0);
    const { dialog } = openNewWorktreeDialog();
    expect(within(dialog).getByLabelText("Branch")).toHaveAccessibleDescription(
      "An existing branch, or the name of a new one.",
    );
    expect(within(dialog).getByLabelText("Start point")).toHaveAccessibleDescription(
      "Where a new branch starts, such as HEAD or main. An existing branch doesn't need one.",
    );
    expect(within(dialog).getByLabelText("Worktree path")).toHaveAccessibleDescription(
      "The folder to check the branch out in. A relative path starts at this repository's folder.",
    );
    expect(within(dialog).getByLabelText("Worktree name")).toHaveAccessibleDescription(
      "Git's name for the worktree, usually the folder's name.",
    );
  });

  it("closes the New worktree dialog once the worktree is created, and on Cancel (#1011)", async () => {
    const onCreateWorktree = vi.fn().mockResolvedValue(null);
    renderPanel({ onCreateWorktree });

    let { dialog, create } = openNewWorktreeDialog();
    expect(create).toBeDisabled();
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel", hidden: true }));
    expect(screen.queryByRole("dialog", { name: "New worktree", hidden: true })).toBeNull();

    ({ dialog, create } = openNewWorktreeDialog());
    fireEvent.change(within(dialog).getByLabelText("Branch"), { target: { value: " topic " } });
    fireEvent.change(within(dialog).getByLabelText("Worktree path"), { target: { value: "../topic" } });
    fireEvent.change(within(dialog).getByLabelText("Worktree name"), { target: { value: "topic" } });
    expect(create).toBeEnabled();
    fireEvent.click(create);

    expect(onCreateWorktree).toHaveBeenCalledWith("topic", "../topic", "topic", null);
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "New worktree", hidden: true })).toBeNull());
  });

  it("opens a worktree in a new tab from its menu (#1011)", () => {
    const onOpenWorktree = vi.fn().mockResolvedValue(undefined);
    renderPanel({ onOpenWorktree });

    openRowMenu("/repos/project-feature");
    fireEvent.click(screen.getByRole("menuitem", { name: "Open in new tab" }));
    expect(onOpenWorktree).toHaveBeenCalledWith("/repos/project-feature");
  });

  it("requires an explicit dialog confirmation before removing the selected worktree", async () => {
    const onRemoveWorktree = vi.fn().mockResolvedValue(undefined);
    renderPanel({ onRemoveWorktree });

    openRowMenu("/repos/project-feature");
    fireEvent.click(screen.getByRole("menuitem", { name: "Remove…" }));
    const dialog = screen.getByRole("dialog", { name: "Remove worktree /repos/project-feature" });

    expect(within(dialog).getByText("Remove worktree at /repos/project-feature?")).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(onRemoveWorktree).not.toHaveBeenCalled();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    openRowMenu("/repos/project-feature");
    fireEvent.click(screen.getByRole("menuitem", { name: "Remove…" }));
    fireEvent.click(
      within(screen.getByRole("dialog", { name: "Remove worktree /repos/project-feature" })).getByRole("button", { name: "Remove worktree" }),
    );

    expect(onRemoveWorktree).toHaveBeenCalledWith("feature");
  });

  // #561: the repository can become busy while the confirmation is open.
  it("says why the confirmation can't remove once the repository became busy", () => {
    const onRemoveWorktree = vi.fn().mockResolvedValue(undefined);
    const { rerender } = renderPanel({ onRemoveWorktree });
    openRowMenu("/repos/project-feature");
    fireEvent.click(screen.getByRole("menuitem", { name: "Remove…" }));
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
    expect(describedByText(confirm)).toBe("A merge is in progress.");
    fireEvent.click(confirm);
    expect(onRemoveWorktree).not.toHaveBeenCalled();
  });
});

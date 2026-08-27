import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { BranchInfo } from "../ipc/RepoClient";
import { BranchSwitcher } from "./BranchSwitcher";

const branches: BranchInfo[] = [
  { name: "main", isCurrent: true },
  { name: "feature", isCurrent: false },
];

function renderSwitcher(overrides: Partial<Parameters<typeof BranchSwitcher>[0]> = {}) {
  localStorage.removeItem("sidebar-branches");
  const result = render(
    <BranchSwitcher
      branches={branches}
      createBranchDraft={null}
      onSwitchBranch={vi.fn()}
      onCreateBranch={vi.fn()}
      onDeleteBranch={vi.fn()}
      onRenameBranch={vi.fn()}
      onOpenCreateBranchDraft={vi.fn()}
      onCloseCreateBranchDraft={vi.fn()}
      onMergeBranch={vi.fn()}
      isMerging={false}
      isRebasing={false}
      operationDisabled={false}
      operationDisabledReason={null}
      graphBranchSelection={null}
      onSetGraphBranchSelection={vi.fn()}
      {...overrides}
    />,
  );
  return result;
}

describe("BranchSwitcher", () => {
  it("shows the current branch name on the toggle button", () => {
    renderSwitcher();

    expect(screen.getByRole("button", { name: "Branch switcher" })).toHaveTextContent("main");
  });

  it("opening the switcher lists all branches, clicking one switches", () => {
    const onSwitchBranch = vi.fn();
    renderSwitcher({ onSwitchBranch });

    fireEvent.click(screen.getByRole("button", { name: "Branch switcher" }));
    fireEvent.click(screen.getByText("feature"));

    expect(onSwitchBranch).toHaveBeenCalledWith("feature");
  });

  it("New Branch… opens the create-branch draft with startPoint HEAD", () => {
    const onOpenCreateBranchDraft = vi.fn();
    renderSwitcher({ onOpenCreateBranchDraft });

    fireEvent.click(screen.getByRole("button", { name: "Branch switcher" }));
    fireEvent.click(screen.getByText("New Branch…"));

    expect(onOpenCreateBranchDraft).toHaveBeenCalledWith("HEAD");
  });

  it("a non-null createBranchDraft shows the create form; submitting calls onCreateBranch with its startPoint", async () => {
    const onCreateBranch = vi.fn().mockResolvedValue(null);
    renderSwitcher({ createBranchDraft: { startPoint: "abc123" }, onCreateBranch });

    fireEvent.change(screen.getByPlaceholderText("New branch name"), {
      target: { value: "my-feature" },
    });
    fireEvent.click(screen.getByText("Create"));

    expect(onCreateBranch).toHaveBeenCalledWith("my-feature", "abc123");
    await waitFor(() => expect(screen.getByPlaceholderText("New branch name")).toHaveValue(""));
  });

  // `useAppState`'s `createBranch` never rejects — it reports failure by resolving to the
  // message (see its comment there), the same contract `RemotePanel`'s `addRemote` already
  // established. See issue #30/UX-002.
  it("shows a failed create-branch's message next to the draft form and keeps the entered name", async () => {
    const onCreateBranch = vi.fn().mockResolvedValue("branch already exists");
    renderSwitcher({ createBranchDraft: { startPoint: "abc123" }, onCreateBranch });

    fireEvent.change(screen.getByPlaceholderText("New branch name"), {
      target: { value: "feature" },
    });
    fireEvent.click(screen.getByText("Create"));

    expect(await screen.findByRole("alert")).toHaveTextContent("branch already exists");
    expect(screen.getByPlaceholderText("New branch name")).toHaveValue("feature");
  });

  it("clears the create-branch failure message once the name is edited again", async () => {
    const onCreateBranch = vi.fn().mockResolvedValue("branch already exists");
    renderSwitcher({ createBranchDraft: { startPoint: "abc123" }, onCreateBranch });

    fireEvent.change(screen.getByPlaceholderText("New branch name"), {
      target: { value: "feature" },
    });
    fireEvent.click(screen.getByText("Create"));
    await screen.findByRole("alert");

    fireEvent.change(screen.getByPlaceholderText("New branch name"), {
      target: { value: "feature-2" },
    });

    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("dismissing the create-branch failure message clears it", async () => {
    const onCreateBranch = vi.fn().mockResolvedValue("branch already exists");
    renderSwitcher({ createBranchDraft: { startPoint: "abc123" }, onCreateBranch });

    fireEvent.change(screen.getByPlaceholderText("New branch name"), {
      target: { value: "feature" },
    });
    fireEvent.click(screen.getByText("Create"));
    await screen.findByRole("alert");

    fireEvent.click(screen.getByRole("button", { name: "Dismiss error" }));

    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("Cancel in the create form calls onCloseCreateBranchDraft", () => {
    const onCloseCreateBranchDraft = vi.fn();
    renderSwitcher({ createBranchDraft: { startPoint: "HEAD" }, onCloseCreateBranchDraft });

    fireEvent.click(screen.getByText("Cancel"));

    expect(onCloseCreateBranchDraft).toHaveBeenCalled();
  });

  it("with no saved selection, every branch's graph checkbox is checked by default", () => {
    renderSwitcher();

    fireEvent.click(screen.getByRole("button", { name: "Branch switcher" }));

    expect(screen.getByRole("checkbox", { name: "Show main in graph" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Show feature in graph" })).toBeChecked();
  });

  it("unchecking a branch while showing all calls onSetGraphBranchSelection with every other branch", () => {
    const onSetGraphBranchSelection = vi.fn();
    renderSwitcher({ onSetGraphBranchSelection });

    fireEvent.click(screen.getByRole("button", { name: "Branch switcher" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "Show feature in graph" }));

    expect(onSetGraphBranchSelection).toHaveBeenCalledWith(["main"]);
  });

  it("a branch absent from an explicit selection renders unchecked; checking it adds it back", () => {
    const onSetGraphBranchSelection = vi.fn();
    renderSwitcher({ graphBranchSelection: ["main"], onSetGraphBranchSelection });

    fireEvent.click(screen.getByRole("button", { name: "Branch switcher" }));

    expect(screen.getByRole("checkbox", { name: "Show main in graph" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Show feature in graph" })).not.toBeChecked();

    fireEvent.click(screen.getByRole("checkbox", { name: "Show feature in graph" }));

    expect(onSetGraphBranchSelection).toHaveBeenCalledWith(["main", "feature"]);
  });

  it("clicking Delete once calls onDeleteBranch with force=false; confirming the force-delete dialog forces it", async () => {
    const onDeleteBranch = vi.fn().mockResolvedValue(undefined);
    renderSwitcher({ onDeleteBranch });

    fireEvent.click(screen.getByRole("button", { name: "Branch switcher" }));
    const deleteButtons = screen.getAllByText("Delete");
    fireEvent.click(deleteButtons[1]); // "feature" row — index 1 in the branches fixture above
    await Promise.resolve();

    expect(onDeleteBranch).toHaveBeenCalledWith("feature", false);

    // Since `branches` prop is unchanged (delete didn't actually remove it, as this fixture's
    // parent never updates the prop), a force-delete confirmation dialog opens for "feature".
    const dialog = await screen.findByRole("dialog", { name: "Force delete feature" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Force Delete" }));

    expect(onDeleteBranch).toHaveBeenCalledWith("feature", true);
  });

  it("Cancel in the force-delete dialog dismisses it without deleting, leaving Delete in place", async () => {
    const onDeleteBranch = vi.fn().mockResolvedValue(undefined);
    renderSwitcher({ onDeleteBranch });

    fireEvent.click(screen.getByRole("button", { name: "Branch switcher" }));
    fireEvent.click(screen.getAllByText("Delete")[1]);
    await Promise.resolve();

    const dialog = await screen.findByRole("dialog", { name: "Force delete feature" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));

    expect(onDeleteBranch).toHaveBeenCalledTimes(1);
    expect(onDeleteBranch).not.toHaveBeenCalledWith("feature", true);
    expect(screen.queryByRole("dialog", { name: "Force delete feature" })).not.toBeInTheDocument();
    expect(screen.getAllByText("Delete").length).toBe(2);
  });

  it("Rename shows an inline input; Enter calls onRenameBranch", () => {
    const onRenameBranch = vi.fn();
    renderSwitcher({ onRenameBranch });

    fireEvent.click(screen.getByRole("button", { name: "Branch switcher" }));
    const renameButtons = screen.getAllByText("Rename");
    fireEvent.click(renameButtons[1]); // "feature" row
    const input = screen.getByDisplayValue("feature");
    fireEvent.change(input, { target: { value: "feature-renamed" } });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(onRenameBranch).toHaveBeenCalledWith("feature", "feature-renamed");
  });

  it("Enter on an empty/whitespace-only rename value does not call onRenameBranch", () => {
    const onRenameBranch = vi.fn();
    renderSwitcher({ onRenameBranch });

    fireEvent.click(screen.getByRole("button", { name: "Branch switcher" }));
    const renameButtons = screen.getAllByText("Rename");
    fireEvent.click(renameButtons[1]); // "feature" row
    const input = screen.getByDisplayValue("feature");
    fireEvent.change(input, { target: { value: "   " } });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(onRenameBranch).not.toHaveBeenCalled();
  });

  it("closing the popover clears a pending force-delete so reopening shows Delete again", async () => {
    const onDeleteBranch = vi.fn().mockResolvedValue(undefined);
    renderSwitcher({ onDeleteBranch });

    fireEvent.click(screen.getByRole("button", { name: "Branch switcher" }));
    const deleteButtons = screen.getAllByText("Delete");
    fireEvent.click(deleteButtons[1]); // "feature" row
    await Promise.resolve();
    await screen.findByText("Force Delete");

    // Close the popover (toggle button) without acting on the pending force-delete.
    fireEvent.click(screen.getByRole("button", { name: "Branch switcher" }));
    // Reopen — as if the branch list changed underneath (e.g. delete succeeded, then a new
    // branch named "feature" was created) and reused the same name.
    fireEvent.click(screen.getByRole("button", { name: "Branch switcher" }));

    const deleteButtonsAfterReopen = screen.getAllByText("Delete");
    expect(deleteButtonsAfterReopen.length).toBe(2);
    expect(screen.queryByText("Force Delete")).toBeNull();
  });

  it("closing the popover clears an in-progress rename so reopening shows the button, not the input", () => {
    renderSwitcher();

    fireEvent.click(screen.getByRole("button", { name: "Branch switcher" }));
    const renameButtons = screen.getAllByText("Rename");
    fireEvent.click(renameButtons[1]); // "feature" row
    expect(screen.getByDisplayValue("feature")).toBeInTheDocument();

    // Close via the branch-switch button in the list, the other path that sets open=false.
    fireEvent.click(screen.getByRole("button", { name: /^main/ }));
    fireEvent.click(screen.getByRole("button", { name: "Branch switcher" }));

    expect(screen.queryByDisplayValue("feature")).toBeNull();
    expect(screen.getAllByText("Rename").length).toBe(2);
  });

  it("clicking Merge into current branch calls onMergeBranch with that branch's name", () => {
    const onMergeBranch = vi.fn();
    renderSwitcher({ onMergeBranch });

    fireEvent.click(screen.getByRole("button", { name: "Branch switcher" }));
    fireEvent.click(screen.getByText("Merge into current branch"));

    expect(onMergeBranch).toHaveBeenCalledWith("feature");
  });

  it("does not render a merge action for the current branch", () => {
    renderSwitcher({ branches: [{ name: "main", isCurrent: true }] });

    fireEvent.click(screen.getByRole("button", { name: "Branch switcher" }));

    expect(screen.queryByText("Merge into current branch")).not.toBeInTheDocument();
  });

  it("disables the merge action while a merge is already in progress", () => {
    renderSwitcher({ isMerging: true });

    fireEvent.click(screen.getByRole("button", { name: "Branch switcher" }));

    expect(screen.getByText("Merge into current branch")).toBeDisabled();
  });

  it("disables the merge action while another repository operation is pending", () => {
    renderSwitcher({ operationDisabled: true });

    fireEvent.click(screen.getByRole("button", { name: "Branch switcher" }));

    expect(screen.getByText("Merge into current branch")).toBeDisabled();
  });

  // Disabled buttons went inert with no explanation — issue #31/UX-003.
  it("explains why the merge action is disabled via its title", () => {
    renderSwitcher({ operationDisabled: true, operationDisabledReason: "A rebase is in progress." });

    fireEvent.click(screen.getByRole("button", { name: "Branch switcher" }));

    expect(screen.getByText("Merge into current branch")).toHaveAttribute(
      "title",
      "A rebase is in progress.",
    );
  });

  it("disables every branch-mutating action while a rebase is in progress", () => {
    // A rebase runs on a detached HEAD and only moves the original branch ref at the very end,
    // so switching/creating/deleting/renaming a branch mid-pause silently retargets an unrelated
    // branch — same reasoning as the merge gating above.
    renderSwitcher({ isRebasing: true, createBranchDraft: { startPoint: "HEAD" } });

    fireEvent.click(screen.getByRole("button", { name: "Branch switcher" }));

    expect(screen.getByRole("button", { name: /^main/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: /^feature/ })).toBeDisabled();
    for (const button of screen.getAllByText("Rename")) {
      expect(button).toBeDisabled();
    }
    for (const button of screen.getAllByText("Delete")) {
      expect(button).toBeDisabled();
    }
    expect(screen.getByText("Merge into current branch")).toBeDisabled();
    expect(screen.getByText("New Branch…")).toBeDisabled();

    fireEvent.change(screen.getByPlaceholderText("New branch name"), {
      target: { value: "my-feature" },
    });
    expect(screen.getByText("Create")).toBeDisabled();
  });

  it("does not switch branches while a rebase is in progress", () => {
    const onSwitchBranch = vi.fn();
    renderSwitcher({ isRebasing: true, onSwitchBranch });

    fireEvent.click(screen.getByRole("button", { name: "Branch switcher" }));
    fireEvent.click(screen.getByText("feature"));

    expect(onSwitchBranch).not.toHaveBeenCalled();
  });
});

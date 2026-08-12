import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { BranchInfo } from "../ipc/RepoClient";
import { BranchSwitcher } from "./BranchSwitcher";

const branches: BranchInfo[] = [
  { name: "main", isCurrent: true },
  { name: "feature", isCurrent: false },
];

function renderSwitcher(overrides: Partial<Parameters<typeof BranchSwitcher>[0]> = {}) {
  return render(
    <BranchSwitcher
      branches={branches}
      createBranchDraft={null}
      onSwitchBranch={vi.fn()}
      onCreateBranch={vi.fn()}
      onDeleteBranch={vi.fn()}
      onRenameBranch={vi.fn()}
      onOpenCreateBranchDraft={vi.fn()}
      onCloseCreateBranchDraft={vi.fn()}
      {...overrides}
    />,
  );
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

  it("a non-null createBranchDraft shows the create form; submitting calls onCreateBranch with its startPoint", () => {
    const onCreateBranch = vi.fn();
    renderSwitcher({ createBranchDraft: { startPoint: "abc123" }, onCreateBranch });

    fireEvent.change(screen.getByPlaceholderText("New branch name"), {
      target: { value: "my-feature" },
    });
    fireEvent.click(screen.getByText("Create"));

    expect(onCreateBranch).toHaveBeenCalledWith("my-feature", "abc123");
  });

  it("Cancel in the create form calls onCloseCreateBranchDraft", () => {
    const onCloseCreateBranchDraft = vi.fn();
    renderSwitcher({ createBranchDraft: { startPoint: "HEAD" }, onCloseCreateBranchDraft });

    fireEvent.click(screen.getByText("Cancel"));

    expect(onCloseCreateBranchDraft).toHaveBeenCalled();
  });

  it("clicking Delete once calls onDeleteBranch with force=false; a second click (still listed) forces it", async () => {
    const onDeleteBranch = vi.fn().mockResolvedValue(undefined);
    renderSwitcher({ onDeleteBranch });

    fireEvent.click(screen.getByRole("button", { name: "Branch switcher" }));
    const deleteButtons = screen.getAllByText("Delete");
    fireEvent.click(deleteButtons[1]); // "feature" row — index 1 in the branches fixture above
    await Promise.resolve();

    expect(onDeleteBranch).toHaveBeenCalledWith("feature", false);

    // Since `branches` prop is unchanged (delete didn't actually remove it, as this fixture's
    // parent never updates the prop), the row now shows "Force Delete" instead of "Delete".
    fireEvent.click(await screen.findByText("Force Delete"));

    expect(onDeleteBranch).toHaveBeenCalledWith("feature", true);
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
});

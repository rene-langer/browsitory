import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { BranchTree } from "./BranchTree";
import { renderTree, treeProps } from "../test/branchTreeProps";

describe("NewBranchForm (in BranchTree)", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("a non-null createBranchDraft shows the create form; submitting calls onCreateBranch with its startPoint", async () => {
    const { props } = renderTree({ createBranchDraft: { startPoint: "HEAD" } });
    fireEvent.change(screen.getByPlaceholderText("New branch name"), { target: { value: "feat/new" } });
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    expect(props.onCreateBranch).toHaveBeenCalledWith("feat/new", "HEAD", true);
  });

  // Ported from BranchSwitcher.test.tsx — the create-form failure/dismiss flow doesn't involve
  // the popover at all, so it transfers unchanged (createBranchDraft renders the form directly).
  it("shows a failed create-branch's message next to the draft form and keeps the entered name", async () => {
    const onCreateBranch = vi.fn().mockResolvedValue("branch already exists");
    renderTree({ createBranchDraft: { startPoint: "abc123" }, onCreateBranch });

    fireEvent.change(screen.getByPlaceholderText("New branch name"), {
      target: { value: "feature" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("branch already exists");
    expect(screen.getByPlaceholderText("New branch name")).toHaveValue("feature");
  });

  it("clears the create-branch failure message once the name is edited again", async () => {
    const onCreateBranch = vi.fn().mockResolvedValue("branch already exists");
    renderTree({ createBranchDraft: { startPoint: "abc123" }, onCreateBranch });

    fireEvent.change(screen.getByPlaceholderText("New branch name"), {
      target: { value: "feature" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    await screen.findByRole("alert");

    fireEvent.change(screen.getByPlaceholderText("New branch name"), {
      target: { value: "feature-2" },
    });

    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("dismissing the create-branch failure message clears it", async () => {
    const onCreateBranch = vi.fn().mockResolvedValue("branch already exists");
    renderTree({ createBranchDraft: { startPoint: "abc123" }, onCreateBranch });

    fireEvent.change(screen.getByPlaceholderText("New branch name"), {
      target: { value: "feature" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    await screen.findByRole("alert");

    fireEvent.click(screen.getByRole("button", { name: "Dismiss error" }));

    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("Cancel in the create form calls onCloseCreateBranchDraft", () => {
    const onCloseCreateBranchDraft = vi.fn();
    renderTree({ createBranchDraft: { startPoint: "HEAD" }, onCloseCreateBranchDraft });

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(onCloseCreateBranchDraft).toHaveBeenCalled();
  });

  it("focuses the new-branch input on open and Escape cancels", () => {
    const { props } = renderTree({ createBranchDraft: { startPoint: "HEAD" } });
    const input = screen.getByPlaceholderText("New branch name");
    expect(input).toHaveFocus();
    fireEvent.keyDown(input, { key: "Escape" });
    expect(props.onCloseCreateBranchDraft).toHaveBeenCalled();
  });

  it("shows the base branch for HEAD and a short id for a commit", () => {
    const { unmount } = renderTree({ createBranchDraft: { startPoint: "HEAD" } });
    expect(screen.getByText(/New branch from/)).toHaveTextContent("New branch from main");
    unmount();
    renderTree({ createBranchDraft: { startPoint: "abcdef1234567890abcdef1234567890abcdef12" } });
    expect(screen.getByText(/New branch from/)).toHaveTextContent("New branch from abcdef1");
  });

  it("validates the name inline and blocks Create, tied to the field", () => {
    const { props } = renderTree({ createBranchDraft: { startPoint: "HEAD" } });
    const input = screen.getByRole("textbox", { name: "Branch name" });
    fireEvent.change(input, { target: { value: "bad name" } });
    expect(input).toHaveAccessibleDescription("Branch names can't contain spaces.");
    expect(screen.getByRole("button", { name: "Create" })).toBeDisabled();
    expect(props.onCreateBranch).not.toHaveBeenCalled();
  });

  // #656: checked out by default, in the create itself (one undo step), and remembered.
  it("checks out the new branch by default, and only creates it when unticked, remembering that", async () => {
    const { props, unmount } = renderTree({ createBranchDraft: { startPoint: "HEAD" } });
    const checkbox = screen.getByRole("checkbox", { name: "Check out after creating" });
    expect(checkbox).toBeChecked();
    fireEvent.click(checkbox);
    fireEvent.change(screen.getByRole("textbox", { name: "Branch name" }), { target: { value: "feat/x" } });
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    await waitFor(() => expect(props.onCreateBranch).toHaveBeenCalledWith("feat/x", "HEAD", false));
    expect(props.onSwitchBranch).not.toHaveBeenCalled();
    unmount();
    renderTree({ createBranchDraft: { startPoint: "HEAD" } });
    expect(screen.getByRole("checkbox", { name: "Check out after creating" })).not.toBeChecked();
  });

  // #561: Create follows `operationBlockers`, not only a rebase run here.
  it("disables Create with the reason whenever the repository is busy", () => {
    const { props } = renderTree({
      createBranchDraft: { startPoint: "HEAD" },
      operationDisabled: true,
      operationDisabledReason: "A merge is in progress.",
    });
    fireEvent.change(screen.getByRole("textbox", { name: "Branch name" }), { target: { value: "feat/x" } });
    const create = screen.getByRole("button", { name: "Create" });
    expect(create).toBeDisabled();
    expect(create).toHaveAttribute("title", "A merge is in progress.");
    expect(within(screen.getByRole("form", { name: "New branch" })).getByText("A merge is in progress.")).toBeInTheDocument();
    fireEvent.submit(screen.getByRole("form", { name: "New branch" }));
    expect(props.onCreateBranch).not.toHaveBeenCalled();
  });

  it("re-checks the busy rule when the repository becomes busy while the form is open", () => {
    const props = treeProps({ createBranchDraft: { startPoint: "HEAD" } });
    const { rerender } = render(<BranchTree {...props} />);
    fireEvent.change(screen.getByRole("textbox", { name: "Branch name" }), { target: { value: "feat/x" } });
    rerender(<BranchTree {...props} operationDisabled operationDisabledReason="A transfer is in progress." />);
    expect(screen.getByRole("button", { name: "Create" })).toHaveAttribute("title", "A transfer is in progress.");
    fireEvent.submit(screen.getByRole("form", { name: "New branch" }));
    expect(props.onCreateBranch).not.toHaveBeenCalled();
    expect(props.onSwitchBranch).not.toHaveBeenCalled();
  });

  // #492: the form owns its fields but stays mounted while closed, as when `BranchTree` held them.
  it("keeps the typed name and the checkout option when the form is closed and reopened", () => {
    const props = treeProps({ createBranchDraft: { startPoint: "HEAD" } });
    const { rerender } = render(<BranchTree {...props} />);
    fireEvent.change(screen.getByRole("textbox", { name: "Branch name" }), { target: { value: "feat/x" } });
    fireEvent.click(screen.getByRole("checkbox", { name: "Check out after creating" }));
    rerender(<BranchTree {...props} createBranchDraft={null} />);
    expect(screen.queryByRole("form", { name: "New branch" })).toBeNull();
    rerender(<BranchTree {...props} />);
    expect(screen.getByRole("textbox", { name: "Branch name" })).toHaveValue("feat/x");
    expect(screen.getByRole("checkbox", { name: "Check out after creating" })).not.toBeChecked();
  });
});

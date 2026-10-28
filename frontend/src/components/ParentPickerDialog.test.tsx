import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { GraphCommit } from "../ipc/RepoClient";
import { ParentPickerDialog } from "./ParentPickerDialog";

function commit(id: string, overrides: Partial<GraphCommit> = {}): GraphCommit {
  return {
    id,
    shortId: id.slice(0, 7),
    summary: `summary ${id}`,
    authorName: "A",
    authorEmail: "a@example.com",
    timestamp: 0,
    parentIds: [],
    branchRefs: [],
    remoteBranchRefs: [],
    ...overrides,
  };
}

describe("ParentPickerDialog", () => {
  it("lists parents, defaults to parent 1, and confirms the chosen mainline", () => {
    const merge = commit("m", { parentIds: ["p1", "p2"] });
    const onConfirm = vi.fn();
    render(
      <ParentPickerDialog
        action="Revert"
        commit={merge}
        commits={[merge, commit("p1", { summary: "main work", branchRefs: ["main"] })]}
        onConfirm={onConfirm}
        onCancel={vi.fn()}
      />,
    );
    const dialog = screen.getByRole("dialog", { name: "Choose parent" });
    const group = within(dialog).getByRole("radiogroup", { name: "Mainline parent" });
    const radios = within(group).getAllByRole("radio");
    expect(radios).toHaveLength(2);
    expect(radios[0]).toBeChecked();
    expect(radios[0]).toHaveFocus();
    expect(within(group).getByText(/main work/)).toBeInTheDocument();
    expect(within(group).getByText("main")).toBeInTheDocument();
    // A parent the graph hasn't loaded still shows its number and short id.
    expect(within(group).getByText("Parent 2")).toBeInTheDocument();
    expect(within(group).getByText("p2")).toBeInTheDocument();
    expect(within(dialog).getByText("Parent 1 is usually the branch that was merged into.")).toBeInTheDocument();
    fireEvent.click(radios[1]);
    fireEvent.click(within(dialog).getByRole("button", { name: "Revert" }));
    expect(onConfirm).toHaveBeenCalledWith(2);
  });

  it("cancels without choosing", () => {
    const merge = commit("m", { parentIds: ["p1", "p2"] });
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    render(
      <ParentPickerDialog action="Cherry-pick" commit={merge} commits={[merge]} onConfirm={onConfirm} onCancel={onCancel} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalled();
    expect(onConfirm).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Cherry-pick" })).toBeInTheDocument();
  });
});

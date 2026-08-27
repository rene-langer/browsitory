import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { StashEntry } from "../ipc/RepoClient";
import { StashPanel } from "./StashPanel";

// Deliberately distinct message stems so `getByText` on one never matches the other.
const stashes: StashEntry[] = [
  { index: 0, message: "WIP on main: stash0fix uncommitted edit", commitId: "stash0" },
  { index: 1, message: "WIP on main: stash1fix earlier edit", commitId: "stash1" },
];

function renderPanel(overrides: Partial<Parameters<typeof StashPanel>[0]> = {}) {
  localStorage.removeItem("sidebar-stashes");
  const result = render(
    <StashPanel
      stashes={stashes}
      onSelectRow={vi.fn()}
      onApplyStash={vi.fn()}
      onDropStash={vi.fn()}
      operationDisabled={false}
      operationDisabledReason={null}
      {...overrides}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "Stashes" }));
  return result;
}

describe("StashPanel", () => {
  it("shows an empty state when there are no stashes", () => {
    renderPanel({ stashes: [] });

    expect(screen.getByText(/No stashes/)).toBeInTheDocument();
  });

  it("renders each stash's message", () => {
    renderPanel();

    expect(screen.getByText(/WIP on main: stash0fix uncommitted edit/)).toBeInTheDocument();
    expect(screen.getByText(/WIP on main: stash1fix earlier edit/)).toBeInTheDocument();
  });

  it("clicking a stash row calls onSelectRow with its commit id", () => {
    const onSelectRow = vi.fn();
    renderPanel({ onSelectRow });

    fireEvent.click(screen.getByText(/stash0fix/).closest("li")!);

    expect(onSelectRow).toHaveBeenCalledWith({ commitId: "stash0" });
  });

  it("clicking Apply on a stash row calls onApplyStash with its index, not onSelectRow", () => {
    const onApplyStash = vi.fn();
    const onSelectRow = vi.fn();
    renderPanel({ onApplyStash, onSelectRow });

    fireEvent.click(screen.getAllByText("Apply")[0]);

    expect(onApplyStash).toHaveBeenCalledWith(0);
    expect(onSelectRow).not.toHaveBeenCalled();
  });

  it("clicking Drop opens a confirmation naming the stash; confirming calls onDropStash with its index", () => {
    const onDropStash = vi.fn();
    const onSelectRow = vi.fn();
    renderPanel({ onDropStash, onSelectRow });

    fireEvent.click(screen.getAllByText("Drop")[1]);
    expect(onDropStash).not.toHaveBeenCalled();
    expect(onSelectRow).not.toHaveBeenCalled();

    const dialog = screen.getByRole("dialog", { name: /Drop stash/ });
    expect(within(dialog).getByText(/stash1fix earlier edit/)).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Drop stash" }));

    expect(onDropStash).toHaveBeenCalledWith(1);
  });

  it("Cancel in the drop confirmation dismisses it without dropping", () => {
    const onDropStash = vi.fn();
    renderPanel({ onDropStash });

    fireEvent.click(screen.getAllByText("Drop")[0]);
    const dialog = screen.getByRole("dialog", { name: /Drop stash/ });
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));

    expect(onDropStash).not.toHaveBeenCalled();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("disables every stash row's Apply and Drop buttons while an operation is pending", () => {
    renderPanel({ operationDisabled: true });

    for (const button of screen.getAllByText("Apply")) {
      expect(button).toBeDisabled();
    }
    for (const button of screen.getAllByText("Drop")) {
      expect(button).toBeDisabled();
    }
  });

  it("explains why the stash Apply/Drop buttons are disabled via their title", () => {
    renderPanel({ operationDisabled: true, operationDisabledReason: "A transfer is in progress." });

    for (const button of screen.getAllByText("Apply")) {
      expect(button).toHaveAttribute("title", "A transfer is in progress.");
    }
    for (const button of screen.getAllByText("Drop")) {
      expect(button).toHaveAttribute("title", "A transfer is in progress.");
    }
  });
});

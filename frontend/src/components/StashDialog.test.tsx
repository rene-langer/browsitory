import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { StashDialog } from "./StashDialog";

function renderDialog(paths: string[] = []) {
  const onStash = vi.fn();
  const onCancel = vi.fn();
  render(<StashDialog paths={paths} onStash={onStash} onCancel={onCancel} />);
  return { onStash, onCancel };
}

describe("StashDialog", () => {
  it("starts with an empty message, keep staged off and untracked on", () => {
    renderDialog();
    expect(screen.getByRole("dialog", { name: "Stash changes" })).toBeInTheDocument();
    const message = screen.getByRole("textbox", { name: "Stash message" });
    expect(message).toHaveValue("");
    expect(message).toHaveAttribute("placeholder", "Optional");
    expect(screen.getByRole("checkbox", { name: "Keep staged changes" })).not.toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Include untracked files" })).toBeChecked();
    expect(screen.queryByText(/^Only:/)).toBeNull();
  });

  it("stashes with a trimmed message and the chosen options", () => {
    const { onStash } = renderDialog();
    fireEvent.change(screen.getByRole("textbox", { name: "Stash message" }), { target: { value: " wip " } });
    fireEvent.click(screen.getByRole("button", { name: "Stash" }));
    expect(onStash).toHaveBeenCalledWith({ message: "wip", keepIndex: false, includeUntracked: true, paths: [] });
  });

  it("sends a null message when it is blank, and the toggled options", () => {
    const { onStash } = renderDialog();
    fireEvent.change(screen.getByRole("textbox", { name: "Stash message" }), { target: { value: "   " } });
    fireEvent.click(screen.getByRole("checkbox", { name: "Keep staged changes" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "Include untracked files" }));
    fireEvent.click(screen.getByRole("button", { name: "Stash" }));
    expect(onStash).toHaveBeenCalledWith({ message: null, keepIndex: true, includeUntracked: false, paths: [] });
  });

  it("names and passes the file of a file stash", () => {
    const { onStash } = renderDialog(["a.txt"]);
    expect(screen.getByText("Only: a.txt")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Stash" }));
    expect(onStash).toHaveBeenCalledWith({ message: null, keepIndex: false, includeUntracked: true, paths: ["a.txt"] });
  });

  it("cancels", () => {
    const { onCancel, onStash } = renderDialog();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalled();
    expect(onStash).not.toHaveBeenCalled();
  });
});

describe("StashDialog for an untracked file", () => {
  it("always includes untracked files and does not offer to leave them out", () => {
    const onStash = vi.fn();
    render(<StashDialog paths={["new.txt"]} untrackedOnly onStash={onStash} onCancel={() => {}} />);

    expect(screen.queryByRole("checkbox", { name: "Include untracked files" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Stash" }));

    expect(onStash).toHaveBeenCalledWith({ message: null, keepIndex: false, includeUntracked: true, paths: ["new.txt"] });
  });
});

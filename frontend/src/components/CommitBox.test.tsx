import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { CommitBox } from "./CommitBox";

describe("CommitBox", () => {
  it("Commit button is disabled when disabled=true", () => {
    render(<CommitBox onCommit={vi.fn()} disabled={true} />);

    expect(screen.getByText("Commit")).toBeDisabled();
  });

  it("Commit button is disabled with an empty message even when disabled=false", () => {
    render(<CommitBox onCommit={vi.fn()} disabled={false} />);

    expect(screen.getByText("Commit")).toBeDisabled();
  });

  it("typing a message and clicking Commit calls onCommit and clears the textarea", () => {
    const onCommit = vi.fn();
    render(<CommitBox onCommit={onCommit} disabled={false} />);

    const textarea = screen.getByPlaceholderText("Commit message");
    fireEvent.change(textarea, { target: { value: "my message" } });
    fireEvent.click(screen.getByText("Commit"));

    expect(onCommit).toHaveBeenCalledWith("my message");
    expect(textarea).toHaveValue("");
  });

  it("Cmd/Ctrl+Enter in the textarea commits", () => {
    const onCommit = vi.fn();
    render(<CommitBox onCommit={onCommit} disabled={false} />);

    const textarea = screen.getByPlaceholderText("Commit message");
    fireEvent.change(textarea, { target: { value: "my message" } });
    fireEvent.keyDown(textarea, { key: "Enter", metaKey: true });

    expect(onCommit).toHaveBeenCalledWith("my message");
  });

  it("Cmd/Ctrl+Enter does nothing when disabled", () => {
    const onCommit = vi.fn();
    render(<CommitBox onCommit={onCommit} disabled={true} />);

    const textarea = screen.getByPlaceholderText("Commit message");
    fireEvent.change(textarea, { target: { value: "my message" } });
    fireEvent.keyDown(textarea, { key: "Enter", metaKey: true });

    expect(onCommit).not.toHaveBeenCalled();
  });
});

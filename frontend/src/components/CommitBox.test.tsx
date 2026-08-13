import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { CommitBox } from "./CommitBox";

describe("CommitBox", () => {
  it("Commit button is disabled when disabled=true", () => {
    render(<CommitBox onCommit={vi.fn()} disabled={true} onAbortMerge={vi.fn()} />);

    expect(screen.getByText("Commit")).toBeDisabled();
  });

  it("Commit button is disabled with an empty message even when disabled=false", () => {
    render(<CommitBox onCommit={vi.fn()} disabled={false} onAbortMerge={vi.fn()} />);

    expect(screen.getByText("Commit")).toBeDisabled();
  });

  it("typing a message and clicking Commit calls onCommit and clears the textarea", () => {
    const onCommit = vi.fn();
    render(<CommitBox onCommit={onCommit} disabled={false} onAbortMerge={vi.fn()} />);

    const textarea = screen.getByPlaceholderText("Commit message");
    fireEvent.change(textarea, { target: { value: "my message" } });
    fireEvent.click(screen.getByText("Commit"));

    expect(onCommit).toHaveBeenCalledWith("my message");
    expect(textarea).toHaveValue("");
  });

  it("Cmd/Ctrl+Enter in the textarea commits", () => {
    const onCommit = vi.fn();
    render(<CommitBox onCommit={onCommit} disabled={false} onAbortMerge={vi.fn()} />);

    const textarea = screen.getByPlaceholderText("Commit message");
    fireEvent.change(textarea, { target: { value: "my message" } });
    fireEvent.keyDown(textarea, { key: "Enter", metaKey: true });

    expect(onCommit).toHaveBeenCalledWith("my message");
  });

  it("Cmd/Ctrl+Enter does nothing when disabled", () => {
    const onCommit = vi.fn();
    render(<CommitBox onCommit={onCommit} disabled={true} onAbortMerge={vi.fn()} />);

    const textarea = screen.getByPlaceholderText("Commit message");
    fireEvent.change(textarea, { target: { value: "my message" } });
    fireEvent.keyDown(textarea, { key: "Enter", metaKey: true });

    expect(onCommit).not.toHaveBeenCalled();
  });

  it("pre-fills the message from initialMessage", () => {
    render(<CommitBox onCommit={vi.fn()} disabled={false} onAbortMerge={vi.fn()} initialMessage="Merge branch 'feature'" />);

    expect(screen.getByDisplayValue("Merge branch 'feature'")).toBeInTheDocument();
  });

  it("does not clobber a user edit if initialMessage is passed again unchanged", () => {
    const { rerender } = render(
      <CommitBox onCommit={vi.fn()} disabled={false} onAbortMerge={vi.fn()} initialMessage="Merge branch 'feature'" />,
    );
    fireEvent.change(screen.getByPlaceholderText("Commit message"), {
      target: { value: "edited by user" },
    });

    rerender(
      <CommitBox onCommit={vi.fn()} disabled={false} onAbortMerge={vi.fn()} initialMessage="Merge branch 'feature'" />,
    );

    expect(screen.getByDisplayValue("edited by user")).toBeInTheDocument();
  });

  it("renders an Abort merge button only when initialMessage is set, and calls onAbortMerge", () => {
    const onAbortMerge = vi.fn();
    const { rerender } = render(
      <CommitBox onCommit={vi.fn()} disabled={false} onAbortMerge={onAbortMerge} />,
    );
    expect(screen.queryByText("Abort merge")).not.toBeInTheDocument();

    rerender(
      <CommitBox
        onCommit={vi.fn()}
        disabled={false}
        onAbortMerge={onAbortMerge}
        initialMessage="Merge branch 'feature'"
      />,
    );
    fireEvent.click(screen.getByText("Abort merge"));

    expect(onAbortMerge).toHaveBeenCalled();
  });
});

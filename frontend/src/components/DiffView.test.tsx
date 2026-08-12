import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { DiffHunk } from "../ipc/RepoClient";
import { DiffView } from "./DiffView";

describe("DiffView", () => {
  it("renders each line's content", () => {
    const hunks: DiffHunk[] = [
      {
        oldStart: 1,
        oldLines: 2,
        newStart: 1,
        newLines: 3,
        lines: [
          { origin: "Context", content: "unchanged" },
          { origin: "Remove", content: "old value" },
          { origin: "Add", content: "new value" },
        ],
      },
    ];

    render(<DiffView hunks={hunks} />);

    expect(screen.getByText(/unchanged/)).toBeInTheDocument();
    expect(screen.getByText(/old value/)).toBeInTheDocument();
    expect(screen.getByText(/new value/)).toBeInTheDocument();
  });

  it("added and removed lines get distinct CSS classes", () => {
    const hunks: DiffHunk[] = [
      {
        oldStart: 1,
        oldLines: 2,
        newStart: 1,
        newLines: 3,
        lines: [
          { origin: "Context", content: "unchanged" },
          { origin: "Remove", content: "old value" },
          { origin: "Add", content: "new value" },
        ],
      },
    ];

    render(<DiffView hunks={hunks} />);

    const removedLine = screen
      .getByText("old value", { exact: false })
      .closest(".diff-line");
    const addedLine = screen
      .getByText("new value", { exact: false })
      .closest(".diff-line");

    expect(removedLine).toHaveClass("diff-line-remove");
    expect(addedLine).toHaveClass("diff-line-add");
  });

  it("renders a message when there are no hunks", () => {
    render(<DiffView hunks={[]} />);

    expect(screen.getByText("No differences")).toBeInTheDocument();
  });
});

import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { BlameLine } from "../ipc/RepoClient";
import { BlameView } from "./BlameView";

const lines: BlameLine[] = [
  {
    lineNumber: 1,
    content: "line one",
    commitId: "aaa111",
    shortId: "aaa1111",
    authorName: "Rene",
    timestamp: 1,
  },
  {
    lineNumber: 2,
    content: "line two",
    commitId: "bbb222",
    shortId: "bbb2222",
    authorName: "Someone",
    timestamp: 2,
  },
];

describe("BlameView", () => {
  it("renders each line's number, short id, author, and content", () => {
    render(<BlameView lines={lines} onSelectRow={vi.fn()} />);

    expect(screen.getByText("aaa1111")).toBeInTheDocument();
    expect(screen.getByText("Rene")).toBeInTheDocument();
    expect(screen.getByText("line one")).toBeInTheDocument();
    expect(screen.getByText("bbb2222")).toBeInTheDocument();
    expect(screen.getByText("Someone")).toBeInTheDocument();
    expect(screen.getByText("line two")).toBeInTheDocument();
  });

  it("clicking a line calls onSelectRow with that line's commit id", () => {
    const onSelectRow = vi.fn();
    render(<BlameView lines={lines} onSelectRow={onSelectRow} />);

    fireEvent.click(screen.getByText("line two").closest("tr")!);

    expect(onSelectRow).toHaveBeenCalledWith({ commitId: "bbb222" });
  });
});

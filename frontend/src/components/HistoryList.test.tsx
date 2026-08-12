import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { CommitInfo, StatusEntry } from "../ipc/RepoClient";
import { HistoryList } from "./HistoryList";

const status: StatusEntry[] = [
  { path: "src/main.rs", staged: false, kind: "Modified" },
  { path: "README.md", staged: true, kind: "New" },
];

const log: CommitInfo[] = [
  {
    id: "aaa111...",
    shortId: "aaa1111",
    summary: "second commit",
    authorName: "Rene",
    authorEmail: "rene@example.com",
    timestamp: 2,
  },
  {
    id: "bbb222...",
    shortId: "bbb2222",
    summary: "first commit",
    authorName: "Rene",
    authorEmail: "rene@example.com",
    timestamp: 1,
  },
];

describe("HistoryList", () => {
  it("renders the Uncommitted Changes row with a change-count badge", () => {
    render(
      <HistoryList status={status} log={log} selectedRow="uncommitted" onSelectRow={vi.fn()} />,
    );

    expect(screen.getByText("Uncommitted Changes (2)")).toBeInTheDocument();
  });

  it("renders each commit's short id and summary", () => {
    render(
      <HistoryList status={status} log={log} selectedRow="uncommitted" onSelectRow={vi.fn()} />,
    );

    expect(screen.getByText(/aaa1111/)).toBeInTheDocument();
    expect(screen.getByText(/second commit/)).toBeInTheDocument();
    expect(screen.getByText(/bbb2222/)).toBeInTheDocument();
    expect(screen.getByText(/first commit/)).toBeInTheDocument();
  });

  it("clicking a commit row calls onSelectRow with that commit's id", () => {
    const onSelectRow = vi.fn();
    render(
      <HistoryList
        status={status}
        log={log}
        selectedRow="uncommitted"
        onSelectRow={onSelectRow}
      />,
    );

    fireEvent.click(screen.getByText(/second commit/).closest("li")!);

    expect(onSelectRow).toHaveBeenCalledWith({ commitId: "aaa111..." });
  });

  it("ArrowDown moves from Uncommitted Changes to the first commit", () => {
    const onSelectRow = vi.fn();
    render(
      <HistoryList
        status={status}
        log={log}
        selectedRow="uncommitted"
        onSelectRow={onSelectRow}
      />,
    );

    const list = screen.getByRole("list");
    fireEvent.keyDown(list, { key: "ArrowDown" });

    expect(onSelectRow).toHaveBeenCalledWith({ commitId: "aaa111..." });
  });

  it("ArrowUp from the first row does nothing (clamped, not wrapped)", () => {
    const onSelectRow = vi.fn();
    render(
      <HistoryList
        status={status}
        log={log}
        selectedRow="uncommitted"
        onSelectRow={onSelectRow}
      />,
    );

    const list = screen.getByRole("list");
    fireEvent.keyDown(list, { key: "ArrowUp" });

    expect(onSelectRow).toHaveBeenCalledWith("uncommitted");
  });

  it("ArrowDown from the last commit does nothing (clamped)", () => {
    const onSelectRow = vi.fn();
    render(
      <HistoryList
        status={status}
        log={log}
        selectedRow={{ commitId: "bbb222..." }}
        onSelectRow={onSelectRow}
      />,
    );

    const list = screen.getByRole("list");
    fireEvent.keyDown(list, { key: "ArrowDown" });

    expect(onSelectRow).toHaveBeenCalledWith({ commitId: "bbb222..." });
  });
});

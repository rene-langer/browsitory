import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { CommitInfo, StashEntry, StatusEntry } from "../ipc/RepoClient";
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
      <HistoryList
        status={status}
        log={log}
        stashes={[]}
        selectedRow="uncommitted"
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onApplyStash={vi.fn()}
        onDropStash={vi.fn()}
      />,
    );

    expect(screen.getByText("Uncommitted Changes (2)")).toBeInTheDocument();
  });

  it("renders each commit's short id and summary", () => {
    render(
      <HistoryList
        status={status}
        log={log}
        stashes={[]}
        selectedRow="uncommitted"
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onApplyStash={vi.fn()}
        onDropStash={vi.fn()}
      />,
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
        stashes={[]}
        selectedRow="uncommitted"
        onSelectRow={onSelectRow}
        onBranchFromCommit={vi.fn()}
        onApplyStash={vi.fn()}
        onDropStash={vi.fn()}
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
        stashes={[]}
        selectedRow="uncommitted"
        onSelectRow={onSelectRow}
        onBranchFromCommit={vi.fn()}
        onApplyStash={vi.fn()}
        onDropStash={vi.fn()}
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
        stashes={[]}
        selectedRow="uncommitted"
        onSelectRow={onSelectRow}
        onBranchFromCommit={vi.fn()}
        onApplyStash={vi.fn()}
        onDropStash={vi.fn()}
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
        stashes={[]}
        selectedRow={{ commitId: "bbb222..." }}
        onSelectRow={onSelectRow}
        onBranchFromCommit={vi.fn()}
        onApplyStash={vi.fn()}
        onDropStash={vi.fn()}
      />,
    );

    const list = screen.getByRole("list");
    fireEvent.keyDown(list, { key: "ArrowDown" });

    expect(onSelectRow).toHaveBeenCalledWith({ commitId: "bbb222..." });
  });

  it("right-clicking a commit row shows a 'Branch from here' menu entry", () => {
    render(
      <HistoryList
        status={status}
        log={log}
        stashes={[]}
        selectedRow="uncommitted"
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onApplyStash={vi.fn()}
        onDropStash={vi.fn()}
      />,
    );

    fireEvent.contextMenu(screen.getByText(/second commit/).closest("li")!);

    expect(screen.getByText("Branch from here")).toBeInTheDocument();
  });

  it("clicking 'Branch from here' calls onBranchFromCommit with that commit's id and closes the menu", () => {
    const onBranchFromCommit = vi.fn();
    render(
      <HistoryList
        status={status}
        log={log}
        stashes={[]}
        selectedRow="uncommitted"
        onSelectRow={vi.fn()}
        onBranchFromCommit={onBranchFromCommit}
        onApplyStash={vi.fn()}
        onDropStash={vi.fn()}
      />,
    );

    fireEvent.contextMenu(screen.getByText(/second commit/).closest("li")!);
    fireEvent.click(screen.getByText("Branch from here"));

    expect(onBranchFromCommit).toHaveBeenCalledWith("aaa111...");
    expect(screen.queryByText("Branch from here")).not.toBeInTheDocument();
  });

  it("right-clicking the Uncommitted Changes row does not show the menu", () => {
    render(
      <HistoryList
        status={status}
        log={log}
        stashes={[]}
        selectedRow="uncommitted"
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onApplyStash={vi.fn()}
        onDropStash={vi.fn()}
      />,
    );

    fireEvent.contextMenu(screen.getByText(/Uncommitted Changes/).closest("li")!);

    expect(screen.queryByText("Branch from here")).not.toBeInTheDocument();
  });

  // Deliberately distinct from `log`'s "aaa1111"/"bbb2222" shortIds above — a stash message
  // containing either substring would make `getByText` match both the stash row and the
  // colliding commit row and throw on the ambiguity.
  const stashes: StashEntry[] = [
    { index: 0, message: "WIP on main: stash0fix uncommitted edit", commitId: "stash0" },
    { index: 1, message: "WIP on main: stash1fix earlier edit", commitId: "stash1" },
  ];

  it("renders each stash's message", () => {
    render(
      <HistoryList
        status={status}
        log={log}
        stashes={stashes}
        selectedRow="uncommitted"
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onApplyStash={vi.fn()}
        onDropStash={vi.fn()}
      />,
    );

    expect(screen.getByText(/WIP on main: stash0fix uncommitted edit/)).toBeInTheDocument();
    expect(screen.getByText(/WIP on main: stash1fix earlier edit/)).toBeInTheDocument();
  });

  it("clicking a stash row calls onSelectRow with its commit id", () => {
    const onSelectRow = vi.fn();
    render(
      <HistoryList
        status={status}
        log={log}
        stashes={stashes}
        selectedRow="uncommitted"
        onSelectRow={onSelectRow}
        onBranchFromCommit={vi.fn()}
        onApplyStash={vi.fn()}
        onDropStash={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText(/stash0fix/).closest("li")!);

    expect(onSelectRow).toHaveBeenCalledWith({ commitId: "stash0" });
  });

  it("clicking Apply on a stash row calls onApplyStash with its index, not onSelectRow", () => {
    const onApplyStash = vi.fn();
    const onSelectRow = vi.fn();
    render(
      <HistoryList
        status={status}
        log={log}
        stashes={stashes}
        selectedRow="uncommitted"
        onSelectRow={onSelectRow}
        onBranchFromCommit={vi.fn()}
        onApplyStash={onApplyStash}
        onDropStash={vi.fn()}
      />,
    );

    const applyButtons = screen.getAllByText("Apply");
    fireEvent.click(applyButtons[0]);

    expect(onApplyStash).toHaveBeenCalledWith(0);
    expect(onSelectRow).not.toHaveBeenCalled();
  });

  it("clicking Drop on a stash row calls onDropStash with its index, not onSelectRow", () => {
    const onDropStash = vi.fn();
    const onSelectRow = vi.fn();
    render(
      <HistoryList
        status={status}
        log={log}
        stashes={stashes}
        selectedRow="uncommitted"
        onSelectRow={onSelectRow}
        onBranchFromCommit={vi.fn()}
        onApplyStash={vi.fn()}
        onDropStash={onDropStash}
      />,
    );

    const dropButtons = screen.getAllByText("Drop");
    fireEvent.click(dropButtons[1]);

    expect(onDropStash).toHaveBeenCalledWith(1);
    expect(onSelectRow).not.toHaveBeenCalled();
  });

  it("ArrowDown from Uncommitted Changes lands on the first stash when stashes are present", () => {
    const onSelectRow = vi.fn();
    render(
      <HistoryList
        status={status}
        log={log}
        stashes={stashes}
        selectedRow="uncommitted"
        onSelectRow={onSelectRow}
        onBranchFromCommit={vi.fn()}
        onApplyStash={vi.fn()}
        onDropStash={vi.fn()}
      />,
    );

    const list = screen.getByRole("list");
    fireEvent.keyDown(list, { key: "ArrowDown" });

    expect(onSelectRow).toHaveBeenCalledWith({ commitId: "stash0" });
  });
});

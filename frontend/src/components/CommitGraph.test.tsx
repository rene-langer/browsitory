import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { GraphCommit, StatusEntry } from "../ipc/RepoClient";
import { CommitGraph } from "./CommitGraph";

const status: StatusEntry[] = [
  { path: "src/main.rs", staged: false, kind: "Modified" },
  { path: "README.md", staged: true, kind: "New" },
];

const commits: GraphCommit[] = [
  {
    id: "aaa111...",
    shortId: "aaa1111",
    summary: "second commit",
    authorName: "Rene",
    authorEmail: "rene@example.com",
    timestamp: 2,
    parentIds: [],
    branchRefs: [],
  },
  {
    id: "bbb222...",
    shortId: "bbb2222",
    summary: "first commit",
    authorName: "Rene",
    authorEmail: "rene@example.com",
    timestamp: 1,
    parentIds: [],
    branchRefs: [],
  },
];

describe("CommitGraph", () => {
  it("exposes the row list as a labeled listbox, matching ListRow's option rows", () => {
    render(
      <CommitGraph
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        pending={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    expect(screen.getByRole("listbox", { name: "Commit history" })).toBeInTheDocument();
  });

  it("renders the Uncommitted Changes row with a change-count badge", () => {
    render(
      <CommitGraph
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        pending={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    expect(screen.getByText("Uncommitted Changes (2)")).toBeInTheDocument();
  });

  it("renders each commit's short id and summary", () => {
    render(
      <CommitGraph
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        pending={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
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
      <CommitGraph
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        pending={false}
        onSelectRow={onSelectRow}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText(/second commit/).closest("li")!);

    expect(onSelectRow).toHaveBeenCalledWith({ commitId: "aaa111..." });
  });

  it("ArrowDown moves from Uncommitted Changes to the first commit", () => {
    const onSelectRow = vi.fn();
    render(
      <CommitGraph
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        pending={false}
        onSelectRow={onSelectRow}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    const list = screen.getByRole("listbox");
    fireEvent.keyDown(list, { key: "ArrowDown" });

    expect(onSelectRow).toHaveBeenCalledWith({ commitId: "aaa111..." });
  });

  it("ArrowUp from the first row does nothing (clamped, not wrapped)", () => {
    const onSelectRow = vi.fn();
    render(
      <CommitGraph
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        pending={false}
        onSelectRow={onSelectRow}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    const list = screen.getByRole("listbox");
    fireEvent.keyDown(list, { key: "ArrowUp" });

    expect(onSelectRow).toHaveBeenCalledWith("uncommitted");
  });

  it("ArrowDown from the last commit does nothing (clamped)", () => {
    const onSelectRow = vi.fn();
    render(
      <CommitGraph
        status={status}
        commits={commits}
        selectedRow={{ commitId: "bbb222..." }}
        pending={false}
        onSelectRow={onSelectRow}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    const list = screen.getByRole("listbox");
    fireEvent.keyDown(list, { key: "ArrowDown" });

    expect(onSelectRow).toHaveBeenCalledWith({ commitId: "bbb222..." });
  });

  it("right-clicking a commit row shows a 'Branch from here' menu entry", () => {
    render(
      <CommitGraph
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        pending={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    fireEvent.contextMenu(screen.getByText(/second commit/).closest("li")!);

    expect(screen.getByText("Branch from here")).toBeInTheDocument();
  });

  it("clicking 'Branch from here' calls onBranchFromCommit with that commit's id and closes the menu", () => {
    const onBranchFromCommit = vi.fn();
    render(
      <CommitGraph
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        pending={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={onBranchFromCommit}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    fireEvent.contextMenu(screen.getByText(/second commit/).closest("li")!);
    fireEvent.click(screen.getByText("Branch from here"));

    expect(onBranchFromCommit).toHaveBeenCalledWith("aaa111...");
    expect(screen.queryByText("Branch from here")).not.toBeInTheDocument();
  });

  it("right-clicking the Uncommitted Changes row does not show the menu", () => {
    render(
      <CommitGraph
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        pending={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    fireEvent.contextMenu(screen.getByText(/Uncommitted Changes/).closest("li")!);

    expect(screen.queryByText("Branch from here")).not.toBeInTheDocument();
  });

  it("renders a branch badge for a commit that is a branch tip", () => {
    const commitsWithBranch: GraphCommit[] = [
      { ...commits[0], branchRefs: ["main"] },
      commits[1],
    ];
    render(
      <CommitGraph
        status={status}
        commits={commitsWithBranch}
        selectedRow="uncommitted"
        pending={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    expect(screen.getByText("main")).toBeInTheDocument();
  });

  it("renders a lane graphic for every commit row", () => {
    const { container } = render(
      <CommitGraph
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        pending={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    expect(container.querySelectorAll("li.commit-row svg").length).toBe(commits.length);
  });

  it("still renders each commit's short id and summary as plain text in its own li (hard E2E compatibility constraint)", () => {
    render(
      <CommitGraph
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        pending={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    const row = screen.getByText(/second commit/).closest("li");
    expect(row).not.toBeNull();
    expect(row?.tagName).toBe("LI");
    expect(row?.textContent).toContain("aaa1111 second commit");
  });

  it("still sets aria-selected on the selected commit's li (hard E2E compatibility constraint)", () => {
    render(
      <CommitGraph
        status={status}
        commits={commits}
        selectedRow={{ commitId: "aaa111..." }}
        pending={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    const row = screen.getByText(/second commit/).closest("li");
    expect(row).not.toBeNull();
    expect(row?.getAttribute("aria-selected")).toBe("true");
  });

  it("right-clicking a commit and choosing Rebase onto here calls onRebaseFromCommit", () => {
    const onRebaseFromCommit = vi.fn();
    render(
      <CommitGraph
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        pending={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={onRebaseFromCommit}
      />,
    );

    const row = screen.getByText(/second commit/).closest("li");
    fireEvent.contextMenu(row!);
    fireEvent.click(screen.getByText("Rebase onto here"));

    expect(onRebaseFromCommit).toHaveBeenCalledWith("aaa111...");
  });

  it("dims other lanes' lines while hovering a row, and undims on mouse leave", () => {
    // A fork: F1 and M2 are both children of M1, so F1 sits on lane 0 and M2 opens lane 1.
    const forkCommits: GraphCommit[] = [
      {
        id: "F1",
        shortId: "F1",
        summary: "F1",
        authorName: "Rene",
        authorEmail: "rene@example.com",
        timestamp: 3,
        parentIds: ["M1"],
        branchRefs: [],
      },
      {
        id: "M2",
        shortId: "M2",
        summary: "M2",
        authorName: "Rene",
        authorEmail: "rene@example.com",
        timestamp: 2,
        parentIds: ["M1"],
        branchRefs: [],
      },
      {
        id: "M1",
        shortId: "M1",
        summary: "M1",
        authorName: "Rene",
        authorEmail: "rene@example.com",
        timestamp: 1,
        parentIds: [],
        branchRefs: [],
      },
    ];

    render(
      <CommitGraph
        status={status}
        commits={forkCommits}
        selectedRow="uncommitted"
        pending={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    const m2Row = screen.getByText(/M2/).closest("li")!;
    fireEvent.mouseEnter(m2Row);

    const f1Row = screen.getByText(/F1/).closest("li")!;
    const f1Circle = f1Row.querySelector("circle")!;
    const m2Circle = m2Row.querySelector("circle")!;
    expect(f1Circle.getAttribute("opacity")).toBe("0.25");
    expect(m2Circle.getAttribute("opacity")).toBe("1");

    fireEvent.mouseLeave(m2Row);
    expect(f1Circle.getAttribute("opacity")).toBe("1");
  });

  it("shift-clicking a second commit and right-clicking within the range shows a Squash entry", () => {
    // A straight three-commit chain: C -> B -> A, oldest last (array order matches CommitGraph's
    // newest-first display order).
    const chainCommits: GraphCommit[] = [
      { ...commits[0], id: "C", shortId: "C", summary: "C", parentIds: ["B"] },
      { ...commits[0], id: "B", shortId: "B", summary: "B", parentIds: ["A"] },
      { ...commits[0], id: "A", shortId: "A", summary: "A", parentIds: [] },
    ];
    render(
      <CommitGraph
        status={status}
        commits={chainCommits}
        selectedRow="uncommitted"
        pending={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
        onSquashCommits={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText(/^C /).closest("li")!);
    fireEvent.click(screen.getByText(/^B /).closest("li")!, { shiftKey: true });
    fireEvent.contextMenu(screen.getByText(/^B /).closest("li")!);

    expect(screen.getByText("Squash 2 commits")).toBeInTheDocument();
    expect(screen.queryByText("Branch from here")).not.toBeInTheDocument();
  });

  it("clicking Squash N commits calls onSquashCommits with the base commit and the newer ids to fold in", () => {
    const chainCommits: GraphCommit[] = [
      { ...commits[0], id: "C", shortId: "C", summary: "C", parentIds: ["B"] },
      { ...commits[0], id: "B", shortId: "B", summary: "B", parentIds: ["A"] },
      { ...commits[0], id: "A", shortId: "A", summary: "A", parentIds: [] },
    ];
    const onSquashCommits = vi.fn();
    render(
      <CommitGraph
        status={status}
        commits={chainCommits}
        selectedRow="uncommitted"
        pending={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
        onSquashCommits={onSquashCommits}
      />,
    );

    fireEvent.click(screen.getByText(/^C /).closest("li")!);
    fireEvent.click(screen.getByText(/^B /).closest("li")!, { shiftKey: true });
    fireEvent.contextMenu(screen.getByText(/^B /).closest("li")!);
    fireEvent.click(screen.getByText("Squash 2 commits"));

    // C and B fold into B's parent A (the oldest selected commit stays as the surviving Pick).
    expect(onSquashCommits).toHaveBeenCalledWith("A", ["C"]);
  });

  it("does not offer Squash across a fork point (non-linear range)", () => {
    const forkCommits: GraphCommit[] = [
      { ...commits[0], id: "F1", shortId: "F1", summary: "F1", parentIds: ["M1"] },
      { ...commits[0], id: "M2", shortId: "M2", summary: "M2", parentIds: ["M1"] },
      { ...commits[0], id: "M1", shortId: "M1", summary: "M1", parentIds: [] },
    ];
    render(
      <CommitGraph
        status={status}
        commits={forkCommits}
        selectedRow="uncommitted"
        pending={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
        onSquashCommits={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText(/^F1 /).closest("li")!);
    fireEvent.click(screen.getByText(/^M2 /).closest("li")!, { shiftKey: true });
    fireEvent.contextMenu(screen.getByText(/^M2 /).closest("li")!);

    expect(screen.queryByText(/Squash \d+ commits/)).not.toBeInTheDocument();
    expect(screen.getByText("Branch from here")).toBeInTheDocument();
  });

  it("disables Rebase onto here while a repository operation is pending", () => {
    render(
      <CommitGraph
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        pending={true}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    fireEvent.contextMenu(screen.getByText(/second commit/).closest("li")!);

    expect(screen.getByText("Rebase onto here")).toBeDisabled();
  });
});

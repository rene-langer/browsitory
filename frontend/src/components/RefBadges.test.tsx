import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { GraphCommit } from "../ipc/RepoClient";
import { RefBadges } from "./RefBadges";
import styles from "./CommitGraph.module.css";

const commit: GraphCommit = {
  id: "a".repeat(40),
  shortId: "aaaaaaa",
  summary: "s",
  authorName: "A",
  authorEmail: "a@x",
  timestamp: 0,
  parentIds: [],
  branchRefs: ["main"],
  remoteBranchRefs: ["origin/main", "origin/old"],
  tags: ["v1.0", "v1.1"],
};

describe("RefBadges", () => {
  it("renders branch, filtered remote and tag badges", () => {
    render(<RefBadges commit={commit} shownRemoteBranches={new Set(["origin/main"])} />);
    expect(screen.getByText("main")).toHaveClass(styles.branchBadge);
    expect(screen.getByText("origin/main")).toHaveClass(styles.remoteBranchBadge);
    expect(screen.queryByText("origin/old")).toBeNull();
    expect(screen.getByTitle("Tag v1.0")).toHaveTextContent("v1.0");
    expect(screen.getByTitle("Tag v1.0")).toHaveClass(styles.tagBadge);
    expect(screen.getByTitle("Tag v1.1")).toBeInTheDocument();
  });

  it("shows every remote badge without a filter", () => {
    render(<RefBadges commit={commit} />);
    expect(screen.getByText("origin/old")).toBeInTheDocument();
  });

  it("treats a commit without tags as having none", () => {
    render(<RefBadges commit={{ ...commit, tags: undefined }} />);
    expect(screen.queryByTitle(/^Tag /)).toBeNull();
  });

  it("reports a right-click on a tag and stops it reaching the row", () => {
    const onTag = vi.fn();
    const onRow = vi.fn();
    render(
      <div onContextMenu={onRow}>
        <RefBadges commit={commit} onTagContextMenu={onTag} />
      </div>,
    );
    fireEvent.contextMenu(screen.getByTitle("Tag v1.1"));
    expect(onTag).toHaveBeenCalledWith("v1.1", expect.anything());
    expect(onRow).not.toHaveBeenCalled();
  });

  it("lets a tag right-click reach the row when there is no tag menu", () => {
    const onRow = vi.fn();
    render(
      <div onContextMenu={onRow}>
        <RefBadges commit={commit} />
      </div>,
    );
    fireEvent.contextMenu(screen.getByTitle("Tag v1.1"));
    expect(onRow).toHaveBeenCalled();
  });
});

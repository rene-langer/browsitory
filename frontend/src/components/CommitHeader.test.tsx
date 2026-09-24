import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { GraphCommit, RepoClient } from "../ipc/RepoClient";
import { CommitHeader } from "./CommitHeader";

const FULL_ID = "abc1234567890abcdef1234567890abcdef12345";
const PARENT_ID = "def4567890abcdef1234567890abcdef12345678";

const commit: GraphCommit = {
  id: FULL_ID,
  shortId: "abc1234",
  summary: "Fix the thing",
  authorName: "Jane Doe",
  authorEmail: "jane@example.com",
  timestamp: 1_700_000_000,
  parentIds: [PARENT_ID],
  branchRefs: [],
  remoteBranchRefs: [],
};

function clientWithMessage(message: string): RepoClient {
  return { getCommitMessage: vi.fn(async () => message) } as unknown as RepoClient;
}

function renderHeader(overrides: Partial<React.ComponentProps<typeof CommitHeader>> = {}) {
  return render(
    <CommitHeader
      repoPath="/repo"
      client={clientWithMessage("Fix the thing\n")}
      commitId={FULL_ID}
      commit={commit}
      knownCommitIds={new Set()}
      onSelectRow={() => {}}
      {...overrides}
    />,
  );
}

describe("CommitHeader", () => {
  it("shows author, full SHA, subject, body and parents", async () => {
    renderHeader({
      client: clientWithMessage("Fix the thing\n\nLonger explanation\nover two lines.\n"),
      knownCommitIds: new Set([PARENT_ID]),
    });

    expect(screen.getByText(/Jane Doe/)).toBeInTheDocument();
    expect(screen.getByText(/jane@example.com/)).toBeInTheDocument();
    expect(screen.getByText(FULL_ID)).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Fix the thing" })).toBeInTheDocument();
    expect(await screen.findByText(/Longer explanation/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: `Go to parent ${PARENT_ID.slice(0, 7)}` })).toBeInTheDocument();
  });

  it("selects a parent commit when its link is clicked", async () => {
    const onSelectRow = vi.fn();
    renderHeader({ knownCommitIds: new Set([PARENT_ID]), onSelectRow });

    fireEvent.click(screen.getByRole("button", { name: /Go to parent/ }));

    expect(onSelectRow).toHaveBeenCalledWith({ commitId: PARENT_ID });
  });

  it("renders a parent that is not in the loaded history as plain text", () => {
    renderHeader();

    expect(screen.queryByRole("button", { name: /Go to parent/ })).toBeNull();
    expect(screen.getByText(PARENT_ID.slice(0, 7))).toBeInTheDocument();
  });

  it("copies the full SHA", async () => {
    const writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    renderHeader();

    fireEvent.click(screen.getByRole("button", { name: "Copy full SHA" }));

    await waitFor(() => expect(writeText).toHaveBeenCalledWith(FULL_ID));
  });
});

import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { ConflictSegment, RepoClient } from "../ipc/RepoClient";
import { ConflictResolutionPane } from "./ConflictResolutionPane";

function unused(): never {
  throw new Error("not used in this test");
}

function fakeClient(overrides: Partial<RepoClient>): RepoClient {
  return {
    pickRepoFolder: unused,
    listRecentRepos: unused,
    openRepo: unused,
    getStatus: unused,
    getCommitGraph: unused,
    listBranches: unused,
    createBranch: unused,
    switchBranch: unused,
    deleteBranch: unused,
    renameBranch: unused,
    listStashes: unused,
    saveStash: unused,
    applyStash: unused,
    dropStash: unused,
    getBlame: unused,
    getWorkingDiff: unused,
    getCommitDiff: unused,
    getCommitFiles: unused,
    stageFile: unused,
    unstageFile: unused,
    commit: unused,
    mergeBranch: unused,
    getConflictHunks: unused,
    resolveConflict: unused,
    abortMerge: unused,
    getMergeMessage: unused,
    ...overrides,
  };
}

const segments: ConflictSegment[] = [
  { kind: "Clean", content: "line one" },
  { kind: "Conflict", ours: "main two", theirs: "feature two" },
  { kind: "Clean", content: "line three" },
];

describe("ConflictResolutionPane", () => {
  it("renders clean segments as text and conflict segments with both sides", async () => {
    const client = fakeClient({ getConflictHunks: async () => segments });

    render(<ConflictResolutionPane client={client} path="shared.txt" onResolve={vi.fn()} />);

    await waitFor(() => screen.getByText(/line one/));
    expect(screen.getByText(/main two/)).toBeInTheDocument();
    expect(screen.getByText(/feature two/)).toBeInTheDocument();
    expect(screen.getByText(/line three/)).toBeInTheDocument();
  });

  it("Save resolution defaults to ours for every conflict and calls onResolve with the joined text", async () => {
    const onResolve = vi.fn();
    const client = fakeClient({ getConflictHunks: async () => segments });

    render(<ConflictResolutionPane client={client} path="shared.txt" onResolve={onResolve} />);

    await waitFor(() => screen.getByText("Save resolution"));
    fireEvent.click(screen.getByText("Save resolution"));

    expect(onResolve).toHaveBeenCalledWith("shared.txt", "line one\nmain two\nline three");
  });

  it("Accept Theirs changes that conflict's contribution to the saved text", async () => {
    const onResolve = vi.fn();
    const client = fakeClient({ getConflictHunks: async () => segments });

    render(<ConflictResolutionPane client={client} path="shared.txt" onResolve={onResolve} />);

    await waitFor(() => screen.getByText("Accept Theirs"));
    fireEvent.click(screen.getByText("Accept Theirs"));
    fireEvent.click(screen.getByText("Save resolution"));

    expect(onResolve).toHaveBeenCalledWith("shared.txt", "line one\nfeature two\nline three");
  });

  it("Accept Both concatenates ours then theirs", async () => {
    const onResolve = vi.fn();
    const client = fakeClient({ getConflictHunks: async () => segments });

    render(<ConflictResolutionPane client={client} path="shared.txt" onResolve={onResolve} />);

    await waitFor(() => screen.getByText("Accept Both"));
    fireEvent.click(screen.getByText("Accept Both"));
    fireEvent.click(screen.getByText("Save resolution"));

    expect(onResolve).toHaveBeenCalledWith(
      "shared.txt",
      "line one\nmain two\nfeature two\nline three",
    );
  });

  it("shows an error message when the conflict is not a text conflict", async () => {
    const client = fakeClient({
      getConflictHunks: async () => {
        throw new Error("'binary.dat' is an add/delete conflict, not a text conflict");
      },
    });

    render(<ConflictResolutionPane client={client} path="binary.dat" onResolve={vi.fn()} />);

    await waitFor(() => screen.getByRole("alert"));
    expect(screen.getByRole("alert")).toHaveTextContent(/add\/delete conflict/);
  });
});

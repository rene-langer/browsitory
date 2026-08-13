import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RepoClient } from "../ipc/RepoClient";
import { RepoPicker } from "./RepoPicker";

function unimplemented(): never {
  throw new Error("not implemented in this fake");
}

function fakeClient(overrides: Partial<RepoClient>): RepoClient {
  return {
    pickRepoFolder: async () => unimplemented(),
    listRecentRepos: async () => unimplemented(),
    openRepo: async () => {},
    getStatus: async () => unimplemented(),
    getLog: async () => unimplemented(),
    listBranches: async () => unimplemented(),
    createBranch: async () => unimplemented(),
    switchBranch: async () => unimplemented(),
    deleteBranch: async () => unimplemented(),
    renameBranch: async () => unimplemented(),
    listStashes: async () => unimplemented(),
    saveStash: async () => unimplemented(),
    applyStash: async () => unimplemented(),
    dropStash: async () => unimplemented(),
    getBlame: async () => unimplemented(),
    getWorkingDiff: async () => unimplemented(),
    getCommitDiff: async () => unimplemented(),
    getCommitFiles: async () => unimplemented(),
    stageFile: async () => unimplemented(),
    unstageFile: async () => unimplemented(),
    commit: async () => unimplemented(),
    ...overrides,
  };
}

describe("RepoPicker", () => {
  it("renders each recent repo and opens it on click", async () => {
    const client = fakeClient({
      listRecentRepos: async () => ["/repo/a", "/repo/b"],
    });
    const onOpenRepo = vi.fn();

    render(<RepoPicker client={client} onOpenRepo={onOpenRepo} />);

    expect(await screen.findByText("/repo/a")).toBeInTheDocument();
    expect(await screen.findByText("/repo/b")).toBeInTheDocument();

    fireEvent.click(screen.getByText("/repo/a"));

    expect(onOpenRepo).toHaveBeenCalledTimes(1);
    expect(onOpenRepo).toHaveBeenCalledWith("/repo/a");
  });

  it("shows a message when there are no recent repos", async () => {
    const client = fakeClient({
      listRecentRepos: async () => [],
    });

    render(<RepoPicker client={client} onOpenRepo={vi.fn()} />);

    expect(await screen.findByText("No recent repositories")).toBeInTheDocument();
  });

  it("Open Folder button opens the picked path", async () => {
    const client = fakeClient({
      listRecentRepos: async () => [],
      pickRepoFolder: async () => "/picked/repo",
    });
    const onOpenRepo = vi.fn();

    render(<RepoPicker client={client} onOpenRepo={onOpenRepo} />);

    fireEvent.click(screen.getByText("Open Folder"));

    await waitFor(() => {
      expect(onOpenRepo).toHaveBeenCalledWith("/picked/repo");
    });
  });

  it("Open Folder button does nothing when the dialog is cancelled", async () => {
    const client = fakeClient({
      listRecentRepos: async () => [],
      pickRepoFolder: async () => null,
    });
    const onOpenRepo = vi.fn();

    render(<RepoPicker client={client} onOpenRepo={onOpenRepo} />);

    fireEvent.click(screen.getByText("Open Folder"));

    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(onOpenRepo).not.toHaveBeenCalled();
  });

  it("renders the error instead of the recent list when listRecentRepos rejects", async () => {
    const client = fakeClient({
      listRecentRepos: async () => {
        throw new Error("config unreadable");
      },
    });

    render(<RepoPicker client={client} onOpenRepo={vi.fn()} />);

    expect(await screen.findByRole("alert")).toHaveTextContent("config unreadable");
  });
});

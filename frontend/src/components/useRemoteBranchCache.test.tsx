import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { BranchTree } from "./BranchTree";
import { renderTree } from "../test/branchTreeProps";

const oneRemote = [{ name: "origin", fetchUrl: "git@github.com:user/repo.git", pushUrl: null, authMode: null, authUsername: null }];
const originOnly = [{ name: "origin", fetchUrl: "x", pushUrl: null, authMode: null, authUsername: null }];
const remote = [{ name: "origin", fetchUrl: "u", pushUrl: null, authMode: null, authUsername: null }];

describe("useRemoteBranchCache (in BranchTree)", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("lists remote folders and lazily loads their branches on first expand", async () => {
    const onListRemoteBranches = vi.fn().mockResolvedValue(["main", "feat/foo"]);
    renderTree({ remotes: oneRemote, onListRemoteBranches });
    expect(onListRemoteBranches).not.toHaveBeenCalled();
    const remoteFolder = screen.getByRole("button", { name: "origin" }).closest("li")!;
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    expect(onListRemoteBranches).toHaveBeenCalledWith("origin");
    // Scoped to the remote's own <li>, not `screen`: `baseBranches` (the default local branches)
    // already has a "feat/foo" branch, so an unscoped query would ambiguously match either row.
    expect(await within(remoteFolder).findByText("foo")).toBeInTheDocument();
  });

  it("does not re-fetch remote branches on a second expand", async () => {
    const onListRemoteBranches = vi.fn().mockResolvedValue(["main"]);
    renderTree({ remotes: oneRemote, onListRemoteBranches });
    fireEvent.click(screen.getByRole("button", { name: "origin" })); // expand
    await screen.findByText("main");
    fireEvent.click(screen.getByRole("button", { name: "origin" })); // collapse
    fireEvent.click(screen.getByRole("button", { name: "origin" })); // expand again
    expect(onListRemoteBranches).toHaveBeenCalledOnce();
  });

  // The lazily-fetched `remoteBranches[remoteName]` cache was never invalidated after Fetch, so
  // the tree kept showing pre-fetch branches for the rest of the session (folder open state is
  // localStorage-persisted, so not even a restart mid-session helped). Fetch must drop the cache
  // entry and, if the folder is currently open, re-fetch immediately.
  it("Fetch drops the cached remote branch list and re-fetches when the folder is open", async () => {
    const onListRemoteBranches = vi
      .fn()
      .mockResolvedValueOnce(["main"])
      .mockResolvedValueOnce(["main", "feat/new-on-remote"]);
    const onFetchRemote = vi.fn().mockResolvedValue(undefined);
    renderTree({ remotes: oneRemote, onListRemoteBranches, onFetchRemote });

    const remoteFolder = screen.getByRole("button", { name: "origin" }).closest("li")!;
    fireEvent.click(screen.getByRole("button", { name: "origin" })); // expand, first fetch
    await within(remoteFolder).findByText("main");
    expect(onListRemoteBranches).toHaveBeenCalledTimes(1);

    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Fetch" }));
    expect(onFetchRemote).toHaveBeenCalledWith("origin");

    expect(await within(remoteFolder).findByText("new-on-remote")).toBeInTheDocument();
    expect(onListRemoteBranches).toHaveBeenCalledTimes(2);
  });

  it("shows an inline error, without an unhandled rejection, when the lazy remote-branch fetch fails", async () => {
    const onListRemoteBranches = vi.fn().mockRejectedValue(new Error("network unreachable"));
    renderTree({ remotes: oneRemote, onListRemoteBranches });
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not list branches for origin.");
  });

  it("shows a non-interactive Loading row while a remote's branches load", async () => {
    renderTree({ remotes: remote, onListRemoteBranches: vi.fn().mockReturnValue(new Promise(() => {})) });
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    const row = await screen.findByText("Loading…");
    expect(row.closest("[role=button]")).toBeNull();
  });

  it("shows 'No branches' for an empty remote", async () => {
    renderTree({ remotes: remote, onListRemoteBranches: vi.fn().mockResolvedValue([]) });
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    expect(await screen.findByText("No branches")).toBeInTheDocument();
  });

  it("reloads open remote folders when refreshGeneration changes", async () => {
    const onListRemoteBranches = vi.fn().mockResolvedValueOnce(["main", "topic"]).mockResolvedValueOnce(["main"]);
    const { props, rerender } = renderTree({ remotes: originOnly, onListRemoteBranches });
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    expect(await screen.findByRole("button", { name: "Show origin/topic in graph" })).toBeInTheDocument();

    rerender(<BranchTree {...props} refreshGeneration={1} />);

    // No "Loading…" flash: the old list stays until the new one lands.
    expect(screen.queryByText("Loading…")).toBeNull();
    await waitFor(() => expect(screen.queryByRole("button", { name: "Show origin/topic in graph" })).toBeNull());
    expect(screen.getByRole("button", { name: "Show origin/main in graph" })).toBeInTheDocument();
    expect(onListRemoteBranches).toHaveBeenCalledTimes(2);
  });

  it("forgets a closed folder's list on refresh so the next expand reloads it", async () => {
    const onListRemoteBranches = vi.fn().mockResolvedValue(["main"]);
    const { props, rerender } = renderTree({ remotes: originOnly, onListRemoteBranches });
    fireEvent.click(screen.getByRole("button", { name: "origin" })); // expand
    await screen.findByRole("button", { name: "Show origin/main in graph" });
    fireEvent.click(screen.getByRole("button", { name: "origin" })); // collapse

    rerender(<BranchTree {...props} refreshGeneration={1} />);
    expect(onListRemoteBranches).toHaveBeenCalledOnce(); // closed folders are not reloaded eagerly
    fireEvent.click(screen.getByRole("button", { name: "origin" })); // expand again

    await waitFor(() => expect(onListRemoteBranches).toHaveBeenCalledTimes(2));
  });
});

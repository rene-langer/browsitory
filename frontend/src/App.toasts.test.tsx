import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import App from "./App";
import type { RepoClient, TransferProgress, UserCommand } from "./ipc/RepoClient";
import { EMPTY_RESULTS, fakeRepoClient, graphPage } from "./test/fakeRepoClient";

// #1116: one toast stack serves every mounted tab. A toast says which repository it is about
// while its tab isn't the one shown, and its buttons act on that repository only after showing
// its tab.
vi.setConfig({ testTimeout: 20_000 });

const origin = { name: "origin", fetchUrl: "https://example.com/r.git", pushUrl: null, authMode: null, authUsername: null };
const behind = { localBranch: "main", remoteName: "origin", remoteBranch: "main", ahead: 0, behind: 2, pushBranch: "main" };

function transferEvents() {
  const listeners = new Set<(progress: TransferProgress) => void>();
  const subscribeTransferProgress: RepoClient["subscribeTransferProgress"] = (callback) => {
    listeners.add(callback);
    return () => listeners.delete(callback);
  };
  const emit = (operationId: string, operation: TransferProgress["operation"], phase: TransferProgress["phase"], errorKind: TransferProgress["errorKind"] = null) =>
    act(() =>
      listeners.forEach((listener) =>
        listener({ operationId, operation, phase, errorKind, current: 0, total: 0, receivedBytes: 0, message: null, hookOutput: null }),
      ),
    );
  return { subscribeTransferProgress, emit };
}

/** Two tabs, a and b, both on main two commits behind origin/main; a is shown. */
function twoTabs(overrides: Partial<RepoClient> = {}): RepoClient {
  return fakeRepoClient(
    {
      listOpenRepos: async () => ({
        entries: [
          { path: "/repos/a", workspaceId: null },
          { path: "/repos/b", workspaceId: null },
        ],
        activePath: "/repos/a",
      }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [{ path: "x.txt", oldPath: null, staged: true, kind: "Modified" }],
      getCommitGraph: async () => graphPage([]),
      listBranches: async () => [{ name: "main", isCurrent: true, upstream: "origin/main", ahead: 0, behind: 2 }],
      listStashes: async () => [],
      listRemotes: async () => [origin],
      getCurrentUpstream: async () => behind,
      getHeadInfo: async (path) => ({ ...(await EMPTY_RESULTS.getHeadInfo!(path)), headId: "abcdef1234567890abcdef1234567890abcdef12" }),
      ...overrides,
    },
    "empty",
  );
}

const notifications = () => within(screen.getByRole("status", { name: "Notifications" }));

async function showTab(name: "a" | "b") {
  const tab = screen.getByRole("tab", { name: new RegExp(`^${name}(,|$)`) });
  fireEvent.click(tab);
  await waitFor(() => expect(tab).toHaveAttribute("aria-selected", "true"));
}

async function fetchInA(fetchRemote: ReturnType<typeof vi.fn<RepoClient["fetchRemote"]>>) {
  const sync = await screen.findByRole("group", { name: "Sync" });
  const fetch = within(sync).getByRole("button", { name: "Fetch" });
  await waitFor(() => expect(fetch).toBeEnabled());
  fireEvent.click(fetch);
  await waitFor(() => expect(fetchRemote).toHaveBeenCalled());
  expect(fetchRemote.mock.calls[0][0]).toBe("/repos/a");
}

describe("toasts of a tab that isn't shown (#1116)", () => {
  it("names the tab, and its Pull shows that tab and pulls there", async () => {
    const { subscribeTransferProgress, emit } = transferEvents();
    const fetchRemote = vi.fn(async () => "fetch-1");
    const pullCurrentUpstream = vi.fn<RepoClient["pullCurrentUpstream"]>(async () => ({ kind: "UpToDate" }));
    render(<App client={twoTabs({ fetchRemote, pullCurrentUpstream, subscribeTransferProgress })} />);
    await fetchInA(fetchRemote);
    await showTab("b");
    emit("fetch-1", "Fetch", "Completed");

    expect(await notifications().findByText("a: Fetched from origin. main is 2 commits behind origin/main.")).toBeInTheDocument();
    fireEvent.click(notifications().getByRole("button", { name: "Pull" }));
    await waitFor(() => expect(screen.getByRole("tab", { name: /^a(,|$)/ })).toHaveAttribute("aria-selected", "true"));
    await waitFor(() => expect(pullCurrentUpstream).toHaveBeenCalled());
    expect(pullCurrentUpstream.mock.calls.every(([path]) => path === "/repos/a")).toBe(true);
  });

  it("names the tab once another one is shown, and drops the name when it is shown again", async () => {
    const { subscribeTransferProgress, emit } = transferEvents();
    const fetchRemote = vi.fn(async () => "fetch-1");
    render(<App client={twoTabs({ fetchRemote, subscribeTransferProgress })} />);
    await fetchInA(fetchRemote);
    emit("fetch-1", "Fetch", "Completed");
    const message = "Fetched from origin. main is 2 commits behind origin/main.";
    expect(await notifications().findByText(message)).toBeInTheDocument();

    await showTab("b");
    expect(notifications().getByText(`a: ${message}`)).toBeInTheDocument();
    await showTab("a");
    expect(notifications().getByText(message)).toBeInTheDocument();
  });

  it("runs its Undo in its tab once that tab is shown", async () => {
    const undo = vi.fn<RepoClient["undo"]>(async () => ({ kind: "Done", label: "Commit" }));
    render(
      <App
        client={twoTabs({
          commit: async () => ({ kind: "Committed" as const, commitId: "c0ffee1234567890c0ffee1234567890c0ffee12" }),
          undo,
        })}
      />,
    );
    const box = await screen.findByRole("textbox", { name: "Commit message" });
    fireEvent.change(box, { target: { value: "feat: x" } });
    const button = await screen.findByRole("button", { name: /^Commit 1 file/ });
    await waitFor(() => expect(button).toBeEnabled());
    fireEvent.click(button);
    await notifications().findByText(/^Committed c0ffee1/);

    await showTab("b");
    fireEvent.click(notifications().getByRole("button", { name: "Undo" }));
    await waitFor(() => expect(screen.getByRole("tab", { name: /^a(,|$)/ })).toHaveAttribute("aria-selected", "true"));
    await waitFor(() => expect(undo).toHaveBeenCalledWith("/repos/a"));
  });

  it("drops a closed tab's toasts that have buttons", async () => {
    const { subscribeTransferProgress, emit } = transferEvents();
    const fetchRemote = vi.fn(async () => "fetch-1");
    render(<App client={twoTabs({ fetchRemote, subscribeTransferProgress, closeRepo: async () => {} })} />);
    await fetchInA(fetchRemote);
    await showTab("b");
    emit("fetch-1", "Fetch", "Completed");
    expect(await notifications().findByRole("button", { name: "Pull" })).toBeInTheDocument();

    const tabA = screen.getByRole("tab", { name: /^a(,|$)/ });
    fireEvent.click(tabA.parentElement!.querySelector<HTMLButtonElement>("button:not([role='tab'])")!);
    await waitFor(() => expect(screen.queryByRole("tab", { name: /^a(,|$)/ })).toBeNull());
    await waitFor(() => expect(notifications().queryByRole("button", { name: "Pull" })).toBeNull());
  });

  it("announces a transfer that failed in a tab that isn't shown", async () => {
    const { subscribeTransferProgress, emit } = transferEvents();
    const pushCurrentBranch = vi.fn(async () => "push-1");
    render(<App client={twoTabs({ pushCurrentBranch, subscribeTransferProgress })} />);
    const sync = await screen.findByRole("group", { name: "Sync" });
    const push = within(sync).getByRole("button", { name: "Push" });
    await waitFor(() => expect(push).toBeEnabled());
    fireEvent.click(push);
    await waitFor(() => expect(pushCurrentBranch).toHaveBeenCalledWith("/repos/a", "origin", false));
    await showTab("b");
    emit("push-1", "PushBranch", "Failed");

    expect(await notifications().findByText(/^a: Couldn't push main/)).toBeInTheDocument();
  });

  it("doesn't announce another action's error when a hidden tab's transfer succeeds", async () => {
    const { subscribeTransferProgress, emit } = transferEvents();
    const fetchRemote = vi.fn(async () => "fetch-1");
    const stageFile = vi.fn(async () => {
      throw new Error("index.lock exists");
    });
    render(
      <App
        client={twoTabs({
          fetchRemote,
          subscribeTransferProgress,
          stageFile,
          getStatus: async () => [{ path: "x.txt", oldPath: null, staged: false, kind: "Modified" }],
        })}
      />,
    );
    await fetchInA(fetchRemote);
    fireEvent.click(await screen.findByRole("button", { name: "Stage x.txt" }));
    await waitFor(() => expect(stageFile).toHaveBeenCalled());
    await screen.findByText(/Couldn't stage x\.txt/);
    await showTab("b");
    emit("fetch-1", "Fetch", "Completed");

    expect(await notifications().findByText("a: Fetched from origin. main is 2 commits behind origin/main.")).toBeInTheDocument();
    expect(notifications().queryByText(/Couldn't stage/)).toBeNull();
  });

  it("shows a user command's output only in its own tab, opening it from a hidden tab's toast", async () => {
    const command: UserCommand = {
      id: "lint",
      name: "Lint",
      commandLine: "lint",
      workingDirectory: null,
      targets: ["Repository"],
      confirm: false,
      showOutput: false,
      refresh: false,
      variables: [],
      problem: null,
    };
    const runUserCommand = vi.fn(async () => ({ exitCode: 1, success: false, timedOut: false, cancelled: false, output: "lint failed\n" }));
    render(<App client={twoTabs({ listUserCommands: async () => ({ global: [command], repository: [] }), runUserCommand })} />);
    await screen.findByRole("group", { name: "Sync" });
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    fireEvent.change(await screen.findByRole("combobox", { name: "Command palette" }), { target: { value: "Run: Lint" } });
    fireEvent.click(await screen.findByRole("option", { name: "Run: Lint" }));
    await notifications().findByText("Lint failed with exit code 1");

    await showTab("b");
    fireEvent.click(notifications().getByRole("button", { name: "Show output" }));
    await waitFor(() => expect(screen.getByRole("tab", { name: /^a(,|$)/ })).toHaveAttribute("aria-selected", "true"));
    expect(await screen.findByRole("dialog", { name: "Lint", hidden: true })).toBeInTheDocument();

    // A dialog of the tab that isn't shown is not open behind the shown one.
    await showTab("b");
    expect(screen.queryByRole("dialog", { name: "Lint", hidden: true })).toBeNull();
  });
});

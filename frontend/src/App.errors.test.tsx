import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import App from "./App";
import { RepoError, type HeadInfo, type RepoClient, type TransferProgress } from "./ipc/RepoClient";
import { EMPTY_RESULTS, fakeRepoClient, graphPage } from "./test/fakeRepoClient";

// Errors, banners and toasts (#973, #984, #996, #998, #1000). Whole-App renders, like
// App.test.tsx (#217's slow-machine budget).
vi.setConfig({ testTimeout: 20_000 });

const origin = { name: "origin", fetchUrl: "https://example.com/r.git", pushUrl: null, authMode: null, authUsername: null };
const upstream = { localBranch: "main", remoteName: "origin", remoteBranch: "main", ahead: 1, behind: 1, pushBranch: "main" };

async function headInfo(): Promise<HeadInfo> {
  return { ...(await EMPTY_RESULTS.getHeadInfo!("/repos/a")), headId: "abcdef1234567890abcdef1234567890abcdef12" };
}

function repoClient(overrides: Partial<RepoClient> = {}): RepoClient {
  return fakeRepoClient(
    {
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [{ path: "a.txt", oldPath: null, staged: true, kind: "Modified" }],
      getCommitGraph: async () => graphPage([]),
      listBranches: async () => [{ name: "main", isCurrent: true, upstream: "origin/main", ahead: 1, behind: 1 }],
      listStashes: async () => [],
      listRemotes: async () => [origin],
      getCurrentUpstream: async () => upstream,
      getHeadInfo: headInfo,
      ...overrides,
    },
    "empty",
  );
}

/** A start page with no tab open, whose "Open repository…" picks `picked`. */
function startPageClient(picked: string, overrides: Partial<RepoClient> = {}): RepoClient {
  return fakeRepoClient(
    {
      listOpenRepos: async () => ({ entries: [], activePath: null }),
      persistOpenRepos: async () => {},
      listRecentRepos: async () => [],
      listPinnedRepos: async () => [],
      listMissingRecentRepos: async () => [],
      pickRepoFolder: async () => picked,
      ...overrides,
    },
    "empty",
  );
}

/** `subscribeTransferProgress` for every subscriber, and `emit` to send them all an event. */
function transferEvents() {
  const listeners = new Set<(progress: TransferProgress) => void>();
  const subscribeTransferProgress: RepoClient["subscribeTransferProgress"] = (callback) => {
    listeners.add(callback);
    return () => listeners.delete(callback);
  };
  const emit = (progress: TransferProgress) => act(() => listeners.forEach((listener) => listener(progress)));
  return { subscribeTransferProgress, emit };
}

/** The error banners outside any dialog. */
function banners(): HTMLElement[] {
  return screen.queryAllByRole("alert", { hidden: true }).filter((element) => element.closest('[role="dialog"]') === null);
}

const buttonNames = (element: HTMLElement) =>
  within(element)
    .getAllByRole("button")
    .map((button) => button.getAttribute("aria-label") ?? button.textContent);

async function commitFromTheBox() {
  const box = await screen.findByRole("textbox", { name: "Commit message" });
  fireEvent.change(box, { target: { value: "feat: x" } });
  const button = await screen.findByRole("button", { name: /^Commit 1 file/ });
  await waitFor(() => expect(button).toBeEnabled());
  fireEvent.click(button);
}

describe("the error banner (#984)", () => {
  async function rejectPush(overrides: Partial<RepoClient> = {}) {
    const { subscribeTransferProgress, emit } = transferEvents();
    const pushCurrentBranch = vi.fn(async () => "push-1");
    render(
      <App
        client={repoClient({
          pushCurrentBranch,
          subscribeTransferProgress,
          ...overrides,
        })}
      />,
    );
    const sync = await screen.findByRole("group", { name: "Sync" });
    const push = within(sync).getByRole("button", { name: "Push" });
    await waitFor(() => expect(push).toBeEnabled());
    fireEvent.click(push);
    await waitFor(() => expect(pushCurrentBranch).toHaveBeenCalledWith("/repos/a", "origin", false));
    emit({
      operationId: "push-1",
      operation: "PushBranch",
      phase: "Failed",
      errorKind: "NonFastForward",
      current: 0,
      total: 0,
      receivedBytes: 0,
      message: null,
      hookOutput: null,
    });
    return await waitFor(() => {
      const [banner] = banners();
      expect(banner).toBeDefined();
      return banner!;
    });
  }

  it("offers Pull first, then Force push…, Details and dismiss, all in the banner's row", async () => {
    const banner = await rejectPush();
    expect(banner).toHaveTextContent("Push was rejected because the remote has newer commits.");
    expect(buttonNames(banner)).toEqual(["Pull", "Force push…", "Details", "Dismiss error"]);
  });

  it("pulls from the banner's Pull", async () => {
    const pullCurrentUpstream = vi.fn(async () => ({ kind: "UpToDate" as const }));
    const banner = await rejectPush({ pullCurrentUpstream });
    fireEvent.click(within(banner).getByRole("button", { name: "Pull" }));
    await waitFor(() => expect(pullCurrentUpstream).toHaveBeenCalledWith("/repos/a"));
  });

  it("opens the force push dialog from Force push…", async () => {
    const banner = await rejectPush();
    fireEvent.click(within(banner).getByRole("button", { name: "Force push…" }));
    expect(await screen.findByRole("dialog", { name: "Confirm force push" })).toBeInTheDocument();
  });
});

describe("no banner while a dialog handles the error (#996)", () => {
  it("shows no banner while the identity dialog is open, nor after Cancel", async () => {
    const commit = vi.fn(async () => {
      throw new RepoError("MissingIdentity", "Set your name and email for git before committing.");
    });
    render(<App client={repoClient({ commit })} />);
    await commitFromTheBox();

    const dialog = await screen.findByRole("dialog", { name: "Set your git identity" });
    expect(banners()).toEqual([]);
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Set your git identity" })).toBeNull());
    expect(banners()).toEqual([]);
  });

  it("shows no banner while the hook failure dialog is open; after Close the banner offers its output", async () => {
    const commit = vi.fn(async () => ({ kind: "HookFailed" as const, hook: "pre-commit", output: "lint failed\n" }));
    render(<App client={repoClient({ commit })} />);
    await commitFromTheBox();

    const dialog = await screen.findByRole("dialog", { name: "pre-commit hook failed" });
    expect(banners()).toEqual([]);
    fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(banners()).toHaveLength(1));
    expect(within(banners()[0]!).getByRole("button", { name: "Show hook output…" })).toBeInTheDocument();
  });
});

describe("opening a folder that isn't a repository (#973)", () => {
  const notARepository = new RepoError(
    "Other",
    "failed to open repository: could not find repository at '/work'; class=Repository (6); code=NotFound (-3)",
  );

  it("says so and lists the repositories in its direct subfolders, each with Open", async () => {
    const openRepo = vi.fn(async (path: string) => {
      if (path === "/work") throw notARepository;
    });
    const scanReposInRoot = vi.fn(async () => ["/work/.config", "/work/api", "/work/node_modules", "/work/web"]);
    render(<App client={startPageClient("/work", { openRepo, scanReposInRoot })} />);
    fireEvent.click(await screen.findByRole("button", { name: "Open repository…" }));

    const banner = await waitFor(() => {
      const [found] = banners();
      expect(found).toHaveTextContent("This folder isn't a Git repository: /work");
      return found!;
    });
    expect(scanReposInRoot).toHaveBeenCalledWith("/work");
    const list = within(banner).getByRole("list", { name: "Repositories in /work" });
    expect(within(list).getAllByRole("listitem").map((item) => item.textContent)).toEqual([
      expect.stringContaining("api"),
      expect.stringContaining("web"),
    ]);
    expect(within(banner).queryByRole("button", { name: "Retry" })).toBeNull();
    expect(banner).not.toHaveTextContent("class=");

    fireEvent.click(within(list).getByRole("button", { name: "Open api" }));
    await waitFor(() => expect(openRepo).toHaveBeenCalledWith("/work/api"));
  });

  it("says so without a list when no subfolder is a repository", async () => {
    const openRepo = vi.fn(async () => {
      throw notARepository;
    });
    render(<App client={startPageClient("/work", { openRepo, scanReposInRoot: async () => [] })} />);
    fireEvent.click(await screen.findByRole("button", { name: "Open repository…" }));

    const banner = await waitFor(() => {
      const [found] = banners();
      expect(found).toHaveTextContent("This folder isn't a Git repository: /work");
      return found!;
    });
    expect(within(banner).queryByRole("list")).toBeNull();
    expect(within(banner).queryByRole("button", { name: "Retry" })).toBeNull();
  });
});

describe("opening a recent repository whose folder is gone (#998)", () => {
  it("offers to remove it from the recent repositories, and no Retry", async () => {
    const openRepo = vi.fn(async () => {
      throw new RepoError(
        "Other",
        "failed to open repository: failed to resolve path '/gone': No such file or directory; class=Os (2); code=NotFound (-3)",
      );
    });
    const removeRecentRepo = vi.fn(async () => {});
    render(
      <App
        client={startPageClient("/gone", {
          openRepo,
          removeRecentRepo,
          listMissingRecentRepos: async () => ["/gone"],
          scanReposInRoot: async () => {
            throw new Error("No such file or directory");
          },
        })}
      />,
    );
    fireEvent.click(await screen.findByRole("button", { name: "Open repository…" }));

    const banner = await waitFor(() => {
      const [found] = banners();
      expect(found).toHaveTextContent("The folder /gone no longer exists. Remove it from recent repositories?");
      return found!;
    });
    expect(buttonNames(banner)).toEqual(["Remove", "Dismiss error"]);
    fireEvent.click(within(banner).getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(removeRecentRepo).toHaveBeenCalledWith("/gone"));
    await waitFor(() => expect(banners()).toEqual([]));
  });
});

describe("a fetch that leaves the branch only ahead (#999)", () => {
  it("says so and offers Push", async () => {
    const { subscribeTransferProgress, emit } = transferEvents();
    const fetchRemote = vi.fn(async () => "fetch-1");
    const pushCurrentBranch = vi.fn(async () => "push-1");
    render(
      <App
        client={repoClient({
          listBranches: async () => [{ name: "main", isCurrent: true, upstream: "origin/main", ahead: 2, behind: 0 }],
          getCurrentUpstream: async () => ({ ...upstream, ahead: 2, behind: 0 }),
          fetchRemote,
          pushCurrentBranch,
          subscribeTransferProgress,
        })}
      />,
    );
    const sync = await screen.findByRole("group", { name: "Sync" });
    const fetch = within(sync).getByRole("button", { name: "Fetch" });
    await waitFor(() => expect(fetch).toBeEnabled());
    fireEvent.click(fetch);
    await waitFor(() => expect(fetchRemote).toHaveBeenCalled());
    emit({
      operationId: "fetch-1",
      operation: "Fetch",
      phase: "Completed",
      errorKind: null,
      current: 0,
      total: 0,
      receivedBytes: 0,
      message: null,
      hookOutput: null,
    });

    const region = screen.getByRole("status", { name: "Notifications" });
    expect(await within(region).findByText("Fetched from origin. main is 2 commits ahead of origin/main.")).toBeInTheDocument();
    fireEvent.click(within(region).getByRole("button", { name: "Push" }));
    await waitFor(() => expect(pushCurrentBranch).toHaveBeenCalledWith("/repos/a", "origin", false));
  });
});

describe("one notification region (#1000)", () => {
  it("shows the update notice in the toast region, with the toasts", async () => {
    render(<App client={repoClient()} updateBanner={<div>Update v9.9.9 ready</div>} />);
    const region = await screen.findByRole("status", { name: "Notifications" });
    expect(within(region).getByText("Update v9.9.9 ready")).toBeInTheDocument();
  });
});

import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import type { RepoClient } from "./ipc/RepoClient";
import { fakeRepoClient, graphPage } from "./test/fakeRepoClient";

// Whole-App renders, like App.test.tsx (#217's slow-machine budget).
vi.setConfig({ testTimeout: 20_000 });

const origin = { name: "origin", fetchUrl: "https://example.com/r.git", pushUrl: null, authMode: null, authUsername: null };
const upstream = { localBranch: "main", remoteName: "origin", remoteBranch: "main", ahead: 1, behind: 2, pushBranch: "main" };
const change = { path: "a.txt", oldPath: null, staged: false, kind: "Modified" as const };

function shortcutClient(overrides: Partial<RepoClient> = {}): RepoClient {
  return fakeRepoClient(
    {
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [change],
      getCommitGraph: async () => graphPage([]),
      listStashes: async () => [],
      listBranches: async () => [{ name: "main", isCurrent: true, upstream: "origin/main", ahead: 1, behind: 2 }],
      listRemotes: async () => [origin],
      getCurrentUpstream: async () => upstream,
      ...overrides,
    },
    "empty",
  );
}

function setWindowWidth(width: number) {
  Object.defineProperty(window, "innerWidth", { writable: true, configurable: true, value: width });
  act(() => {
    fireEvent(window, new Event("resize"));
  });
}

const pushButton = () => within(screen.getByRole("group", { name: "Repository" })).getByRole("button", { name: "Push" });

async function renderApp(client: RepoClient, { remote = true } = {}) {
  const view = render(<App client={client} />);
  await screen.findByRole("tab", { name: "a" });
  await waitFor(() => expect(view.container.querySelector('[data-active-repo="true"]')).not.toBeNull());
  // The toolbar's Push enables once the upstream is read: the commands are then built.
  if (remote) await waitFor(() => expect(pushButton()).not.toHaveAttribute("aria-disabled"));
  else await screen.findByRole("listbox", { name: "Commit history" });
  return view;
}

const notifications = () => within(screen.getByRole("status", { name: "Notifications" }));

let originalWidth: number;
beforeEach(() => {
  originalWidth = window.innerWidth;
  setWindowWidth(1200);
});
afterEach(() => {
  setWindowWidth(originalWidth);
  localStorage.clear();
});

describe("global shortcuts for the core loop (#646)", () => {
  it("refreshes on F5 and fetches on Shift+F5", async () => {
    const getStatus = vi.fn(async () => [change]);
    const fetchRemote = vi.fn(async () => "fetch-1");
    await renderApp(shortcutClient({ getStatus, fetchRemote }));

    const calls = getStatus.mock.calls.length;
    fireEvent.keyDown(window, { key: "F5" });
    await waitFor(() => expect(getStatus.mock.calls.length).toBeGreaterThan(calls));

    fireEvent.keyDown(window, { key: "F5", shiftKey: true });
    await waitFor(() => expect(fetchRemote).toHaveBeenCalledWith("/repos/a", "origin", null));
  });

  it("pulls on Ctrl+Shift+L and pushes on Ctrl+Shift+K", async () => {
    const pushCurrentBranch = vi.fn(async () => "push-1");
    const pullCurrentUpstream = vi.fn(async () => ({ kind: "UpToDate" as const }));
    await renderApp(shortcutClient({ pushCurrentBranch, pullCurrentUpstream }));

    fireEvent.keyDown(window, { key: "L", code: "KeyL", ctrlKey: true, shiftKey: true });
    await waitFor(() => expect(pullCurrentUpstream).toHaveBeenCalledWith("/repos/a"));
    await waitFor(() => expect(pushButton()).not.toHaveAttribute("aria-disabled"));

    fireEvent.keyDown(window, { key: "K", code: "KeyK", ctrlKey: true, shiftKey: true });
    await waitFor(() => expect(pushCurrentBranch).toHaveBeenCalledWith("/repos/a", "origin", false));
    // Ctrl+Shift+K is not Ctrl+K: no palette.
    expect(screen.queryByRole("combobox", { name: "Command palette" })).toBeNull();
  });

  it("stages all changes on Ctrl+Shift+A, but not from a text field", async () => {
    const stagePaths = vi.fn(async () => {});
    await renderApp(shortcutClient({ stagePaths }));
    await screen.findByRole("listbox", { name: "Unstaged changes" });

    const search = screen.getByRole("combobox", { name: /Search commits/ });
    fireEvent.keyDown(search, { key: "A", code: "KeyA", ctrlKey: true, shiftKey: true });
    expect(stagePaths).not.toHaveBeenCalled();

    fireEvent.keyDown(window, { key: "A", code: "KeyA", ctrlKey: true, shiftKey: true });
    await waitFor(() => expect(stagePaths).toHaveBeenCalledWith("/repos/a", ["a.txt"]));
  });

  // #642: the last few tabs stay mounted, hidden; their global listeners must stay quiet.
  it("runs a shortcut only in the shown tab while another tab's workspace is mounted in the background", async () => {
    const stagePaths = vi.fn(async () => {});
    const getStatus = vi.fn(async (_path: string) => [change]);
    const view = await renderApp(
      shortcutClient({
        stagePaths,
        getStatus,
        listOpenRepos: async () => ({
          entries: [
            { path: "/repos/a", workspaceId: null },
            { path: "/repos/b", workspaceId: null },
          ],
          activePath: "/repos/a",
        }),
      }),
    );
    fireEvent.click(screen.getByRole("tab", { name: "b" }));
    await waitFor(() => expect(view.container.querySelectorAll("[data-active-repo]")).toHaveLength(2));
    await waitFor(() => expect(getStatus).toHaveBeenCalledWith("/repos/b"));
    await screen.findByRole("listbox", { name: "Unstaged changes" });

    fireEvent.keyDown(window, { key: "A", code: "KeyA", ctrlKey: true, shiftKey: true });
    await waitFor(() => expect(stagePaths).toHaveBeenCalledWith("/repos/b", ["a.txt"]));
    expect(stagePaths).not.toHaveBeenCalledWith("/repos/a", expect.anything());

    const statusReadsOf = (path: string) => getStatus.mock.calls.filter(([repo]) => repo === path).length;
    const readsOfA = statusReadsOf("/repos/a");
    const readsOfB = statusReadsOf("/repos/b");
    fireEvent.keyDown(window, { key: "F5" });
    await waitFor(() => expect(statusReadsOf("/repos/b")).toBeGreaterThan(readsOfB));
    expect(statusReadsOf("/repos/a")).toBe(readsOfA);
  });

  it("goes to the commit message on Alt+C, also when Option typed another character", async () => {
    await renderApp(shortcutClient());
    const message = await screen.findByRole("textbox", { name: /Commit message/ });
    fireEvent.keyDown(window, { key: "ç", code: "KeyC", altKey: true });
    await waitFor(() => expect(message).toHaveFocus());
  });

  it("says why a shortcut can't run", async () => {
    const fetchRemote = vi.fn(async () => "fetch-1");
    await renderApp(
      shortcutClient({ fetchRemote, listRemotes: async () => [], getCurrentUpstream: async () => null, getStatus: async () => [] }),
      { remote: false },
    );
    fireEvent.keyDown(window, { key: "F5", shiftKey: true });
    expect(await notifications().findByText("Can't fetch now. No remote configured.")).toBeInTheDocument();
    fireEvent.keyDown(window, { key: "A", code: "KeyA", ctrlKey: true, shiftKey: true });
    expect(await notifications().findByText("Can't stage now. No changes to stage.")).toBeInTheDocument();
    expect(fetchRemote).not.toHaveBeenCalled();
  });

  it("shows the shortcuts beside their commands in the palette and in the toolbar's tooltips", async () => {
    await renderApp(shortcutClient());
    const bar = screen.getByRole("group", { name: "Repository" });
    expect(within(bar).getByRole("button", { name: "Refresh" })).toHaveAttribute("title", "Refresh (F5)");
    expect(within(bar).getByRole("button", { name: "Fetch" })).toHaveAttribute("title", "Fetch from origin (Shift+F5)");

    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    fireEvent.change(await screen.findByRole("combobox", { name: "Command palette" }), { target: { value: "Refresh" } });
    const option = await screen.findByRole("option", { name: "Refresh" });
    expect(option.querySelector("kbd")).toHaveTextContent("F5");
  });
});

describe("switching tabs by keyboard (#646)", () => {
  const tabsClient = () =>
    shortcutClient({
      listOpenRepos: async () => ({
        entries: [
          { path: "/repos/a", workspaceId: null },
          { path: "/repos/b", workspaceId: null },
          { path: "/repos/c", workspaceId: null },
        ],
        activePath: "/repos/a",
      }),
    });
  const selectedTab = () => screen.getByRole("tab", { selected: true });
  const whenActive = async (name: string) => {
    await waitFor(() => expect(selectedTab()).toHaveAccessibleName(name));
    await waitFor(() =>
      expect(document.querySelector('[data-active-repo="true"]')?.getAttribute("aria-labelledby")).toBe(selectedTab().id),
    );
  };

  it("moves with Ctrl+Tab and Ctrl+Shift+Tab, wrapping, and jumps with Ctrl+1…9", async () => {
    await renderApp(tabsClient());
    await whenActive("a");

    fireEvent.keyDown(window, { key: "Tab", ctrlKey: true });
    await whenActive("b");
    fireEvent.keyDown(window, { key: "Tab", ctrlKey: true, shiftKey: true });
    await whenActive("a");
    fireEvent.keyDown(window, { key: "Tab", ctrlKey: true, shiftKey: true });
    await whenActive("c");

    fireEvent.keyDown(window, { key: "1", code: "Digit1", ctrlKey: true });
    await whenActive("a");
    fireEvent.keyDown(window, { key: "2", code: "Digit2", ctrlKey: true });
    await whenActive("b");
    // Ctrl+9 is the last tab.
    fireEvent.keyDown(window, { key: "9", code: "Digit9", ctrlKey: true });
    await whenActive("c");
  });
});

describe("moving between panes with F6 (#670)", () => {
  it("names the panes as regions and cycles through them, back to where the focus was", async () => {
    const view = await renderApp(shortcutClient());
    const workspace = view.container.querySelector<HTMLElement>('[data-active-repo="true"]')!;
    expect(within(workspace).getByRole("region", { name: "Sidebar" })).toBeInTheDocument();
    const history = within(workspace).getByRole("region", { name: "History" });
    const diff = within(workspace).getByRole("region", { name: "Diff" });
    const toolbar = screen.getByRole("group", { name: "Repository" });
    const sidebar = within(workspace).getByRole("region", { name: "Sidebar" });

    fireEvent.keyDown(window, { key: "F6" });
    expect(toolbar).toContainElement(document.activeElement as HTMLElement);
    fireEvent.keyDown(document.activeElement!, { key: "F6" });
    expect(sidebar).toContainElement(document.activeElement as HTMLElement);
    fireEvent.keyDown(document.activeElement!, { key: "F6" });
    // The history's list, not the search field before it.
    expect(document.activeElement).toBe(within(history).getByRole("listbox", { name: "Commit history" }));
    fireEvent.keyDown(document.activeElement!, { key: "F6" });
    expect(diff).toContainElement(document.activeElement as HTMLElement);

    // Back into the diff after a detour, to the element focused there last.
    const message = within(diff).getByRole("textbox", { name: /Commit message/ });
    act(() => message.focus());
    fireEvent.keyDown(message, { key: "F6", shiftKey: true });
    expect(document.activeElement).toBe(within(history).getByRole("listbox", { name: "Commit history" }));
    fireEvent.keyDown(document.activeElement!, { key: "F6" });
    expect(document.activeElement).toBe(message);
    // From the last pane, F6 wraps around to the toolbar.
    fireEvent.keyDown(message, { key: "F6" });
    expect(toolbar).toContainElement(document.activeElement as HTMLElement);
  });
});

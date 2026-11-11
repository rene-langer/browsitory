import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { RepoError, type FileDropEvent, type RepoClient } from "./ipc/RepoClient";
import { fakeRepoClient, graphPage } from "./test/fakeRepoClient";

// Whole-App renders, like App.test.tsx (#217's slow-machine budget).
vi.setConfig({ testTimeout: 20_000 });

const origin = { name: "origin", fetchUrl: "https://example.com/r.git", pushUrl: null, authMode: null, authUsername: null };
const upstream = { localBranch: "main", remoteName: "origin", remoteBranch: "main", ahead: 1, behind: 2, pushBranch: "main" };

function layoutClient(overrides: Partial<RepoClient> = {}): RepoClient {
  return fakeRepoClient(
    {
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => graphPage([]),
      listStashes: async () => [],
      listBranches: async () => [
        { name: "main", isCurrent: true, upstream: "origin/main", ahead: 1, behind: 2 },
        { name: "feature", isCurrent: false },
        { name: "fix/login", isCurrent: false },
      ],
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

async function renderApp(client: RepoClient) {
  const view = render(<App client={client} />);
  await screen.findByRole("tab", { name: "a" });
  await waitFor(() => expect(view.container.querySelector('[data-active-repo="true"]')).not.toBeNull());
  return view;
}

const toolbar = () => screen.getByRole("group", { name: "Repository" });
const sidebarPane = () =>
  screen.getByRole("separator", { name: "Sidebar width", hidden: true }).previousElementSibling as HTMLElement;

let originalWidth: number;
beforeEach(() => {
  originalWidth = window.innerWidth;
  setWindowWidth(1200);
});
afterEach(() => {
  setWindowWidth(originalWidth);
  localStorage.clear();
});

describe("the diverged-pull prompt (#636)", () => {
  const divergedClient = () => layoutClient({ pullCurrentUpstream: async () => ({ kind: "Diverged", upstreamRef: "origin/main" }) });

  it("appears from the toolbar's Pull while the Branches section is closed, and Pull waits for it", async () => {
    localStorage.setItem("sidebar-branches", "closed");
    await renderApp(divergedClient());
    const pull = within(toolbar()).getByRole("button", { name: "Pull" });
    await waitFor(() => expect(pull).not.toHaveAttribute("aria-disabled"));
    fireEvent.click(pull);
    const dialog = await screen.findByRole("dialog", { name: "Pull has diverged" });
    expect(dialog).toHaveTextContent("origin/main");
    expect(pull).toHaveAttribute("aria-disabled", "true");
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Pull has diverged" })).toBeNull());
    expect(pull).not.toHaveAttribute("aria-disabled");
  });

  it("is not inside the hidden sidebar in a narrow window", async () => {
    setWindowWidth(800);
    await renderApp(divergedClient());
    await waitFor(() => expect(sidebarPane()).toHaveAttribute("hidden"));
    const pull = within(toolbar()).getByRole("button", { name: "Pull" });
    await waitFor(() => expect(pull).not.toHaveAttribute("aria-disabled"));
    fireEvent.click(pull);
    const dialog = await screen.findByRole("dialog", { name: "Pull has diverged" });
    // `showModal()` inside a hidden ancestor is an invisible modal that makes the page inert.
    expect(dialog.closest("[hidden]")).toBeNull();
  });
});

describe("a pull git config wants as a merge commit (#740)", () => {
  it("merges the upstream with a merge commit at once, without the diverged dialog", async () => {
    const mergeBranch = vi.fn(async () => ({ kind: "Merged" as const }));
    await renderApp(
      layoutClient({
        pullCurrentUpstream: async () => ({ kind: "NoFastForward", upstreamRef: "refs/remotes/origin/main" }),
        getPullStrategy: async () => null,
        mergeBranch,
      }),
    );
    const pull = within(toolbar()).getByRole("button", { name: "Pull" });
    await waitFor(() => expect(pull).not.toHaveAttribute("aria-disabled"));
    fireEvent.click(pull);

    await waitFor(() =>
      expect(mergeBranch).toHaveBeenCalledWith("/repos/a", "refs/remotes/origin/main", { fastForward: "Never", squash: false }),
    );
    expect(mergeBranch).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("dialog", { name: "Pull has diverged" })).toBeNull();
  });
});

describe("the sidebar in a narrow window (#635)", () => {
  it("opens as a drawer from the toolbar and closes with Escape", async () => {
    setWindowWidth(800);
    await renderApp(layoutClient());
    await waitFor(() => expect(sidebarPane()).toHaveAttribute("hidden"));
    const toggle = within(toolbar()).getByRole("button", { name: "Sidebar" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(toggle);
    expect(sidebarPane()).not.toHaveAttribute("hidden");
    expect(sidebarPane()).toHaveAttribute("data-overlay", "true");
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(within(sidebarPane()).getByText("Branches")).toBeInTheDocument();
    fireEvent.keyDown(within(sidebarPane()).getByText("Branches"), { key: "Escape" });
    expect(sidebarPane()).toHaveAttribute("hidden");
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(toggle).toHaveFocus();
  });

  it("opens the drawer from the divider and the shortcut, and reports the width it shows", async () => {
    setWindowWidth(800);
    await renderApp(layoutClient());
    await waitFor(() => expect(sidebarPane()).toHaveAttribute("hidden"));
    const divider = screen.getByRole("separator", { name: "Sidebar width" });
    expect(divider).toHaveAttribute("aria-valuenow", "0");
    fireEvent.doubleClick(divider);
    expect(sidebarPane()).toHaveAttribute("data-overlay", "true");
    fireEvent.keyDown(window, { key: "b", code: "KeyB", ctrlKey: true, altKey: true });
    expect(sidebarPane()).toHaveAttribute("hidden");
    fireEvent.keyDown(window, { key: "b", code: "KeyB", ctrlKey: true, altKey: true });
    expect(sidebarPane()).not.toHaveAttribute("hidden");
  });

  it("collapses and expands the docked sidebar from the toolbar in a wide window", async () => {
    await renderApp(layoutClient());
    const toggle = within(toolbar()).getByRole("button", { name: "Sidebar" });
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(sidebarPane()).not.toHaveAttribute("hidden");
    fireEvent.click(toggle);
    expect(sidebarPane()).toHaveAttribute("hidden");
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(toggle);
    expect(sidebarPane()).not.toHaveAttribute("hidden");
    expect(sidebarPane()).not.toHaveAttribute("data-overlay");
  });
});

describe("the repository toolbar (#637)", () => {
  it("holds sync, Stash, Undo/Redo and Refresh; the sidebar has no second Pull", async () => {
    const getStatus = vi.fn(async () => [{ path: "a.txt", oldPath: null, staged: false, kind: "Modified" as const }]);
    const saveStash = vi.fn(async () => {});
    await renderApp(layoutClient({ getStatus, saveStash }));
    const bar = toolbar();
    for (const name of ["Fetch", "Pull", "Push", "Refresh"]) expect(within(bar).getByRole("button", { name })).toBeInTheDocument();
    expect(within(bar).getByRole("button", { name: "Nothing to undo" })).toBeInTheDocument();
    const stash = within(bar).getByRole("button", { name: "Stash" });
    await waitFor(() => expect(stash).not.toHaveAttribute("aria-disabled"));
    expect(within(sidebarPane()).queryByRole("button", { name: "Pull" })).toBeNull();
    const dock = await screen.findByRole("group", { name: "Commit actions" });
    expect(within(dock).queryByRole("button", { name: "Stash" })).toBeNull();

    const calls = getStatus.mock.calls.length;
    fireEvent.click(within(bar).getByRole("button", { name: "Refresh" }));
    await waitFor(() => expect(getStatus.mock.calls.length).toBeGreaterThan(calls));
    fireEvent.click(stash);
    await waitFor(() => expect(saveStash).toHaveBeenCalled());
  });
});

describe("Apply patch… in the toolbar (#649)", () => {
  it("asks for a patch file, previews it and applies it to the working tree (#658)", async () => {
    const content = { base64: "ZGlmZg==" };
    const openPatchFile = vi.fn(async () => content);
    const preview = {
      parts: [{ mail: null, files: [{ path: "a.txt", oldPath: null, status: "Modified" as const, additions: 1, deletions: 1, binary: false }] }],
      problem: null,
    };
    const previewPatch = vi.fn(async () => preview);
    const applyPatch = vi.fn(async () => ({ kind: "Applied" as const }));
    await renderApp(layoutClient({ openPatchFile, previewPatch, applyPatch }));
    const button = within(toolbar()).getByRole("button", { name: "Apply patch…" });
    await waitFor(() => expect(button).not.toHaveAttribute("aria-disabled"));
    fireEvent.click(button);
    const dialog = await screen.findByRole("dialog", { name: "Apply patch" });
    expect(previewPatch).toHaveBeenCalledWith("/repos/a", content);
    expect(applyPatch).not.toHaveBeenCalled();
    fireEvent.click(within(dialog).getByRole("button", { name: "Apply" }));
    await waitFor(() => expect(applyPatch).toHaveBeenCalledWith("/repos/a", content, "WorkTree"));
    expect(openPatchFile).toHaveBeenCalledOnce();
  });
});

describe("dropping files on the window (#649)", () => {
  function dropClient(overrides: Partial<RepoClient> = {}) {
    let emit: (event: FileDropEvent) => void = () => {};
    const client = layoutClient({
      subscribeFileDrops: (listener) => {
        emit = listener;
        return () => {};
      },
      ...overrides,
    });
    return { client, emit: (event: FileDropEvent) => act(() => emit(event)) };
  }

  it("shows a hint while files hover and hides it when they leave", async () => {
    const { client, emit } = dropClient();
    await renderApp(client);
    emit({ kind: "over" });
    expect(screen.getByText(/Drop a folder to open it/)).toBeInTheDocument();
    emit({ kind: "leave" });
    expect(screen.queryByText(/Drop a folder to open it/)).toBeNull();
  });

  it("opens a dropped folder as a tab and applies a dropped patch to the active repository", async () => {
    const openRepo = vi.fn(async () => {});
    const content = { base64: "ZGlmZg==" };
    const readPatchFile = vi.fn(async () => content);
    const applyPatch = vi.fn(async () => ({ kind: "Applied" as const }));
    const { client, emit } = dropClient({ openRepo, readPatchFile, applyPatch });
    await renderApp(client);
    emit({ kind: "over" });
    emit({ kind: "drop", paths: ["/home/me/fix.patch", "/repos/b"] });
    expect(screen.queryByText(/Drop a folder to open it/)).toBeNull();
    await waitFor(() => expect(applyPatch).toHaveBeenCalledWith("/repos/a", content, "WorkTree"));
    expect(readPatchFile).toHaveBeenCalledWith("/home/me/fix.patch");
    await waitFor(() => expect(openRepo).toHaveBeenCalledWith("/repos/b"));
    expect(openRepo).not.toHaveBeenCalledWith("/home/me/fix.patch");
  });
});

describe("the current-branch switcher (#638)", () => {
  it("shows the branch and its upstream, and switches branch from a filtered list", async () => {
    const switchBranch = vi.fn(async () => {});
    await renderApp(layoutClient({ switchBranch }));
    const chip = await within(toolbar()).findByRole("button", { name: /^Current branch main/ });
    expect(chip).toHaveTextContent("main");
    expect(chip).toHaveTextContent("origin/main");
    fireEvent.click(chip);
    const input = await screen.findByRole("combobox", { name: "Switch branch" });
    fireEvent.change(input, { target: { value: "feat" } });
    const options = screen.getAllByRole("option").map((option) => option.textContent);
    expect(options).toContain("feature");
    expect(options).not.toContain("fix/login");
    fireEvent.click(screen.getByRole("option", { name: "feature" }));
    await waitFor(() => expect(switchBranch).toHaveBeenCalledWith("/repos/a", "feature"));
  });

  it("lists recently switched branches first", async () => {
    const switchBranch = vi.fn(async () => {});
    await renderApp(layoutClient({ switchBranch }));
    fireEvent.click(await within(toolbar()).findByRole("button", { name: /^Current branch main/ }));
    fireEvent.change(await screen.findByRole("combobox", { name: "Switch branch" }), { target: { value: "login" } });
    fireEvent.click(screen.getByRole("option", { name: "fix/login" }));
    await waitFor(() => expect(switchBranch).toHaveBeenCalledWith("/repos/a", "fix/login"));

    fireEvent.click(await within(toolbar()).findByRole("button", { name: /^Current branch main/ }));
    await screen.findByRole("combobox", { name: "Switch branch" });
    const options = screen.getAllByRole("option").map((option) => option.textContent);
    expect(options[0]).toBe("fix/login");
  });

  it("offers Stash and switch when local changes are in the way, then toasts with Undo (#653)", async () => {
    const switchBranch = vi.fn(async () => {
      throw new RepoError("LocalChanges", "Your local changes would be overwritten by switching to feature.");
    });
    const stashAndSwitch = vi.fn(async () => ({ kind: "Reapplied" as const }));
    await renderApp(
      layoutClient({
        switchBranch,
        getSwitchConflicts: async () => ["a.txt"],
        stashAndSwitch,
        getUndoState: async () => ({ undo: "Switch to feature with local changes", redo: null }),
      }),
    );
    fireEvent.click(await within(toolbar()).findByRole("button", { name: /^Current branch main/ }));
    fireEvent.click(await screen.findByRole("option", { name: "feature" }));
    const dialog = await screen.findByRole("dialog", { name: "Local changes are in the way" });
    expect(dialog).toHaveTextContent("a.txt");
    fireEvent.click(within(dialog).getByRole("button", { name: "Stash and switch" }));
    await waitFor(() => expect(stashAndSwitch).toHaveBeenCalledWith("/repos/a", "feature", true));
    const notifications = within(screen.getByRole("status", { name: "Notifications" }));
    expect(await notifications.findByText("Switched to feature with your changes")).toBeInTheDocument();
    expect(notifications.getByRole("button", { name: "Undo" })).toBeInTheDocument();
    expect(screen.queryByRole("dialog", { name: "Local changes are in the way" })).toBeNull();
  });

  // One create-and-checkout, so one Undo takes it back (#656).
  it("creates and switches to a new branch from here in one step", async () => {
    const createBranch = vi.fn(async () => {});
    const switchBranch = vi.fn(async () => {});
    await renderApp(layoutClient({ createBranch, switchBranch }));
    fireEvent.click(await within(toolbar()).findByRole("button", { name: /^Current branch main/ }));
    fireEvent.change(await screen.findByRole("combobox", { name: "Switch branch" }), { target: { value: "topic" } });
    fireEvent.click(screen.getByRole("option", { name: "Create branch topic from main" }));
    await waitFor(() => expect(createBranch).toHaveBeenCalledWith("/repos/a", "topic", "HEAD", true));
    expect(switchBranch).not.toHaveBeenCalled();
  });

  it("lists a remote-only branch and checks it out as a tracking branch (#726)", async () => {
    const createBranch = vi.fn(async () => {});
    const setCurrentUpstream = vi.fn(async () => {});
    await renderApp(
      layoutClient({
        createBranch,
        setCurrentUpstream,
        listRemoteBranches: async () => ["main", "feature", "spike"],
      }),
    );
    fireEvent.click(await within(toolbar()).findByRole("button", { name: /^Current branch main/ }));
    expect(screen.queryByRole("option", { name: "origin/feature" })).toBeNull();
    fireEvent.click(await screen.findByRole("option", { name: "origin/spike" }));
    await waitFor(() => expect(createBranch).toHaveBeenCalledWith("/repos/a", "spike", "origin/spike", true));
    await waitFor(() => expect(setCurrentUpstream).toHaveBeenCalledWith("/repos/a", "origin", "spike"));
  });
});

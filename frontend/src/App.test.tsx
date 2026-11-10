import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import App from "./App";
import styles from "./App.module.css";
import { RepoError, type CommitOutcome, type HookProgress, type OperationKind, type RepoClient, type RootFileName, type SequenceOutcome, type TerminalSequence } from "./ipc/RepoClient";
import { credentialRemediation } from "./lib/errorMessages";
import { publishTransportStatus } from "./ipc/transportStatus";
import type { ReleaseNotesEntry } from "./components/ReleaseNotesModal";
import { fakeRepoClient } from "./test/fakeRepoClient";
import { useContext, useEffect } from "react";
import { UpdateCheckContext, type UpdateCheck } from "./state/updateCheck";

// Stands in for the desktop app's UpdateBanner: registers `check` as "Check for updates" (#630).
function FakeUpdater({ check }: { check: UpdateCheck }) {
  const registry = useContext(UpdateCheckContext);
  useEffect(() => registry?.register(check), [registry, check]);
  return null;
}

// The generated release notes, which App reads once at import. Tests swap the contents of this
// array in place; by default it holds the real notes.
const releaseNotes = vi.hoisted(() => [] as ReleaseNotesEntry[]);
vi.mock("./generated/releaseNotes.json", async (importOriginal) => {
  releaseNotes.push(...(await importOriginal<{ default: ReleaseNotesEntry[] }>()).default);
  return { default: releaseNotes };
});

// Every test here renders the whole App (RepoWorkspace and all its panels) and drives it through
// several re-renders, which is far more work than a component test. On an idle machine these
// tests take 0.3–1.2 s; with a load average of about 24 on 4 cores the median was 2.3 s and the
// slowest passing ones 4–5 s, so Vitest's old 5 s default failed different tests on each run
// (#217). 20 s is 4× that worst case; the longer budget only matters when the machine is that
// busy, or when a test hangs. vi.setConfig applies to this file only: Vitest's worker calls
// vi.resetConfig() after every test file, with or without isolation (vitest/dist/chunks/base.*.js,
// "reset after tests, because user might call `vi.setConfig` in setupFile").
vi.setConfig({ testTimeout: 20_000 });

function fakeClient(overrides: Partial<RepoClient> = {}): RepoClient {
  return fakeRepoClient(overrides, "empty");
}

describe("App", () => {
  it("uses its injected RepoClient to restore the initial repositories", async () => {
    const listOpenRepos = vi.fn().mockResolvedValue({ entries: [], activePath: null });
    const client = fakeClient({ listOpenRepos });

    render(<App client={client} />);

    await waitFor(() => expect(listOpenRepos).toHaveBeenCalledOnce());
  });

  it("leaves updater UI absent unless the transport bootstrap injects it", async () => {
    const listOpenRepos = vi.fn().mockResolvedValue({ entries: [], activePath: null });
    const client = fakeClient({ listOpenRepos });
    const view = render(<App client={client} />);
    await waitFor(() => expect(listOpenRepos).toHaveBeenCalledOnce());

    expect(screen.queryByText("Tauri updater")).not.toBeInTheDocument();

    view.rerender(
      <App client={client} updateBanner={<div>Tauri updater</div>} />,
    );

    expect(screen.getByText("Tauri updater")).toBeInTheDocument();
  });

  it("renders a transport status failure with the existing inline error treatment", async () => {
    const listOpenRepos = vi.fn().mockResolvedValue({ entries: [], activePath: null });
    const client = fakeClient({ listOpenRepos });

    render(<App client={client} />);
    await waitFor(() => expect(listOpenRepos).toHaveBeenCalledOnce());

    act(() => {
      publishTransportStatus({
        state: "failed",
        message: "sidecar stopped unexpectedly",
      });
    });

    expect(await screen.findByText("sidecar stopped unexpectedly")).toBeInTheDocument();
  });

  it("renders the header and a loading placeholder while the open repos restore", () => {
    const client = fakeClient({ listOpenRepos: () => new Promise(() => {}) });
    render(<App client={client} />);
    expect(screen.getByRole("heading", { name: "Browsitory" })).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Loading");
  });

  it("shows a failed open-repos restore in the reserved error slot instead of shifting the layout", async () => {
    const client = fakeClient({
      listOpenRepos: async () => {
        throw new Error("config.toml is unreadable");
      },
    });

    render(<App client={client} />);

    const banner = await screen.findByText("Error: config.toml is unreadable");
    // The reserved-space slot (`.errorLayer`, shared with the other App-level banners) floats
    // the banner over the workspace instead of pushing it down (FB-004) — asserting the class
    // instead of computed layout, since jsdom doesn't run layout.
    expect(banner.closest(`.${styles.errorLayer}`)).not.toBeNull();
  });

  it("renders a single Notifications region with a repository open", async () => {
    const client = fakeClient({
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
    });
    render(<App client={client} />);
    await screen.findByRole("tab", { name: "a" });
    expect(screen.getAllByRole("status", { name: "Notifications" })).toHaveLength(1);
  });

  it("shows a checkout's hook in the Hook status strip, not the commit box, and cancels it (#185)", async () => {
    let emit: ((event: HookProgress) => void) | undefined;
    const cancelTransfer = vi.fn(async () => {});
    const client = fakeClient({
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
      cancelTransfer,
      subscribeHookProgress: (listener) => {
        emit = listener;
        return () => {};
      },
    });
    render(<App client={client} />);
    await screen.findByRole("tab", { name: "a" });

    act(() =>
      emit?.({ repoPath: "/repos/a", operationId: "switch-1", hook: "post-checkout", phase: "Started", source: "Checkout" }),
    );

    const strip = screen.getByRole("status", { name: "Hook status" });
    expect(strip).toHaveTextContent("Running post-checkout hook…");
    expect(screen.getAllByText("Running post-checkout hook…")).toHaveLength(1);
    fireEvent.click(within(strip).getByRole("button", { name: "Cancel post-checkout hook" }));
    await waitFor(() => expect(cancelTransfer).toHaveBeenCalledWith("/repos/a", "switch-1"));

    act(() =>
      emit?.({ repoPath: "/repos/a", operationId: "switch-1", hook: "post-checkout", phase: "Finished", source: "Checkout" }),
    );
    expect(screen.getByRole("status", { name: "Hook status" })).toHaveTextContent("The post-checkout hook was cancelled.");

    act(() =>
      emit?.({ repoPath: "/repos/a", operationId: "commit-2", hook: "pre-commit", phase: "Started", source: "Commit" }),
    );
    // A commit's hook shows in the commit box, never in the strip.
    expect(screen.getByRole("status", { name: "Hook status" })).not.toHaveTextContent("pre-commit");
  });

  it("closes the active tab on Ctrl/Cmd+W", async () => {
    const closeRepo = vi.fn().mockResolvedValue(undefined);
    const client = fakeClient({
      listOpenRepos: async () => ({
        entries: [
          { path: "/repos/a", workspaceId: null },
          { path: "/repos/b", workspaceId: null },
        ],
        activePath: "/repos/a",
      }),
      openRepo: async () => {},
      closeRepo,
      persistOpenRepos: async () => {},
      // RepoWorkspace's mount-time refresh() fans out to these; fakeClient has no empty answer
      // for them (they throw), which is fine for tests that never mount a RepoWorkspace but
      // this one does.
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
    });

    const { container } = render(<App client={client} />);

    await screen.findByRole("tab", { name: "a" });
    expect(screen.getByRole("tab", { name: "b" })).toBeInTheDocument();
    // The Ctrl/Cmd+W listener lives in the active RepoWorkspace, which mounts after the tabs
    // render, and it ignores the key while the workspace is busy. Wait for both, or a keydown
    // fired under load lands before the listener exists and is lost.
    await waitFor(() => expect(container.querySelector('[data-active-repo="true"]')).not.toBeNull());
    await waitFor(() => expect(screen.getAllByTitle("Close (Delete)")).toHaveLength(2));

    fireEvent.keyDown(window, { key: "w", ctrlKey: true });

    await waitFor(
      () => expect(screen.queryByRole("tab", { name: "a" })).not.toBeInTheDocument(),
      { timeout: 3000 },
    );
    expect(screen.getByRole("tab", { name: "b" })).toBeInTheDocument();
  });

  it("reopens a closed tab at its old position on Ctrl/Cmd+Shift+T (#613)", async () => {
    const openRepo = vi.fn().mockResolvedValue(undefined);
    const client = fakeClient({
      listOpenRepos: async () => ({
        entries: [
          { path: "/repos/a", workspaceId: null },
          { path: "/repos/b", workspaceId: null },
        ],
        activePath: "/repos/a",
      }),
      openRepo,
      closeRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
    });
    const { container } = render(<App client={client} />);
    await waitFor(() => expect(container.querySelector('[data-active-repo="true"]')).not.toBeNull());
    await waitFor(() => expect(screen.getAllByTitle("Close (Delete)")).toHaveLength(2));

    fireEvent.keyDown(window, { key: "w", ctrlKey: true });
    await waitFor(() => expect(screen.queryByRole("tab", { name: "a" })).not.toBeInTheDocument(), { timeout: 3000 });
    await waitFor(() => expect(container.querySelector('[data-active-repo="true"]')).not.toBeNull());

    fireEvent.keyDown(window, { key: "T", ctrlKey: true, shiftKey: true });

    await waitFor(() => expect(screen.getByRole("tab", { name: "a", selected: true })).toBeInTheDocument(), { timeout: 3000 });
    expect(screen.getAllByRole("tab").map((tab) => tab.textContent)).toEqual(["a", "b"]);
    expect(openRepo).toHaveBeenLastCalledWith("/repos/a");
  });

  it("marks an inactive tab with uncommitted changes, checked through getStatus (#613)", async () => {
    const change = { path: "file.txt", oldPath: null, staged: false, kind: "Modified" as const };
    const getStatus = vi.fn(async (path: string) => (path === "/repos/b" ? [change] : []));
    const client = fakeClient({
      listOpenRepos: async () => ({
        entries: [
          { path: "/repos/a", workspaceId: null },
          { path: "/repos/b", workspaceId: null },
        ],
        activePath: "/repos/a",
      }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus,
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
    });
    render(<App client={client} />);

    await waitFor(() => expect(screen.getByRole("tab", { name: "b" })).toHaveAccessibleDescription("Uncommitted changes"));
    expect(screen.getByRole("tab", { name: "a" })).not.toHaveAccessibleDescription("Uncommitted changes");
  });

  it("opens the user guide site from the palette through the client (#320)", async () => {
    const openExternalUrl = vi.fn().mockResolvedValue(undefined);
    const client = fakeClient({
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
      openExternalUrl,
    });
    const { container } = render(<App client={client} />);
    await waitFor(() => expect(container.querySelector('[data-active-repo="true"]')).not.toBeNull());

    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    const input = await screen.findByRole("combobox", { name: "Command palette" });
    fireEvent.change(input, { target: { value: "Open user guide" } });
    fireEvent.click(await screen.findByRole("option", { name: "Open user guide" }));

    await waitFor(() => expect(openExternalUrl).toHaveBeenCalledWith("https://rene-langer.github.io/browsitory/"));
  });

  // #494: the palette's sidebar commands go through the workspace's reveal requests, which the
  // sections handle themselves.
  it("expands a collapsed sidebar section and a remote's folder from the palette (#494)", async () => {
    localStorage.setItem("sidebar-tags", "closed");
    localStorage.setItem("sidebar-branches", "closed");
    const listRemoteBranches = vi.fn().mockResolvedValue(["develop"]);
    const client = fakeClient({
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
      listRemotes: async () => [{ name: "origin", fetchUrl: "https://example.com/r.git", pushUrl: null, authMode: null, authUsername: null }],
      listRemoteBranches,
    });
    const { container } = render(<App client={client} />);
    await waitFor(() => expect(container.querySelector('[data-active-repo="true"]')).not.toBeNull());
    const sidebar = screen.getByRole("complementary", { name: "Repository sections" });
    expect(within(sidebar).getByRole("button", { name: "Tags" })).toHaveAttribute("aria-expanded", "false");

    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    fireEvent.change(await screen.findByRole("combobox", { name: "Command palette" }), { target: { value: "Go to Tags" } });
    fireEvent.click(await screen.findByRole("option", { name: "Go to Tags" }));
    expect(within(sidebar).getByRole("button", { name: "Tags" })).toHaveAttribute("aria-expanded", "true");

    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    fireEvent.change(await screen.findByRole("combobox", { name: "Command palette" }), {
      target: { value: "Browse branches on origin" },
    });
    fireEvent.click(await screen.findByRole("option", { name: "Browse branches on origin" }));
    expect(within(sidebar).getByRole("button", { name: "Branches" })).toHaveAttribute("aria-expanded", "true");
    expect(within(sidebar).getByRole("button", { name: "origin" })).toHaveAttribute("aria-expanded", "true");
    expect(await within(sidebar).findByText("develop")).toBeInTheDocument();
    expect(listRemoteBranches).toHaveBeenCalledWith("/repos/a", "origin");
  });

  function guideClient(openExternalUrl: RepoClient["openExternalUrl"]) {
    return fakeClient({
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
      openExternalUrl,
    });
  }

  it("says so when the user guide can't be opened from the palette (#320)", async () => {
    const openExternalUrl = vi.fn().mockRejectedValue(new Error("no browser"));
    const { container } = render(<App client={guideClient(openExternalUrl)} />);
    await waitFor(() => expect(container.querySelector('[data-active-repo="true"]')).not.toBeNull());

    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    const input = await screen.findByRole("combobox", { name: "Command palette" });
    fireEvent.change(input, { target: { value: "Open user guide" } });
    fireEvent.click(await screen.findByRole("option", { name: "Open user guide" }));

    expect(await screen.findByText("Couldn't open the user guide: no browser")).toBeInTheDocument();
  });

  it("closes the release notes before opening the user guide, so a failure is visible (#320)", async () => {
    const openExternalUrl = vi.fn().mockRejectedValue(new Error("no browser"));
    const { container } = render(<App client={guideClient(openExternalUrl)} />);
    await waitFor(() => expect(container.querySelector('[data-active-repo="true"]')).not.toBeNull());

    fireEvent.click(screen.getByRole("button", { name: "Release notes" }));
    fireEvent.click(await screen.findByRole("button", { name: "User guide" }));

    expect(await screen.findByText("Couldn't open the user guide: no browser")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "User guide" })).not.toBeInTheDocument();
    expect(openExternalUrl).toHaveBeenCalledWith("https://rene-langer.github.io/browsitory/");
  });

  describe("the header's palette button and Help menu (#647)", () => {
    it("opens the command palette from the Commands button, and closes it again", async () => {
      const { container } = render(<App client={guideClient(vi.fn().mockResolvedValue(undefined))} />);
      await waitFor(() => expect(container.querySelector('[data-active-repo="true"]')).not.toBeNull());

      const button = screen.getByRole("button", { name: /^Commands/ });
      expect(button).toHaveAttribute("aria-keyshortcuts", "Control+K");
      fireEvent.click(button);
      expect(await screen.findByRole("combobox", { name: "Command palette" })).toBeInTheDocument();
      fireEvent.keyDown(window, { key: "k", ctrlKey: true });
      await waitFor(() => expect(screen.queryByRole("combobox", { name: "Command palette" })).not.toBeInTheDocument());
    });

    it("opens the user guide, the keyboard shortcuts and what's new from the Help menu", async () => {
      const openExternalUrl = vi.fn().mockResolvedValue(undefined);
      const { container } = render(<App client={guideClient(openExternalUrl)} />);
      await waitFor(() => expect(container.querySelector('[data-active-repo="true"]')).not.toBeNull());

      const help = screen.getByRole("button", { name: "Help" });
      expect(help).toHaveAttribute("aria-haspopup", "menu");
      fireEvent.click(help);
      fireEvent.click(await screen.findByRole("menuitem", { name: "User guide" }));
      await waitFor(() => expect(openExternalUrl).toHaveBeenCalledWith("https://rene-langer.github.io/browsitory/"));

      fireEvent.click(help);
      fireEvent.click(await screen.findByRole("menuitem", { name: "Keyboard shortcuts" }));
      expect(await screen.findByRole("heading", { name: "Keyboard shortcuts" })).toBeInTheDocument();
      fireEvent.click(screen.getByRole("button", { name: "Close" }));
      await waitFor(() => expect(screen.queryByRole("heading", { name: "Keyboard shortcuts" })).not.toBeInTheDocument());

      fireEvent.click(help);
      fireEvent.click(await screen.findByRole("menuitem", { name: "What’s new" }));
      expect(await screen.findByRole("button", { name: "User guide" })).toBeInTheDocument();
    });

    it("offers the Help menu, without the shortcuts, before any repository is open", async () => {
      const client = fakeClient({
        listOpenRepos: async () => ({ entries: [], activePath: null }),
        persistOpenRepos: async () => {},
      });
      render(<App client={client} />);
      const help = await screen.findByRole("button", { name: "Help" });
      expect(screen.queryByRole("button", { name: /^Commands/ })).not.toBeInTheDocument();
      fireEvent.click(help);
      expect(await screen.findByRole("menuitem", { name: "User guide" })).toBeInTheDocument();
      expect(screen.queryByRole("menuitem", { name: "Keyboard shortcuts" })).not.toBeInTheDocument();
    });
  });

  describe("palette discard commands", () => {
    function discardClient(overrides: Partial<RepoClient> = {}) {
      return fakeClient({
        listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
        openRepo: async () => {},
        persistOpenRepos: async () => {},
        getStatus: async () => [{ path: "a.txt", oldPath: null, staged: false, kind: "Modified" }],
        getCommitGraph: async () => [],
        listBranches: async () => [],
        listStashes: async () => [],
        getDiffSettings: async () => ({ ignoreWhitespace: false, contextLines: 3, syntaxHighlight: true, wordWrap: "Auto", algorithm: "Default", showWhitespace: false, expandUntracked: true }),
        discardFile: vi.fn().mockResolvedValue(undefined),
        discardAllUnstaged: vi.fn().mockResolvedValue(undefined),
        ...overrides,
      });
    }

    async function runPaletteCommand(label: string) {
      fireEvent.keyDown(window, { key: "k", ctrlKey: true });
      const input = await screen.findByRole("combobox", { name: "Command palette" });
      fireEvent.change(input, { target: { value: label } });
      fireEvent.click(await screen.findByRole("option", { name: label }));
    }

    it("opens the file confirm without discarding, then cancels and confirms", async () => {
      const client = discardClient();
      render(<App client={client} />);
      fireEvent.click(await screen.findByText("a.txt (Modified)"));

      await runPaletteCommand("Discard changes in current file");
      expect(await screen.findByText("Discard changes to a.txt? This can't be undone.")).toBeInTheDocument();
      expect(client.discardFile).not.toHaveBeenCalled();

      fireEvent.click(screen.getByText("Cancel"));
      expect(screen.queryByText("Discard changes to a.txt? This can't be undone.")).not.toBeInTheDocument();
      expect(client.discardFile).not.toHaveBeenCalled();

      await runPaletteCommand("Discard changes in current file");
      fireEvent.click(await screen.findByRole("button", { name: "Discard changes" }));
      await waitFor(() => expect(client.discardFile).toHaveBeenCalledTimes(1));
      expect(client.discardFile).toHaveBeenCalledWith("/repos/a", "a.txt");
    });

    it("opens the discard-all confirm without discarding, then cancels and confirms", async () => {
      const client = discardClient();
      render(<App client={client} />);
      await screen.findByText("a.txt (Modified)");

      await runPaletteCommand("Discard all unstaged changes");
      expect(await screen.findByText("Discard changes to 1 file? This can't be undone.")).toBeInTheDocument();
      expect(client.discardAllUnstaged).not.toHaveBeenCalled();

      fireEvent.click(screen.getByText("Cancel"));
      expect(screen.queryByText("Discard changes to 1 file? This can't be undone.")).not.toBeInTheDocument();
      expect(client.discardAllUnstaged).not.toHaveBeenCalled();

      await runPaletteCommand("Discard all unstaged changes");
      fireEvent.click(await screen.findByRole("button", { name: "Discard all" }));
      await waitFor(() => expect(client.discardAllUnstaged).toHaveBeenCalledTimes(1));
      expect(client.discardAllUnstaged).toHaveBeenCalledWith("/repos/a", ["a.txt"]);
    });

    // #521: discarding while resolving a merge or a paused step is allowed; only an action in
    // flight or a transfer blocks it (`busyReason`), not `operationBlockers`.
    it("discards during a merge, which blocks other repository operations (#521)", async () => {
      const client = discardClient({ getMergeMessage: async () => "Merge branch 'x'" });
      render(<App client={client} />);
      await screen.findByText("a.txt (Modified)");

      await runPaletteCommand("Discard all unstaged changes");
      const confirm = await screen.findByRole("button", { name: "Discard all" });
      expect(confirm).toBeEnabled();
      fireEvent.click(confirm);
      await waitFor(() => expect(client.discardAllUnstaged).toHaveBeenCalledWith("/repos/a", ["a.txt"]));
    });
  });

  it("closes a focused, inactive tab on Delete without switching away from the active one", async () => {
    const client = fakeClient({
      listOpenRepos: async () => ({
        entries: [
          { path: "/repos/a", workspaceId: null },
          { path: "/repos/b", workspaceId: null },
        ],
        activePath: "/repos/a",
      }),
      openRepo: async () => {},
      closeRepo: vi.fn().mockResolvedValue(undefined),
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
    });

    render(<App client={client} />);
    const tabB = await screen.findByRole("tab", { name: "b" });
    tabB.focus();

    fireEvent.keyDown(tabB, { key: "Delete" });

    await waitFor(() => expect(screen.queryByRole("tab", { name: "b" })).not.toBeInTheDocument());
    expect(screen.getByRole("tab", { name: "a" })).toHaveAttribute("aria-selected", "true");
    expect(client.closeRepo).toHaveBeenCalledWith("/repos/b");
  });

  it("unmounts an inactive repo's workspace instead of hiding it with CSS (PERF-001)", async () => {
    const client = fakeClient({
      listOpenRepos: async () => ({
        entries: [
          { path: "/repos/a", workspaceId: null },
          { path: "/repos/b", workspaceId: null },
        ],
        activePath: "/repos/b",
      }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      // RepoWorkspace's mount-time refresh() fans out to these for whichever tab actually mounts.
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
    });

    render(<App client={client} />);

    await screen.findByRole("tab", { name: "a" });
    expect(screen.getByRole("tab", { name: "b" })).toBeInTheDocument();

    // Both tabs exist in the tab strip, but only the active repo's workspace (b) is mounted —
    // the audit's own evidence for this finding was three open repos producing three copies of
    // section headings like "Branches" in the DOM; the fix renders exactly one.
    await waitFor(() => expect(screen.queryAllByText("Branches")).toHaveLength(1));
  });

  it("keeps a half-typed commit message across switching tabs away and back", async () => {
    const client = fakeClient({
      listOpenRepos: async () => ({
        entries: [
          { path: "/repos/a", workspaceId: null },
          { path: "/repos/b", workspaceId: null },
        ],
        activePath: "/repos/a",
      }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
    });

    render(<App client={client} />);
    await screen.findByRole("tab", { name: "a" });

    fireEvent.change(await screen.findByRole("textbox", { name: "Commit message" }), {
      target: { value: "wip: half-typed" },
    });

    fireEvent.click(screen.getByRole("tab", { name: "b" }));
    // b's workspace replaced a's (PERF-001 unmounts inactive tabs), with its own empty draft.
    await waitFor(() => expect(screen.getByRole("tab", { name: "b" })).toHaveAttribute("aria-selected", "true"));
    expect(screen.getByRole("textbox", { name: "Commit message" })).toHaveValue("");

    fireEvent.click(screen.getByRole("tab", { name: "a" }));
    await waitFor(() => expect(screen.getByRole("tab", { name: "a" })).toHaveAttribute("aria-selected", "true"));
    expect(screen.getByRole("textbox", { name: "Commit message" })).toHaveValue("wip: half-typed");
  });

  it("re-enables a tab's close button once it is switched away from mid-operation", async () => {
    const client = fakeClient({
      listOpenRepos: async () => ({
        entries: [
          { path: "/repos/a", workspaceId: null },
          { path: "/repos/b", workspaceId: null },
        ],
        activePath: "/repos/a",
      }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [{ path: "x.txt", oldPath: null, staged: false, kind: "Modified" }],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
      // Never settles: the stash stays in flight, so repo a stays busy for as long as its
      // workspace is mounted.
      saveStash: () => new Promise(() => {}),
    });

    render(<App client={client} />);
    const tabA = await screen.findByRole("tab", { name: "a" });
    // The per-tab close button is `aria-hidden` (mouse-only), so it's found by position: the
    // tab's sibling inside its `role="presentation"` wrapper.
    const closeA = () => tabA.parentElement!.querySelector<HTMLButtonElement>("button:not([role='tab'])")!;

    const stash = await screen.findByRole("button", { name: "Stash" });
    await waitFor(() => expect(stash).toBeEnabled());
    fireEvent.click(stash);
    await waitFor(() => expect(closeA()).toBeDisabled());

    fireEvent.click(screen.getByRole("tab", { name: "b" }));

    await waitFor(() => expect(closeA()).toBeEnabled());
  });

  it("switching away from a tab mid-rebase does not keep its close button disabled", async () => {
    const client = fakeClient({
      listOpenRepos: async () => ({
        entries: [
          { path: "/repos/a", workspaceId: null },
          { path: "/repos/b", workspaceId: null },
        ],
        activePath: "/repos/a",
      }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [{ path: "x.txt", oldPath: null, staged: false, kind: "Modified" }],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
      // The paused rebase is stored on disk (`rebase.json`), so closing this tab no longer loses
      // it: a backgrounded tab mid-rebase is not kept busy.
      getHeadInfo: async () => ({
        headId: "h1", detachedAt: "h1", headPushed: false, operation: "CherryPick" as const, operationLeftover: false, squashMessage: null, commitHooks: [], sequence: null,
        rebase: { branch: "main", onto: "o1", currentStep: 1, totalSteps: 3, health: { kind: "Paused" as const } },
        bisect: null, terminalSequence: null, terminalRebase: null,
      }),
    });

    render(<App client={client} />);
    const tabA = await screen.findByRole("tab", { name: "a" });
    const closeA = () => tabA.parentElement!.querySelector<HTMLButtonElement>("button:not([role='tab'])")!;

    await waitFor(() => expect(closeA()).toBeDisabled());

    fireEvent.click(screen.getByRole("tab", { name: "b" }));
    await waitFor(() => expect(screen.getByRole("tab", { name: "b" })).toHaveAttribute("aria-selected", "true"));

    // Like the plain pending/transfer case above: reopening the tab restores the rebase.
    await waitFor(() => expect(closeA()).toBeEnabled());
  });

  it("opens the shortcut sheet on ? outside of a text input", async () => {
    const client = fakeClient({
      listOpenRepos: async () => ({
        entries: [{ path: "/repos/a", workspaceId: null }],
        activePath: "/repos/a",
      }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
    });

    const { container } = render(<App client={client} />);
    await screen.findByRole("tab", { name: "a" });
    // The ? listener lives in the active RepoWorkspace, which mounts after the tabs (#135).
    await waitFor(() => expect(container.querySelector('[data-active-repo="true"]')).not.toBeNull());

    fireEvent.keyDown(window, { key: "?" });

    expect(await screen.findByRole("heading", { name: "Keyboard shortcuts" })).toBeInTheDocument();
  });

  it("does not open the shortcut sheet on ? while typing in a text field", async () => {
    const client = fakeClient({
      listOpenRepos: async () => ({
        entries: [{ path: "/repos/a", workspaceId: null }],
        activePath: "/repos/a",
      }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
    });

    render(<App client={client} />);
    await screen.findByRole("tab", { name: "a" });

    // Create a temporary input and fire the event on it
    const input = document.createElement("input");
    document.body.appendChild(input);
    fireEvent.keyDown(input, { key: "?" });
    document.body.removeChild(input);

    expect(screen.queryByRole("heading", { name: "Keyboard shortcuts" })).not.toBeInTheDocument();
  });

  it("collapses the sidebar automatically below the narrow-window breakpoint", async () => {
    const client = fakeClient({
      listOpenRepos: async () => ({
        entries: [{ path: "/repos/a", workspaceId: null }],
        activePath: "/repos/a",
      }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
    });

    // Start with a wide window (above NARROW_BREAKPOINT of 900)
    const originalInnerWidth = window.innerWidth;
    Object.defineProperty(window, "innerWidth", {
      writable: true,
      configurable: true,
      value: 1200,
    });

    render(<App client={client} />);
    await screen.findByRole("tab", { name: "a" });
    await waitFor(() => expect(screen.queryAllByText("Branches")).toHaveLength(1));

    // The sidebar pane is the previous sibling of the divider (separator).
    // It has hidden={shownWidth === 0} where shownWidth depends on forceCollapsed.
    const divider = screen.getByRole("separator", { name: "Sidebar width" });
    const sidebarPane = divider.previousElementSibling as HTMLElement;

    // Sidebar should not be hidden when above breakpoint
    expect(sidebarPane).not.toHaveAttribute("hidden");

    // Narrow the window below NARROW_BREAKPOINT
    window.innerWidth = 800;
    act(() => {
      fireEvent(window, new Event("resize"));
    });

    // Sidebar should be hidden when below breakpoint
    await waitFor(() => {
      expect(sidebarPane).toHaveAttribute("hidden");
    });

    // Restore original width
    Object.defineProperty(window, "innerWidth", {
      writable: true,
      configurable: true,
      value: originalInnerWidth,
    });
  });

  it("shows a failing commit hook's output, keeps the message, and commits without hooks", async () => {
    const commit = vi
      .fn<RepoClient["commit"]>()
      .mockResolvedValueOnce({ kind: "HookFailed", hook: "pre-commit", output: "lint failed\n" })
      .mockResolvedValueOnce({ kind: "Committed", commitId: "c1" });
    const client = fakeClient({
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [{ path: "a.txt", oldPath: null, staged: true, kind: "Modified" }],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
      commit,
    });
    render(<App client={client} />);

    const box = await screen.findByRole("textbox", { name: "Commit message" });
    fireEvent.change(box, { target: { value: "feat: hooked" } });
    const commitButton = screen.getByRole("button", { name: "Commit" });
    await waitFor(() => expect(commitButton).toBeEnabled());
    fireEvent.click(commitButton);

    const dialog = await screen.findByRole("dialog", { name: "pre-commit hook failed" });
    expect(within(dialog).getByLabelText("Hook output")).toHaveTextContent("lint failed");
    expect(commit).toHaveBeenCalledWith("/repos/a", "feat: hooked", false, []);
    expect(box).toHaveValue("feat: hooked");

    fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "pre-commit hook failed" })).toBeNull());
    expect(screen.getByText("The pre-commit hook failed.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Show hook output…" }));
    const reopened = await screen.findByRole("dialog", { name: "pre-commit hook failed" });

    fireEvent.click(within(reopened).getByRole("button", { name: "Commit without hooks" }));

    await waitFor(() => expect(commit).toHaveBeenLastCalledWith("/repos/a", "feat: hooked", true, []));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "pre-commit hook failed" })).toBeNull());
    await waitFor(() => expect(box).toHaveValue(""));
  });

  it("offers committed messages and the branch's recent commits under Recent messages (#443)", async () => {
    localStorage.clear();
    const graphCommit = (id: string, summary: string, parentIds: string[]) => ({
      id,
      shortId: id.slice(0, 7),
      summary,
      authorName: "A",
      authorEmail: "a@example.com",
      timestamp: 0,
      parentIds,
      branchRefs: [],
      remoteBranchRefs: [],
    });
    const getCommitMessage = vi.fn(async (_repo: string, id: string) => `${id} full message\n\nBody`);
    const client = fakeClient({
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [{ path: "a.txt", oldPath: null, staged: true, kind: "Modified" }],
      getCommitGraph: async () => [graphCommit("head111", "Tip of main", ["base222"]), graphCommit("base222", "Base", [])],
      getHeadInfo: async () => ({
        headId: "head111",
        detachedAt: null,
        headPushed: false,
        operation: null,
        operationLeftover: false,
        squashMessage: null,
        commitHooks: [],
        sequence: null,
        rebase: null,
        bisect: null, terminalSequence: null, terminalRebase: null,
      }),
      listBranches: async () => [],
      listStashes: async () => [],
      getCommitMessage,
      commit: async () => ({ kind: "Committed", commitId: "c1" }),
    });
    try {
      render(<App client={client} />);
      const box = await screen.findByRole("textbox", { name: "Commit message" });
      fireEvent.change(box, { target: { value: "feat: remembered\n\nwith a body" } });
      const commitButton = screen.getByRole("button", { name: "Commit" });
      await waitFor(() => expect(commitButton).toBeEnabled());
      fireEvent.click(commitButton);
      await waitFor(() => expect(box).toHaveValue(""));

      fireEvent.click(screen.getByRole("button", { name: "Recent messages" }));
      const items = within(screen.getByRole("menu")).getAllByRole("menuitem");
      expect(items.map((item) => item.textContent)).toEqual(["feat: remembered", "Tip of main", "Base"]);
      fireEvent.click(items[0]);
      await waitFor(() => expect(box).toHaveValue("feat: remembered\n\nwith a body"));

      fireEvent.click(screen.getByRole("button", { name: "Recent messages" }));
      fireEvent.click(screen.getByRole("menuitem", { name: "Tip of main" }));
      await waitFor(() => expect(box).toHaveValue("head111 full message\n\nBody"));
      expect(getCommitMessage).toHaveBeenCalledWith("/repos/a", "head111");
      fireEvent.click(screen.getByRole("button", { name: "Restore message" }));
      expect(box).toHaveValue("feat: remembered\n\nwith a body");
    } finally {
      localStorage.clear();
    }
  });

  it("a failing hook on Continue opens the hook dialog and Continue without hooks finishes the sequence", async () => {
    const continueSequence = vi
      .fn<RepoClient["continueSequence"]>()
      .mockResolvedValueOnce({
        kind: "HookFailed",
        hook: "pre-commit",
        output: "lint failed\n",
        progress: { kind: "CherryPick", currentStep: 2, totalSteps: 3, currentCommit: "c2" },
      })
      .mockImplementationOnce(() => retried);
    let finishRetry: (outcome: SequenceOutcome) => void = () => {};
    const retried = new Promise<SequenceOutcome>((resolve) => {
      finishRetry = resolve;
    });
    const sequence = { kind: "CherryPick" as const, currentStep: 2, totalSteps: 3, currentCommit: "c2" };
    const client = fakeClient({
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [{ path: "a.txt", oldPath: null, staged: true, kind: "Modified" }],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
      getMergeMessage: async () => "f two",
      getHeadInfo: async () => ({
        headId: "abc",
        detachedAt: null,
        headPushed: false,
        operation: "CherryPick",
        operationLeftover: false,
        squashMessage: null,
        commitHooks: ["pre-commit"],
        sequence,
        rebase: null,
        bisect: null, terminalSequence: null, terminalRebase: null,
      }),
      continueSequence,
    });
    render(<App client={client} />);

    const continueButton = await screen.findByRole("button", { name: "Continue cherry-pick" });
    await waitFor(() => expect(continueButton).toBeEnabled());
    fireEvent.click(continueButton);

    const dialog = await screen.findByRole("dialog", { name: "pre-commit hook failed" });
    expect(within(dialog).getByLabelText("Hook output")).toHaveTextContent("lint failed");
    expect(continueSequence).toHaveBeenCalledWith("/repos/a", false);

    fireEvent.click(within(dialog).getByRole("button", { name: "Continue without hooks" }));

    await waitFor(() => expect(continueSequence).toHaveBeenLastCalledWith("/repos/a", true));
    // Closed while the retry still runs, so it can't be clicked twice and doesn't hide the
    // panel's running-hook line.
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "pre-commit hook failed" })).toBeNull());
    finishRetry({ kind: "Completed", applied: 3, skipped: [] });
    expect(await screen.findByText("Cherry-picked 3 commits")).toBeInTheDocument();
  });

  it("commits without hooks with the co-authors the box holds, then clears them", async () => {
    const commit = vi
      .fn<RepoClient["commit"]>()
      .mockResolvedValueOnce({ kind: "HookFailed", hook: "commit-msg", output: "no\n" })
      .mockResolvedValueOnce({ kind: "Committed", commitId: "c1" });
    const client = fakeClient({
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [{ path: "a.txt", oldPath: null, staged: true, kind: "Modified" }],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
      listRecentAuthors: async () => [
        { name: "Ada", email: "ada@x.org" },
        { name: "Bob", email: "bob@x.org" },
      ],
      commit,
    });
    render(<App client={client} />);

    const box = await screen.findByRole("textbox", { name: "Commit message" });
    fireEvent.change(box, { target: { value: "feat: pair" } });
    fireEvent.click(screen.getByRole("button", { name: "Add co-author" }));
    fireEvent.click(await screen.findByRole("option", { name: "Ada <ada@x.org>" }));
    const commitButton = screen.getByRole("button", { name: "Commit" });
    await waitFor(() => expect(commitButton).toBeEnabled());
    fireEvent.click(commitButton);
    const ada = { key: "Co-authored-by", value: "Ada <ada@x.org>" };
    const dialog = await screen.findByRole("dialog", { name: "commit-msg hook failed" });
    expect(commit).toHaveBeenCalledWith("/repos/a", "feat: pair", false, [ada]);

    // A chip added after the failure goes into the retry too.
    fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "commit-msg hook failed" })).toBeNull());
    fireEvent.click(screen.getByRole("button", { name: "Add co-author" }));
    fireEvent.click(await screen.findByRole("option", { name: "Bob <bob@x.org>" }));
    fireEvent.click(screen.getByRole("button", { name: "Show hook output…" }));
    const reopened = await screen.findByRole("dialog", { name: "commit-msg hook failed" });
    fireEvent.click(within(reopened).getByRole("button", { name: "Commit without hooks" }));

    await waitFor(() =>
      expect(commit).toHaveBeenLastCalledWith("/repos/a", "feat: pair", true, [
        ada,
        { key: "Co-authored-by", value: "Bob <bob@x.org>" },
      ]),
    );
    await waitFor(() => expect(box).toHaveValue(""));
    expect(screen.queryByRole("list", { name: "Co-authors" })).toBeNull();
  });

  it("never adds hidden co-authors to a merge commit retried without hooks", async () => {
    let mergeMessage: string | null = null;
    let staged = false;
    const commit = vi
      .fn<RepoClient["commit"]>()
      .mockResolvedValueOnce({ kind: "HookFailed", hook: "pre-commit", output: "" })
      .mockResolvedValueOnce({ kind: "Committed", commitId: "c1" });
    const client = fakeClient({
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [{ path: "a.txt", oldPath: null, staged, kind: "Modified" as const }],
      getMergeMessage: async () => mergeMessage,
      stageFile: async () => {
        staged = true;
      },
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
      listRecentAuthors: async () => [{ name: "Ada", email: "ada@x.org" }],
      commit,
    });
    render(<App client={client} />);

    fireEvent.click(await screen.findByRole("button", { name: "Add co-author" }));
    fireEvent.click(await screen.findByRole("option", { name: "Ada <ada@x.org>" }));
    mergeMessage = "Merge branch 'x'";
    fireEvent.click(screen.getByRole("button", { name: "Stage a.txt" }));
    const box = screen.getByRole("textbox", { name: "Commit message" });
    await waitFor(() => expect(box).toHaveValue("Merge branch 'x'"));
    expect(screen.queryByRole("button", { name: "Add co-author" })).toBeNull();
    const commitButton = screen.getByRole("button", { name: "Commit" });
    await waitFor(() => expect(commitButton).toBeEnabled());
    fireEvent.click(commitButton);
    const dialog = await screen.findByRole("dialog", { name: "pre-commit hook failed" });
    expect(commit).toHaveBeenCalledWith("/repos/a", "Merge branch 'x'", false, []);

    fireEvent.click(within(dialog).getByRole("button", { name: "Commit without hooks" }));
    await waitFor(() => expect(commit).toHaveBeenLastCalledWith("/repos/a", "Merge branch 'x'", true, []));
  });

  it("does not bring back a committed message when the tab was switched away mid-commit", async () => {
    let finish: (outcome: CommitOutcome) => void = () => {};
    const commit = vi.fn(() => new Promise<CommitOutcome>((resolve) => (finish = resolve)));
    const client = fakeClient({
      listOpenRepos: async () => ({
        entries: [
          { path: "/repos/a", workspaceId: null },
          { path: "/repos/b", workspaceId: null },
        ],
        activePath: "/repos/a",
      }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async (repoPath: string) => (repoPath === "/repos/a" ? [{ path: "a.txt", oldPath: null, staged: true, kind: "Modified" as const }] : []),
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
      commit,
    });
    render(<App client={client} />);
    await screen.findByRole("tab", { name: "a" });
    fireEvent.change(await screen.findByRole("textbox", { name: "Commit message" }), {
      target: { value: "feat: in flight" },
    });
    const commitButton = screen.getByRole("button", { name: "Commit" });
    await waitFor(() => expect(commitButton).toBeEnabled());
    fireEvent.click(commitButton);
    await waitFor(() => expect(commit).toHaveBeenCalled());

    fireEvent.click(screen.getByRole("tab", { name: "b" }));
    await waitFor(() => expect(screen.getByRole("tab", { name: "b" })).toHaveAttribute("aria-selected", "true"));
    await act(async () => finish({ kind: "Committed", commitId: "c1" }));
    fireEvent.click(screen.getByRole("tab", { name: "a" }));
    await waitFor(() => expect(screen.getByRole("tab", { name: "a" })).toHaveAttribute("aria-selected", "true"));

    expect(await screen.findByRole("textbox", { name: "Commit message" })).toHaveValue("");
  });

  it("retries a failed commit with the message as edited after the hook failed", async () => {
    const commit = vi
      .fn<RepoClient["commit"]>()
      .mockResolvedValueOnce({ kind: "HookFailed", hook: "pre-commit", output: "subject too long\n" })
      .mockResolvedValueOnce({ kind: "Committed", commitId: "c1" });
    const client = fakeClient({
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [{ path: "a.txt", oldPath: null, staged: true, kind: "Modified" }],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
      commit,
    });
    render(<App client={client} />);
    const box = await screen.findByRole("textbox", { name: "Commit message" });
    fireEvent.change(box, { target: { value: "feat: a very long subject" } });
    const commitButton = screen.getByRole("button", { name: "Commit" });
    await waitFor(() => expect(commitButton).toBeEnabled());
    fireEvent.click(commitButton);
    const dialog = await screen.findByRole("dialog", { name: "pre-commit hook failed" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "pre-commit hook failed" })).toBeNull());

    fireEvent.change(box, { target: { value: "feat: short" } });
    fireEvent.click(screen.getByRole("button", { name: "Show hook output…" }));
    const reopened = await screen.findByRole("dialog", { name: "pre-commit hook failed" });
    fireEvent.click(within(reopened).getByRole("button", { name: "Commit without hooks" }));

    await waitFor(() => expect(commit).toHaveBeenLastCalledWith("/repos/a", "feat: short", true, []));
    await waitFor(() => expect(box).toHaveValue(""));
  });

  it("retries a failed commit with the commit message even after switching to amend mode", async () => {
    const commit = vi
      .fn<RepoClient["commit"]>()
      .mockResolvedValueOnce({ kind: "HookFailed", hook: "pre-commit", output: "no\n" })
      .mockResolvedValueOnce({ kind: "Committed", commitId: "c1" });
    const client = fakeClient({
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [{ path: "a.txt", oldPath: null, staged: true, kind: "Modified" }],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
      getHeadInfo: async () => ({ headId: "h1", detachedAt: null, headPushed: false, operation: null, operationLeftover: false, squashMessage: null, commitHooks: ["pre-commit"], sequence: null, rebase: null, bisect: null, terminalSequence: null, terminalRebase: null }),
      getCommitMessage: async () => "HEAD's message",
      commit,
    });
    render(<App client={client} />);
    const box = await screen.findByRole("textbox", { name: "Commit message" });
    fireEvent.change(box, { target: { value: "feat: new work" } });
    const commitButton = screen.getByRole("button", { name: "Commit" });
    await waitFor(() => expect(commitButton).toBeEnabled());
    fireEvent.click(commitButton);
    const dialog = await screen.findByRole("dialog", { name: "pre-commit hook failed" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "pre-commit hook failed" })).toBeNull());

    fireEvent.click(screen.getByLabelText("Amend last commit"));
    await waitFor(() => expect(box).toHaveValue("HEAD's message"));
    fireEvent.click(screen.getByRole("button", { name: "Show hook output…" }));
    const reopened = await screen.findByRole("dialog", { name: "pre-commit hook failed" });
    fireEvent.click(within(reopened).getByRole("button", { name: "Commit without hooks" }));

    await waitFor(() => expect(commit).toHaveBeenLastCalledWith("/repos/a", "feat: new work", true, []));
  });

  it("amends without hooks with the author and co-authors the box holds now", async () => {
    const amendCommit = vi
      .fn<RepoClient["amendCommit"]>()
      .mockResolvedValueOnce({ kind: "HookFailed", hook: "commit-msg", output: "no\n" })
      .mockResolvedValueOnce({ kind: "Committed", commitId: "c2" });
    const client = fakeClient({
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
      listRecentAuthors: async () => [
        { name: "Ada", email: "ada@x.org" },
        { name: "Bob", email: "bob@x.org" },
      ],
      getHeadInfo: async () => ({ headId: "h1", detachedAt: null, headPushed: false, operation: null, operationLeftover: false, squashMessage: null, commitHooks: ["commit-msg"], sequence: null, rebase: null, bisect: null, terminalSequence: null, terminalRebase: null }),
      getCommitMessage: async () => "HEAD's message",
      amendCommit,
    });
    render(<App client={client} />);
    const box = await screen.findByRole("textbox", { name: "Commit message" });
    fireEvent.click(await screen.findByLabelText("Amend last commit"));
    await waitFor(() => expect(box).toHaveValue("HEAD's message"));
    const author = screen.getByRole("textbox", { name: "Author" });
    fireEvent.change(author, { target: { value: "Grace <grace@x.org>" } });
    fireEvent.click(screen.getByRole("button", { name: "Add co-author" }));
    fireEvent.click(await screen.findByRole("option", { name: "Ada <ada@x.org>" }));
    const amendButton = screen.getByRole("button", { name: "Amend" });
    await waitFor(() => expect(amendButton).toBeEnabled());
    fireEvent.click(amendButton);
    const ada = { key: "Co-authored-by", value: "Ada <ada@x.org>" };
    const dialog = await screen.findByRole("dialog", { name: "commit-msg hook failed" });
    expect(amendCommit).toHaveBeenCalledWith("/repos/a", "HEAD's message", false, {
      author: { name: "Grace", email: "grace@x.org" },
      trailers: [ada],
    });

    // Author and chips edited after the failure go into the retry, like the message.
    fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "commit-msg hook failed" })).toBeNull());
    fireEvent.change(author, { target: { value: "Hopper <hopper@x.org>" } });
    fireEvent.click(screen.getByRole("button", { name: "Add co-author" }));
    fireEvent.click(await screen.findByRole("option", { name: "Bob <bob@x.org>" }));
    fireEvent.click(screen.getByRole("button", { name: "Show hook output…" }));
    const reopened = await screen.findByRole("dialog", { name: "commit-msg hook failed" });
    fireEvent.click(within(reopened).getByRole("button", { name: "Amend without hooks" }));

    await waitFor(() =>
      expect(amendCommit).toHaveBeenLastCalledWith("/repos/a", "HEAD's message", true, {
        author: { name: "Hopper", email: "hopper@x.org" },
        trailers: [ada, { key: "Co-authored-by", value: "Bob <bob@x.org>" }],
      }),
    );
  });

  it("publishes the current branch from the sync bar through the Push dialog", async () => {
    const pushBranch = vi.fn(async () => "push-1");
    const client = fakeClient({
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [{ name: "main", isCurrent: true, upstream: null, ahead: null, behind: null }],
      listStashes: async () => [],
      listRemotes: async () => [{ name: "origin", fetchUrl: "x", pushUrl: null, authMode: null, authUsername: null, prune: false }],
      getCurrentUpstream: async () => null,
      pushBranch,
    });
    render(<App client={client} />);

    const push = await screen.findByRole("button", { name: "Push" });
    await waitFor(() => expect(push).toBeEnabled());
    fireEvent.click(push);
    const dialog = await screen.findByRole("dialog", { name: "Push main" });
    expect(within(dialog).getByLabelText("Set as upstream")).toBeChecked();
    fireEvent.click(within(dialog).getByRole("button", { name: "Push" }));

    await waitFor(() => expect(pushBranch).toHaveBeenCalledWith("/repos/a", "origin", "main", "main", true, false));
    expect(screen.queryByRole("dialog", { name: "Push main" })).toBeNull();
  });

  it("renames a branch inline with the name trimmed, and an unchanged name never reaches the client (#418)", async () => {
    localStorage.clear();
    const renameBranch = vi.fn(async () => {});
    const client = fakeClient({
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [
        { name: "main", isCurrent: true },
        { name: "topic", isCurrent: false },
      ],
      listStashes: async () => [],
      renameBranch,
    });
    render(<App client={client} />);

    fireEvent.click(await screen.findByRole("button", { name: "Actions for topic" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Rename" }));
    fireEvent.keyDown(screen.getByRole("textbox", { name: "Rename topic" }), { key: "Enter" });
    expect(screen.queryByRole("textbox", { name: "Rename topic" })).toBeNull();
    expect(renameBranch).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "Actions for topic" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Rename" }));
    const input = screen.getByRole("textbox", { name: "Rename topic" });
    fireEvent.change(input, { target: { value: " topic/old " } });
    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(renameBranch).toHaveBeenCalledWith("/repos/a", "topic", "topic/old"));
    localStorage.clear();
  });

  it("refuses Create tag with a taken name inline, without listing the tag twice (#413)", async () => {
    localStorage.clear();
    const createTag = vi.fn(async () => {});
    const client = fakeClient({
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [
        {
          id: "c1",
          shortId: "c1",
          summary: "first commit",
          authorName: "Test",
          authorEmail: "test@example.com",
          timestamp: 0,
          parentIds: [],
          branchRefs: ["main"],
          remoteBranchRefs: [],
          tags: ["v1"],
        },
      ],
      listBranches: async () => [{ name: "main", isCurrent: true }],
      listStashes: async () => [],
      listTags: async () => [
        { name: "v1", targetId: "c1", commitId: "c1", annotated: false, message: null, taggerName: null, timestamp: null },
      ],
      createTag,
    });
    const { container } = render(<App client={client} />);

    fireEvent.click(await screen.findByRole("button", { name: /^Tags/ }));
    await screen.findByLabelText("Select v1");
    // Tags are created from a commit's menu in the graph (#438).
    await waitFor(() => expect(container.querySelector('[data-commit-id="c1"]')).not.toBeNull());
    fireEvent.contextMenu(container.querySelector('[data-commit-id="c1"]')!);
    fireEvent.click(screen.getByRole("menuitem", { name: "Create tag…" }));
    const form = screen.getByRole("dialog", { name: "Create tag" });
    fireEvent.change(within(form).getByLabelText("Tag name"), { target: { value: "v1" } });
    fireEvent.click(within(form).getByRole("button", { name: "Create tag" }));

    expect(await within(form).findByText('A tag named "v1" already exists.')).toBeInTheDocument();
    expect(screen.getAllByLabelText("Select v1")).toHaveLength(1);
    expect(createTag).not.toHaveBeenCalled();
    localStorage.clear();
  });

  it("refuses Create worktree with a taken name inline, without listing the worktree twice (#413)", async () => {
    localStorage.clear();
    const createWorktree = vi.fn(async () => {});
    const client = fakeClient({
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [{ name: "main", isCurrent: true }],
      listStashes: async () => [],
      listWorktrees: async () => [
        { name: "main", path: "/repos/a", head: "main", isMain: true, isLocked: false, isPrunable: false },
        { name: "feature", path: "/repos/a-feature", head: "feature", isMain: false, isLocked: false, isPrunable: false },
      ],
      createWorktree,
    });
    render(<App client={client} />);

    fireEvent.click(await screen.findByRole("button", { name: /^Worktrees/ }));
    await screen.findByText("/repos/a-feature");
    const form = screen.getByRole("form", { name: "Create worktree" });
    fireEvent.change(within(form).getByLabelText("Worktree name"), { target: { value: "feature" } });
    fireEvent.change(within(form).getByLabelText("Worktree path"), { target: { value: "/repos/a-other" } });
    fireEvent.change(within(form).getByLabelText("Branch"), { target: { value: "other" } });
    fireEvent.click(within(form).getByRole("button", { name: "Create worktree" }));

    expect(await within(form).findByText('A worktree named "feature" already exists.')).toBeInTheDocument();
    expect(screen.queryByText("/repos/a-other")).toBeNull();
    expect(screen.getAllByText("/repos/a-feature")).toHaveLength(1);
    expect(createWorktree).not.toHaveBeenCalled();
    localStorage.clear();
  });

  it("refuses Add remote with a taken name inline, without listing the remote twice (#408)", async () => {
    const addRemote = vi.fn(async () => {});
    const client = fakeClient({
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [{ name: "main", isCurrent: true }],
      listStashes: async () => [],
      listRemotes: async () => [{ name: "origin", fetchUrl: "x", pushUrl: null, authMode: null, authUsername: null }],
      getCurrentUpstream: async () => null,
      addRemote,
    });
    render(<App client={client} />);

    await screen.findByRole("button", { name: "origin" });
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Add remote…" }));
    const form = await screen.findByRole("form", { name: "Add remote" });
    fireEvent.change(within(form).getByLabelText("Remote name"), { target: { value: "origin" } });
    fireEvent.change(within(form).getByTestId("add-remote-fetch-url"), { target: { value: "../other.git" } });
    fireEvent.click(within(form).getByRole("button", { name: "Add remote" }));

    expect(await within(form).findByText('A remote named "origin" already exists.')).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "origin" })).toHaveLength(1);
    expect(addRemote).not.toHaveBeenCalled();
    // The typed values stay for the user to correct.
    expect(within(form).getByLabelText("Remote name")).toHaveValue("origin");
  });

  it("deletes a remote branch from its menu through the confirmation dialog", async () => {
    localStorage.clear();
    const deleteRemoteBranch = vi.fn(async () => "delete-remote-1");
    const client = fakeClient({
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [{ name: "main", isCurrent: true }],
      listStashes: async () => [],
      listRemotes: async () => [{ name: "origin", fetchUrl: "x", pushUrl: null, authMode: null, authUsername: null }],
      listRemoteBranches: async () => ["topic"],
      deleteRemoteBranch,
    });
    render(<App client={client} />);

    fireEvent.click(await screen.findByRole("button", { name: "origin" }));
    fireEvent.click(await screen.findByRole("button", { name: "Actions for origin/topic" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Delete origin/topic…" }));
    const dialog = await screen.findByRole("dialog", { name: "Delete remote branch" });
    expect(dialog).toHaveTextContent("Delete origin/topic on the remote? This can't be undone.");
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete remote branch" }));

    await waitFor(() => expect(deleteRemoteBranch).toHaveBeenCalledWith("/repos/a", "origin", "topic", false));
    expect(screen.queryByRole("dialog", { name: "Delete remote branch" })).toBeNull();
    localStorage.clear();
  });

  describe("merge options", () => {
    function mergeClient(overrides: Partial<RepoClient>) {
      return fakeClient({
        listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
        openRepo: async () => {},
        persistOpenRepos: async () => {},
        getStatus: async () => [],
        getCommitGraph: async () => [],
        listBranches: async () => [
          { name: "main", isCurrent: true },
          { name: "feature", isCurrent: false },
        ],
        listStashes: async () => [],
        getMergeOptions: async () => ({ fastForward: "Allow", squash: false }),
        setMergeOptions: async () => {},
        ...overrides,
      });
    }

    async function mergeFromMenu() {
      fireEvent.contextMenu(await screen.findByRole("button", { name: "feature" }));
      fireEvent.click(screen.getByRole("menuitem", { name: "Merge into current branch…" }));
      const dialog = await screen.findByRole("dialog", { name: "Merge feature" });
      expect(within(dialog).getByRole("heading")).toHaveTextContent("Merge feature into main");
      const merge = within(dialog).getByRole("button", { name: "Merge" });
      await waitFor(() => expect(merge).toBeEnabled());
      return { dialog, merge };
    }

    it("opens the merge dialog from the branch menu and merges with the chosen options", async () => {
      const mergeBranch = vi.fn(async () => ({ kind: "Merged" as const }));
      const setMergeOptions = vi.fn(async () => {});
      render(<App client={mergeClient({ mergeBranch, setMergeOptions })} />);

      const { dialog, merge } = await mergeFromMenu();
      fireEvent.click(within(dialog).getByRole("radio", { name: "Always create a merge commit" }));
      fireEvent.click(merge);

      const options = { fastForward: "Never", squash: false };
      await waitFor(() => expect(mergeBranch).toHaveBeenCalledWith("/repos/a", "feature", options));
      expect(setMergeOptions).toHaveBeenCalledWith("/repos/a", options);
      expect(screen.queryByRole("dialog", { name: "Merge feature" })).toBeNull();
    });

    it.each([
      [{ kind: "Squashed" as const }, "Squashed feature. Review and commit."],
      [{ kind: "UpToDate" as const }, "Already up to date"],
      [{ kind: "FastForwarded" as const }, "Fast-forwarded to feature"],
    ])("shows a toast for a %o merge", async (outcome, toast) => {
      render(<App client={mergeClient({ mergeBranch: async () => outcome })} />);

      const { merge } = await mergeFromMenu();
      fireEvent.click(merge);

      expect(await screen.findByText(toast)).toBeInTheDocument();
    });
  });

  describe("cherry-pick and revert toasts", () => {
    const graph = [
      { id: "c2", shortId: "c2", summary: "two", authorName: "A", authorEmail: "a@x", timestamp: 2, parentIds: ["c1"], branchRefs: ["main"], remoteBranchRefs: [] },
      { id: "c1", shortId: "c1", summary: "one", authorName: "A", authorEmail: "a@x", timestamp: 1, parentIds: [], branchRefs: [], remoteBranchRefs: [] },
    ];
    function pickClient(overrides: Partial<RepoClient>) {
      return fakeClient({
        listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
        openRepo: async () => {},
        persistOpenRepos: async () => {},
        getStatus: async () => [],
        getCommitGraph: async () => graph,
        listBranches: async () => [{ name: "main", isCurrent: true }],
        listStashes: async () => [],
        getGraphBranchSelection: async () => null,
        getCommitFiles: async () => [],
        getCommitMessage: async () => "two",
        getCommitSignature: async () => ({ status: "unsigned", format: null, signer: null, key: null, detail: null }),
        getHeadInfo: async () => ({ headId: "c2", detachedAt: null, headPushed: false, operation: null, operationLeftover: false, squashMessage: null, commitHooks: [], sequence: null, rebase: null, bisect: null, terminalSequence: null, terminalRebase: null }),
        ...overrides,
      });
    }

    async function runPaletteCommand(query: string, option: string) {
      fireEvent.keyDown(window, { key: "k", ctrlKey: true });
      fireEvent.change(await screen.findByRole("combobox", { name: "Command palette" }), { target: { value: query } });
      fireEvent.click(await screen.findByRole("option", { name: option }));
    }

    // Ctrl+F on the graph, then a query whose results replace the graph (#507's "row not rendered").
    async function showSearchResults() {
      const graphList = await screen.findByRole("listbox", { name: "Commit history" });
      graphList.focus();
      fireEvent.keyDown(graphList, { key: "f", ctrlKey: true });
      fireEvent.change(screen.getByRole("combobox", { name: "Search commits" }), { target: { value: "one" } });
      const results = await screen.findByRole("listbox", { name: "Search results" }, { timeout: 2000 });
      expect(screen.queryByRole("listbox", { name: "Commit history" })).toBeNull();
      return results;
    }

    // #146: the dialog the palette opens returns focus to the commit list on Cancel (not to
    // whatever was focused before the palette). Since #507 the palette opens it directly.
    it("the palette's Reset to… returns focus to the commit list after the dialog closes", async () => {
      const original = Element.prototype.scrollIntoView;
      Element.prototype.scrollIntoView = () => {};
      try {
        render(<App client={pickClient({})} />);
        fireEvent.click((await screen.findByText(/c1 one/)).closest("li")!);
        screen.getByRole("tab", { name: "a" }).focus();

        await runPaletteCommand("Reset to c1", "Reset to c1…");
        const dialog = await screen.findByRole("dialog", { name: "Reset branch", hidden: true });
        fireEvent.click(within(dialog).getByRole("button", { name: "Cancel", hidden: true }));
        await waitFor(() => expect(screen.getByRole("listbox", { name: "Commit history" })).toHaveFocus());
      } finally {
        Element.prototype.scrollIntoView = original;
      }
    });

    // #507: the palette runs the menu's own action, toast included.
    it("the palette's Cherry-pick and Revert show the same toasts as the menu (#507)", async () => {
      const cherryPick = vi.fn<RepoClient["cherryPick"]>().mockResolvedValue({ kind: "Committed", commitId: "c3" });
      const revertCommit = vi.fn<RepoClient["revertCommit"]>().mockResolvedValue({ kind: "Committed", commitId: "c4" });
      render(<App client={pickClient({ cherryPick, revertCommit })} />);
      fireEvent.click((await screen.findByText(/c1 one/)).closest("li")!);

      await runPaletteCommand("Cherry-pick c1", "Cherry-pick c1");
      await waitFor(() => expect(cherryPick).toHaveBeenCalledWith("/repos/a", "c1", null));
      expect(await screen.findByText("Cherry-picked")).toBeInTheDocument();

      await runPaletteCommand("Revert c1", "Revert c1");
      await waitFor(() => expect(revertCommit).toHaveBeenCalledWith("/repos/a", "c1", null));
      expect(await screen.findByText("Reverted")).toBeInTheDocument();
    });

    // #507: the graph row isn't rendered while search results are shown; the palette must not
    // depend on it.
    it("the palette's Reset to… works while search results replace the graph (#507)", async () => {
      const original = Element.prototype.scrollIntoView;
      Element.prototype.scrollIntoView = () => {};
      try {
        const resetToCommit = vi.fn<RepoClient["resetToCommit"]>().mockResolvedValue(undefined);
        const searchCommits = vi.fn(async () => ({ commits: [graph[1]], nextCursor: null, scanned: 2 }));
        render(<App client={pickClient({ resetToCommit, searchCommits, cancelSearch: async () => {} })} />);
        const results = await showSearchResults();
        expect(await within(results).findByRole("option", { selected: true })).toHaveTextContent("one");

        await runPaletteCommand("Reset to c1", "Reset to c1…");
        const dialog = await screen.findByRole("dialog", { name: "Reset branch", hidden: true });
        fireEvent.click(within(dialog).getByRole("button", { name: "Reset", hidden: true }));
        await waitFor(() => expect(resetToCommit).toHaveBeenCalledWith("/repos/a", "c1", "mixed"));
        await waitFor(() => expect(screen.queryByRole("dialog", { name: "Reset branch", hidden: true })).toBeNull());
      } finally {
        Element.prototype.scrollIntoView = original;
      }
    });

    it("the palette's Cherry-pick of a merge commit asks for the parent while search results are shown (#507)", async () => {
      const original = Element.prototype.scrollIntoView;
      Element.prototype.scrollIntoView = () => {};
      try {
        const merge = { ...graph[1], id: "m3", shortId: "m3", summary: "merge one", timestamp: 3, parentIds: ["c2", "c1"], branchRefs: [] };
        const cherryPick = vi.fn<RepoClient["cherryPick"]>().mockResolvedValue({ kind: "Committed", commitId: "c5" });
        const searchCommits = vi.fn(async () => ({ commits: [merge], nextCursor: null, scanned: 3 }));
        render(
          <App
            client={pickClient({
              getCommitGraph: async () => [merge, ...graph],
              cherryPick,
              searchCommits,
              cancelSearch: async () => {},
            })}
          />,
        );
        const results = await showSearchResults();
        expect(await within(results).findByRole("option", { selected: true })).toHaveTextContent("merge one");

        await runPaletteCommand("Cherry-pick m3", "Cherry-pick m3…");
        const dialog = await screen.findByRole("dialog", { name: "Choose parent", hidden: true });
        fireEvent.click(within(dialog).getByRole("radio", { name: /Parent 2/, hidden: true }));
        fireEvent.click(within(dialog).getByRole("button", { name: "Cherry-pick", hidden: true }));
        await waitFor(() => expect(cherryPick).toHaveBeenCalledWith("/repos/a", "m3", 2));
        expect(await screen.findByText("Cherry-picked")).toBeInTheDocument();
      } finally {
        Element.prototype.scrollIntoView = original;
      }
    });

    // #515: a search result beyond the loaded history gets the commit actions from its own data.
    it("the palette offers and runs a search result's commit actions when the loaded graph lacks it (#515)", async () => {
      const original = Element.prototype.scrollIntoView;
      Element.prototype.scrollIntoView = () => {};
      try {
        const old = { ...graph[1], id: "c0", shortId: "c0", summary: "one older", timestamp: 0 };
        const cherryPick = vi.fn<RepoClient["cherryPick"]>().mockResolvedValue({ kind: "Committed", commitId: "c5" });
        const revertCommit = vi.fn<RepoClient["revertCommit"]>().mockResolvedValue({ kind: "Committed", commitId: "c6" });
        const resetToCommit = vi.fn<RepoClient["resetToCommit"]>().mockResolvedValue(undefined);
        const searchCommits = vi.fn(async () => ({ commits: [old], nextCursor: null, scanned: 3 }));
        render(
          <App client={pickClient({ cherryPick, revertCommit, resetToCommit, searchCommits, cancelSearch: async () => {} })} />,
        );
        const results = await showSearchResults();
        expect(await within(results).findByRole("option", { selected: true })).toHaveTextContent("one older");

        await runPaletteCommand("Cherry-pick c0", "Cherry-pick c0");
        await waitFor(() => expect(cherryPick).toHaveBeenCalledWith("/repos/a", "c0", null));
        expect(await screen.findByText("Cherry-picked")).toBeInTheDocument();

        await runPaletteCommand("Revert c0", "Revert c0");
        await waitFor(() => expect(revertCommit).toHaveBeenCalledWith("/repos/a", "c0", null));

        await runPaletteCommand("Reset to c0", "Reset to c0…");
        const dialog = await screen.findByRole("dialog", { name: "Reset branch", hidden: true });
        fireEvent.click(within(dialog).getByRole("button", { name: "Reset", hidden: true }));
        await waitFor(() => expect(resetToCommit).toHaveBeenCalledWith("/repos/a", "c0", "mixed"));
      } finally {
        Element.prototype.scrollIntoView = original;
      }
    });

    // #515: the same for a file history commit, a merge asking for its parent.
    it("the palette's Revert of a file history merge outside the loaded graph asks for the parent (#515)", async () => {
      const merge = { ...graph[1], id: "m0", shortId: "m0", summary: "merge older", timestamp: 0, parentIds: ["p1", "p2"] };
      const revertCommit = vi.fn<RepoClient["revertCommit"]>().mockResolvedValue({ kind: "Committed", commitId: "c6" });
      render(
        <App
          client={pickClient({
            getStatus: async () => [{ path: "a.txt", oldPath: null, staged: false, kind: "Modified" }],
            getFileHistory: async () => [{ commit: merge, path: "a.txt", oldPath: null }],
            revertCommit,
          })}
        />,
      );
      fireEvent.click(await screen.findByRole("button", { name: "History of a.txt" }));
      expect(await screen.findByRole("option", { selected: true })).toHaveTextContent("merge older");

      await runPaletteCommand("Revert m0", "Revert m0…");
      const dialog = await screen.findByRole("dialog", { name: "Choose parent", hidden: true });
      fireEvent.click(within(dialog).getByRole("radio", { name: /Parent 2/, hidden: true }));
      fireEvent.click(within(dialog).getByRole("button", { name: "Revert", hidden: true }));
      await waitFor(() => expect(revertCommit).toHaveBeenCalledWith("/repos/a", "m0", 2));
    });

    // #508: a dialog opened before a sequence starts (e.g. in a terminal, picked up by a refresh)
    // must not stash the paused step's working tree away.
    it("disables the stash dialog's Stash once a cherry-pick sequence starts (#508)", async () => {
      let sequence: { kind: "CherryPick"; currentStep: number; totalSteps: number; currentCommit: string } | null = null;
      const saveStash = vi.fn(async () => {});
      render(
        <App
          client={pickClient({
            getStatus: async () => [{ path: "a.txt", oldPath: null, staged: false, kind: "Modified" }],
            getHeadInfo: async () => ({ headId: "c2", detachedAt: null, headPushed: false, operation: null, operationLeftover: false, squashMessage: null, commitHooks: [], sequence, rebase: null, bisect: null, terminalSequence: null, terminalRebase: null }),
            saveStash,
          })}
        />,
      );
      await screen.findByText(/c1 one/);
      await runPaletteCommand("Stash…", "Stash…");
      const dialog = await screen.findByRole("dialog", { name: "Stash changes", hidden: true });
      expect(within(dialog).getByRole("button", { name: "Stash", hidden: true })).toBeEnabled();

      sequence = { kind: "CherryPick", currentStep: 1, totalSteps: 2, currentCommit: "c1" };
      await runPaletteCommand("Refresh", "Refresh");
      const stash = within(dialog).getByRole("button", { name: "Stash", hidden: true });
      await waitFor(() => expect(stash).toBeDisabled());
      expect(stash).toHaveAttribute("title", "A cherry-pick is in progress.");
      fireEvent.submit(stash.closest("form")!);
      await new Promise((resolve) => setTimeout(resolve, 20));
      expect(saveStash).not.toHaveBeenCalled();
    });

    // #514: the commit menu and the palette open the one dialog RepoWorkspace renders, and it
    // re-checks `operationBlockers` while open (a rebase started in a terminal, picked up by a
    // refresh).
    it("the menu's and the palette's Reset open one dialog, disabled once a terminal rebase starts (#514)", async () => {
      const original = Element.prototype.scrollIntoView;
      Element.prototype.scrollIntoView = () => {};
      try {
        let terminalRebase: { kind: "Rebase"; branch: string; onto: string; currentStep: number; totalSteps: number } | null = null;
        const resetToCommit = vi.fn<RepoClient["resetToCommit"]>().mockResolvedValue(undefined);
        render(
          <App
            client={pickClient({
              getHeadInfo: async () => ({ headId: "c2", detachedAt: null, headPushed: false, operation: null, operationLeftover: false, squashMessage: null, commitHooks: [], sequence: null, rebase: null, bisect: null, terminalSequence: null, terminalRebase }),
              resetToCommit,
            })}
          />,
        );
        const row = (await screen.findByText(/c1 one/)).closest("li")!;
        fireEvent.click(row);
        fireEvent.contextMenu(row);
        fireEvent.click(screen.getByRole("menuitem", { name: "Reset main to here…" }));
        await screen.findByRole("dialog", { name: "Reset branch", hidden: true });
        await runPaletteCommand("Reset to c1", "Reset to c1…");
        await waitFor(() => expect(screen.getAllByRole("dialog", { name: "Reset branch", hidden: true })).toHaveLength(1));
        const dialog = screen.getByRole("dialog", { name: "Reset branch", hidden: true });
        expect(within(dialog).getByRole("button", { name: "Reset", hidden: true })).toBeEnabled();

        terminalRebase = { kind: "Rebase", branch: "topic", onto: "c1", currentStep: 1, totalSteps: 2 };
        await runPaletteCommand("Refresh", "Refresh");
        const reset = within(dialog).getByRole("button", { name: "Reset", hidden: true });
        await waitFor(() => expect(reset).toBeDisabled());
        expect(reset).toHaveAttribute("title", "A rebase started in a terminal is in progress.");
        fireEvent.submit(reset.closest("form")!);
        await new Promise((resolve) => setTimeout(resolve, 20));
        expect(resetToCommit).not.toHaveBeenCalled();
      } finally {
        Element.prototype.scrollIntoView = original;
      }
    });

    it("the menu's parent picker is disabled once a cherry-pick sequence starts (#514)", async () => {
      let sequence: { kind: "CherryPick"; currentStep: number; totalSteps: number; currentCommit: string } | null = null;
      const merge = { ...graph[1], id: "m3", shortId: "m3", summary: "merge one", timestamp: 3, parentIds: ["c2", "c1"], branchRefs: [] };
      const cherryPick = vi.fn<RepoClient["cherryPick"]>().mockResolvedValue({ kind: "Committed", commitId: "c5" });
      render(
        <App
          client={pickClient({
            getCommitGraph: async () => [merge, ...graph],
            getHeadInfo: async () => ({ headId: "c2", detachedAt: null, headPushed: false, operation: null, operationLeftover: false, squashMessage: null, commitHooks: [], sequence, rebase: null, bisect: null, terminalSequence: null, terminalRebase: null }),
            cherryPick,
          })}
        />,
      );
      fireEvent.contextMenu((await screen.findByText(/m3 merge one/)).closest("li")!);
      fireEvent.click(screen.getByRole("menuitem", { name: "Cherry-pick…" }));
      const dialog = await screen.findByRole("dialog", { name: "Choose parent", hidden: true });
      expect(within(dialog).getByRole("button", { name: "Cherry-pick", hidden: true })).toBeEnabled();

      sequence = { kind: "CherryPick", currentStep: 1, totalSteps: 2, currentCommit: "c1" };
      await runPaletteCommand("Refresh", "Refresh");
      const pick = within(dialog).getByRole("button", { name: "Cherry-pick", hidden: true });
      await waitFor(() => expect(pick).toBeDisabled());
      expect(pick).toHaveAttribute("title", "A cherry-pick is in progress.");
      fireEvent.submit(pick.closest("form")!);
      await new Promise((resolve) => setTimeout(resolve, 20));
      expect(cherryPick).not.toHaveBeenCalled();
    });

    it("the force push confirmation is disabled once a terminal rebase starts (#514)", async () => {
      let terminalRebase: { kind: "Rebase"; branch: string; onto: string; currentStep: number; totalSteps: number } | null = null;
      const forcePushCurrentBranch = vi.fn<RepoClient["forcePushCurrentBranch"]>().mockResolvedValue("push-1");
      render(
        <App
          client={pickClient({
            getHeadInfo: async () => ({ headId: "c2", detachedAt: null, headPushed: false, operation: null, operationLeftover: false, squashMessage: null, commitHooks: [], sequence: null, rebase: null, bisect: null, terminalSequence: null, terminalRebase }),
            listRemotes: async () => [{ name: "origin", fetchUrl: "x", pushUrl: null, authMode: null, authUsername: null, prune: false }],
            getCurrentUpstream: async () => ({ localBranch: "main", remoteName: "origin", remoteBranch: "main", ahead: 0, behind: 0, pushBranch: "main" }),
            forcePushCurrentBranch,
          })}
        />,
      );
      await screen.findByText(/c1 one/);
      await runPaletteCommand("Force push", "Force push current branch…");
      const dialog = await screen.findByRole("dialog", { name: "Confirm force push", hidden: true });
      expect(within(dialog).getByRole("button", { name: "Force push", hidden: true })).toBeEnabled();

      terminalRebase = { kind: "Rebase", branch: "topic", onto: "c1", currentStep: 1, totalSteps: 2 };
      await runPaletteCommand("Refresh", "Refresh");
      const confirm = within(dialog).getByRole("button", { name: "Force push", hidden: true });
      await waitFor(() => expect(confirm).toBeDisabled());
      expect(confirm).toHaveAttribute("title", "A rebase started in a terminal is in progress.");
      expect(within(dialog).getByText("A rebase started in a terminal is in progress.")).toBeInTheDocument();
      fireEvent.click(confirm);
      await new Promise((resolve) => setTimeout(resolve, 20));
      expect(forcePushCurrentBranch).not.toHaveBeenCalled();
    });

    // #146: a dialog opened from the commit menu returns focus to the commit list.
    it("the menu's Reset and parent picker return focus to the commit list on Cancel (#146)", async () => {
      const merge = { ...graph[1], id: "m3", shortId: "m3", summary: "merge one", timestamp: 3, parentIds: ["c2", "c1"], branchRefs: [] };
      render(<App client={pickClient({ getCommitGraph: async () => [merge, ...graph] })} />);
      const list = await screen.findByRole("listbox", { name: "Commit history" });
      for (const [summary, item, dialogName] of [
        [/c1 one/, "Reset main to here…", "Reset branch"],
        [/m3 merge one/, "Cherry-pick…", "Choose parent"],
      ] as const) {
        screen.getByRole("tab", { name: "a" }).focus();
        fireEvent.contextMenu((await screen.findByText(summary)).closest("li")!);
        fireEvent.click(screen.getByRole("menuitem", { name: item }));
        const dialog = await screen.findByRole("dialog", { name: dialogName, hidden: true });
        fireEvent.click(within(dialog).getByRole("button", { name: "Cancel", hidden: true }));
        await waitFor(() => expect(screen.queryByRole("dialog", { name: dialogName, hidden: true })).toBeNull());
        await waitFor(() => expect(list).toHaveFocus());
      }
    });

    it("says Cherry-picked only when the pick committed (#107)", async () => {
      const cherryPick = vi
        .fn<RepoClient["cherryPick"]>()
        .mockResolvedValueOnce({ kind: "Conflicted", files: ["a.txt"] })
        .mockResolvedValueOnce({ kind: "Committed", commitId: "c3" });
      render(<App client={pickClient({ cherryPick })} />);

      fireEvent.contextMenu((await screen.findByText(/c1 one/)).closest("li")!);
      fireEvent.click(screen.getByRole("menuitem", { name: "Cherry-pick" }));
      await waitFor(() => expect(cherryPick).toHaveBeenCalledWith("/repos/a", "c1", null));
      // Give a would-be toast every chance to render before asserting there is none.
      await waitFor(() => expect(cherryPick).toHaveBeenCalledTimes(1));
      await new Promise((resolve) => setTimeout(resolve, 20));
      expect(screen.queryByText("Cherry-picked")).toBeNull();

      fireEvent.contextMenu((await screen.findByText(/c1 one/)).closest("li")!);
      fireEvent.click(screen.getByRole("menuitem", { name: "Cherry-pick" }));
      expect(await screen.findByText("Cherry-picked")).toBeInTheDocument();
    });

    it("shows a terminal bisect in the strip and keeps cherry-pick, revert and reset enabled (#250)", async () => {
      const cherryPick = vi.fn<RepoClient["cherryPick"]>().mockResolvedValue({ kind: "Committed", commitId: "c3" });
      render(
        <App
          client={pickClient({
            getHeadInfo: async () => ({ headId: "c2", detachedAt: "c2", headPushed: false, operation: null, operationLeftover: false, squashMessage: null, commitHooks: [], sequence: null, rebase: null, bisect: { startedFrom: "main" }, terminalSequence: null, terminalRebase: null }),
            cherryPick,
          })}
        />,
      );

      const strip = await screen.findByRole("status", { name: "Operation status" });
      expect(strip).toHaveTextContent("Bisecting (started from main)");
      expect(strip).toHaveTextContent("Use git bisect good, bad or reset in a terminal.");
      expect(within(strip).queryByRole("button")).toBeNull();

      fireEvent.contextMenu((await screen.findByText(/c1 one/)).closest("li")!);
      expect(screen.getByRole("menuitem", { name: "Revert" })).toBeEnabled();
      expect(screen.getByRole("menuitem", { name: /^Reset .* to here…$/ })).toBeEnabled();
      fireEvent.click(screen.getByRole("menuitem", { name: "Cherry-pick" }));
      await waitFor(() => expect(cherryPick).toHaveBeenCalledWith("/repos/a", "c1", null));
      expect(await screen.findByText("Cherry-picked")).toBeInTheDocument();
    });

    it("shows a terminal rebase in the strip, not a merge, and blocks repository actions (#263)", async () => {
      render(
        <App
          client={pickClient({
            getHeadInfo: async () => ({ headId: "c2", detachedAt: "c2", headPushed: false, operation: null, operationLeftover: false, squashMessage: null, commitHooks: [], sequence: null, rebase: null, bisect: null, terminalSequence: null, terminalRebase: { kind: "Rebase", branch: "topic", onto: "c1", currentStep: 1, totalSteps: 2 } }),
            getMergeMessage: async () => "t1",
          })}
        />,
      );

      const strip = await screen.findByRole("status", { name: "Operation status" });
      await waitFor(() => expect(strip).toHaveTextContent("Rebasing topic, step 1 of 2 (started in a terminal)"));
      expect(strip).not.toHaveTextContent("Merging");
      expect(within(strip).queryByRole("button")).toBeNull();

      fireEvent.contextMenu((await screen.findByText(/c1 one/)).closest("li")!);
      expect(screen.getByRole("menuitem", { name: "Cherry-pick" })).toBeDisabled();
      expect(screen.getByRole("menuitem", { name: "Revert" })).toBeDisabled();
    });

    // #503: between steps a terminal rebase leaves no MERGE_MSG; the palette must still agree
    // with the commit menu.
    it("blocks cherry-pick, revert and checkout in the menu and the palette during a terminal rebase (#503)", async () => {
      const original = Element.prototype.scrollIntoView;
      Element.prototype.scrollIntoView = () => {};
      try {
        render(
          <App
            client={pickClient({
              getHeadInfo: async () => ({ headId: "c2", detachedAt: "c2", headPushed: false, operation: null, operationLeftover: false, squashMessage: null, commitHooks: [], sequence: null, rebase: null, bisect: null, terminalSequence: null, terminalRebase: { kind: "Rebase", branch: "topic", onto: "c1", currentStep: 1, totalSteps: 2 } }),
              getMergeMessage: async () => null,
            })}
          />,
        );
        const strip = await screen.findByRole("status", { name: "Operation status" });
        await waitFor(() => expect(strip).toHaveTextContent("Rebasing topic, step 1 of 2 (started in a terminal)"));

        const row = (await screen.findByText(/c1 one/)).closest("li")!;
        fireEvent.click(row);
        fireEvent.contextMenu(row);
        for (const name of ["Cherry-pick", "Revert", "Checkout (detached)"]) {
          const item = screen.getByRole("menuitem", { name });
          expect(item).toBeDisabled();
          expect(item).toHaveAttribute("title", "A rebase started in a terminal is in progress.");
        }
        fireEvent.keyDown(screen.getByRole("menuitem", { name: "Cherry-pick" }), { key: "Escape" });

        fireEvent.keyDown(window, { key: "k", ctrlKey: true });
        fireEvent.change(await screen.findByRole("combobox", { name: "Command palette" }), { target: { value: "c1" } });
        const results = await screen.findByRole("listbox", { name: "Command results" });
        for (const name of ["Cherry-pick c1", "Revert c1", "Checkout c1 (detached)", "Reset to c1…"]) {
          expect(within(results).queryByRole("option", { name })).toBeNull();
        }
      } finally {
        Element.prototype.scrollIntoView = original;
      }
    });

    it("shows a stored sequence in the strip, continues it, and blocks other picks meanwhile", async () => {
      const sequence = { kind: "CherryPick" as const, currentStep: 1, totalSteps: 2, currentCommit: "c1" };
      let stored: typeof sequence | null = sequence;
      const continueSequence = vi.fn(async () => {
        stored = null;
        return { kind: "Completed" as const, applied: 2, skipped: [] };
      });
      render(
        <App
          client={pickClient({
            getHeadInfo: async () => ({ headId: "c2", detachedAt: null, headPushed: false, operation: null, operationLeftover: false, squashMessage: null, commitHooks: [], sequence: stored, rebase: null, bisect: null, terminalSequence: null, terminalRebase: null }),
            continueSequence,
          })}
        />,
      );

      const strip = await screen.findByRole("status", { name: "Operation status" });
      expect(strip).toHaveTextContent("Cherry-picking 1/2");
      fireEvent.contextMenu((await screen.findByText(/c1 one/)).closest("li")!);
      expect(screen.getByRole("menuitem", { name: "Cherry-pick" })).toBeDisabled();
      fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });

      fireEvent.click(within(strip).getByRole("button", { name: "Continue" }));
      await waitFor(() => expect(continueSequence).toHaveBeenCalledWith("/repos/a", false));
      expect(await screen.findByText("Cherry-picked 2 commits")).toBeInTheDocument();
      await waitFor(() => expect(screen.queryByRole("status", { name: "Operation status" })).toBeNull());
    });

    it("aborts through abortSequence and says when HEAD was left alone", async () => {
      const sequence = { kind: "Revert" as const, currentStep: 2, totalSteps: 2, currentCommit: "c1" };
      const abortSequence = vi.fn(async () => ({ rewound: false }));
      const abortMerge = vi.fn(async () => {});
      render(
        <App
          client={pickClient({
            getHeadInfo: async () => ({ headId: "c2", detachedAt: null, headPushed: false, operation: null, operationLeftover: false, squashMessage: null, commitHooks: [], sequence, rebase: null, bisect: null, terminalSequence: null, terminalRebase: null }),
            abortSequence,
            abortMerge,
          })}
        />,
      );
      const strip = await screen.findByRole("status", { name: "Operation status" });
      fireEvent.click(within(strip).getByRole("button", { name: "Abort revert" }));
      await waitFor(() => expect(abortSequence).toHaveBeenCalledWith("/repos/a"));
      expect(abortMerge).not.toHaveBeenCalled();
      expect(await screen.findByText("Stopped the revert. HEAD had moved, so it was left where it is.")).toBeInTheDocument();
    });

    describe("a sequence started in a terminal (#258, #253)", () => {
      const terminal = (overrides: Partial<TerminalSequence> = {}): TerminalSequence => ({
        kind: "CherryPick",
        pausedOnConflict: false,
        startedAt: "1a2b3c4d5e6f7a8b",
        startedAtSummary: "X",
        rewinds: true,
        commitsToRemove: 2,
        ...overrides,
      });
      const headWith = (terminalSequence: TerminalSequence | null, operation: OperationKind | null = null) => ({
        headId: "c2",
        detachedAt: null,
        headPushed: false,
        operation,
        operationLeftover: false,
        squashMessage: null,
        commitHooks: [],
        sequence: null,
        rebase: null,
        bisect: null,
        terminalSequence,
        terminalRebase: null,
      });

      it("asks before Abort removes the commits it made, and Cancel aborts nothing", async () => {
        let current: TerminalSequence | null = terminal();
        const abortSequence = vi.fn(async () => {
          current = null;
          return { rewound: true };
        });
        const abortMerge = vi.fn(async () => {});
        render(<App client={pickClient({ getHeadInfo: async () => headWith(current), abortSequence, abortMerge })} />);

        const strip = await screen.findByRole("status", { name: "Operation status" });
        expect(strip).toHaveTextContent("Cherry-pick paused in a terminal");
        expect(strip).toHaveTextContent("Run git cherry-pick --continue there to apply the remaining commits.");
        fireEvent.click(within(strip).getByRole("button", { name: "Abort cherry-pick" }));
        let dialog = await screen.findByRole("dialog", { name: "Abort the whole cherry-pick?" });
        expect(dialog).toHaveTextContent(
          'This also removes the 2 commits it made and puts the branch back at 1a2b3c4 "X". Uncommitted changes stay.',
        );
        fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
        await waitFor(() => expect(screen.queryByRole("dialog", { name: "Abort the whole cherry-pick?" })).toBeNull());
        expect(abortSequence).not.toHaveBeenCalled();

        fireEvent.click(within(strip).getByRole("button", { name: "Abort cherry-pick" }));
        dialog = await screen.findByRole("dialog", { name: "Abort the whole cherry-pick?" });
        fireEvent.click(within(dialog).getByRole("button", { name: "Abort cherry-pick" }));
        expect(await screen.findByText("Aborted cherry-pick")).toBeInTheDocument();
        expect(abortSequence).toHaveBeenCalledTimes(1);
        // What the dialog showed goes along, so a sequence that moved on meanwhile is refused.
        expect(abortSequence).toHaveBeenCalledWith("/repos/a", {
          headId: "c2",
          startedAt: "1a2b3c4d5e6f7a8b",
          rewinds: true,
          commitsToRemove: 2,
          pausedOnConflict: false,
        });
        expect(abortMerge).not.toHaveBeenCalled();
      });

      it("aborts with no dialog when nothing is removed", async () => {
        const abortSequence = vi.fn(async () => ({ rewound: false }));
        render(
          <App
            client={pickClient({
              getHeadInfo: async () => headWith(terminal({ kind: "Revert", rewinds: false, commitsToRemove: 0 })),
              abortSequence,
            })}
          />,
        );
        const strip = await screen.findByRole("status", { name: "Operation status" });
        fireEvent.click(within(strip).getByRole("button", { name: "Abort revert" }));
        await waitFor(() =>
          expect(abortSequence).toHaveBeenCalledWith("/repos/a", {
            headId: "c2",
            startedAt: "1a2b3c4d5e6f7a8b",
            rewinds: false,
            commitsToRemove: 0,
            pausedOnConflict: false,
          }),
        );
        expect(screen.queryByRole("dialog")).toBeNull();
        expect(await screen.findByText("Stopped the revert. HEAD had moved, so it was left where it is.")).toBeInTheDocument();
      });

      it("the commit box's Abort cherry-pick on a paused step takes the same path", async () => {
        const abortSequence = vi.fn(async () => ({ rewound: true }));
        const abortMerge = vi.fn(async () => {});
        render(
          <App
            client={pickClient({
              getStatus: async () => [{ path: "f", oldPath: null, staged: true, kind: "Modified" }],
              getCommitGraph: async () => [],
              getMergeMessage: async () => "A",
              getHeadInfo: async () => headWith(terminal({ pausedOnConflict: true, commitsToRemove: 1 }), "CherryPick"),
              abortSequence,
              abortMerge,
            })}
          />,
        );
        const strip = await screen.findByRole("status", { name: "Operation status" });
        expect(strip).toHaveTextContent("Cherry-picking (started in a terminal)");
        const buttons = await screen.findAllByRole("button", { name: "Abort cherry-pick" });
        const commitBoxAbort = buttons.find((button) => !strip.contains(button));
        expect(commitBoxAbort).toBeDefined();
        fireEvent.click(commitBoxAbort!);
        const dialog = await screen.findByRole("dialog", { name: "Abort the whole cherry-pick?" });
        expect(dialog).toHaveTextContent("This also removes the commit it made");
        fireEvent.click(within(dialog).getByRole("button", { name: "Abort cherry-pick" }));
        await waitFor(() => expect(abortSequence).toHaveBeenCalledTimes(1));
        expect(abortSequence).toHaveBeenCalledWith("/repos/a", expect.objectContaining({ pausedOnConflict: true }));
        expect(abortMerge).not.toHaveBeenCalled();
      });

      it("shows a cherry-pick's refusal while the terminal sequence is paused", async () => {
        const refusal =
          "A cherry-pick started in a terminal isn't finished. Continue it there with git cherry-pick --continue, or abort it first.";
        const cherryPick = vi.fn<RepoClient["cherryPick"]>().mockRejectedValue(refusal);
        render(<App client={pickClient({ getHeadInfo: async () => headWith(terminal()), cherryPick })} />);

        fireEvent.contextMenu((await screen.findByText(/c1 one/)).closest("li")!);
        fireEvent.click(screen.getByRole("menuitem", { name: "Cherry-pick" }));
        await waitFor(() => expect(cherryPick).toHaveBeenCalled());
        expect(await screen.findByText(refusal)).toBeInTheDocument();
      });
    });

    it("toasts \"Stopped the rebase. HEAD had moved…\" when abort did not restore", async () => {
      const rebase = { branch: "main", onto: "c1", currentStep: 1, totalSteps: 2, health: { kind: "Diverged" as const, reason: "HeadMoved" as const } };
      const abortRebase = vi.fn(async () => ({ restored: false }));
      render(
        <App
          client={pickClient({
            getHeadInfo: async () => ({ headId: "c2", detachedAt: null, headPushed: false, operation: null, operationLeftover: false, squashMessage: null, commitHooks: [], sequence: null, rebase, bisect: null, terminalSequence: null, terminalRebase: null }),
            abortRebase,
          })}
        />,
      );
      const strip = await screen.findByRole("status", { name: "Operation status" });
      expect(strip).toHaveTextContent("Rebase of main interrupted");
      fireEvent.click(within(strip).getByRole("button", { name: "Abort rebase" }));
      await waitFor(() => expect(abortRebase).toHaveBeenCalledWith("/repos/a"));
      expect(await screen.findByText("Stopped the rebase. HEAD had moved, so it was left where it is.")).toBeInTheDocument();
    });

    it("offers Switch to the branch after aborting a rebase whose branch moved", async () => {
      const rebase = { branch: "main", onto: "c1", currentStep: 1, totalSteps: 2, health: { kind: "Diverged" as const, reason: "BranchMoved" as const } };
      const abortRebase = vi.fn(async () => ({ restored: false }));
      const switchBranch = vi.fn(async () => {});
      render(
        <App
          client={pickClient({
            getHeadInfo: async () => ({ headId: "c2", detachedAt: "c2", headPushed: false, operation: null, operationLeftover: false, squashMessage: null, commitHooks: [], sequence: null, rebase, bisect: null, terminalSequence: null, terminalRebase: null }),
            abortRebase,
            switchBranch,
          })}
        />,
      );
      const strip = await screen.findByRole("status", { name: "Operation status" });
      fireEvent.click(within(strip).getByRole("button", { name: "Abort rebase" }));
      expect(await screen.findByText("Stopped the rebase. main had changed, so HEAD was left where it is.")).toBeInTheDocument();
      fireEvent.click(await screen.findByRole("button", { name: "Switch to main" }));
      await waitFor(() => expect(switchBranch).toHaveBeenCalledWith("/repos/a", "main"));
    });

    it("a paused rebase reported on the first refresh shows the strip and the dock", async () => {
      // The restart case: the backend restored the rebase from disk before the app asked.
      const rebase = { branch: "main", onto: "c1", currentStep: 1, totalSteps: 2, health: { kind: "Paused" as const } };
      const abortRebase = vi.fn(async () => ({ restored: true }));
      render(
        <App
          client={pickClient({
            getHeadInfo: async () => ({ headId: "c1", detachedAt: "c1", headPushed: false, operation: "CherryPick" as const, operationLeftover: false, squashMessage: null, commitHooks: [], sequence: null, rebase, bisect: null, terminalSequence: null, terminalRebase: null }),
            getMergeMessage: async () => "two",
            abortRebase,
          })}
        />,
      );
      const strip = await screen.findByRole("status", { name: "Operation status" });
      expect(strip).toHaveTextContent("Rebasing, step 1 of 2");
      const dock = await screen.findByRole("group", { name: "Commit actions" });
      expect(within(dock).getByRole("button", { name: "Continue rebase" })).toBeEnabled();
      fireEvent.click(within(strip).getByRole("button", { name: "Abort rebase" }));
      expect(await screen.findByText("Aborted rebase")).toBeInTheDocument();
    });

    it("toasts a finished multi-commit revert with its skipped count", async () => {
      const revertMany = vi.fn(async () => ({ kind: "Completed" as const, applied: 1, skipped: ["c1"] }));
      render(<App client={pickClient({ revertMany })} />);

      fireEvent.click((await screen.findByText(/c2 two/)).closest("li")!);
      fireEvent.click(screen.getByText(/c1 one/).closest("li")!, { ctrlKey: true });
      fireEvent.click(screen.getByRole("button", { name: "Revert 2 commits" }));

      await waitFor(() => expect(revertMany).toHaveBeenCalledWith("/repos/a", ["c1", "c2"]));
      expect(await screen.findByText("Reverted 1 commit, skipped 1 with no changes")).toBeInTheDocument();
    });
  });

  describe("stash options", () => {
    function stashClient(overrides: Partial<RepoClient>) {
      return fakeClient({
        listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
        openRepo: async () => {},
        persistOpenRepos: async () => {},
        getStatus: async () => [{ path: "a.txt", oldPath: null, staged: false, kind: "Modified" }],
        getWorkingDiff: async () => [],
        getCommitGraph: async () => [],
        listBranches: async () => [{ name: "main", isCurrent: true }],
        listStashes: async () => [],
        ...overrides,
      });
    }

    it.each([
      ["Pop", { kind: "Applied" as const }, "Stash popped"],
      ["Apply", { kind: "Applied" as const }, "Stash applied"],
      ["Pop", { kind: "Conflicted" as const, files: ["a.txt"] }, "Stash applied with conflicts; the stash was kept. Resolve the files, then drop it."],
      ["Apply", { kind: "Conflicted" as const, files: ["a.txt"] }, "Stash applied with conflicts. Resolve the files."],
    ])("%s from the Stashes panel shows its outcome toast (%o)", async (button, outcome, toast) => {
      localStorage.clear();
      const popStash = vi.fn(async () => outcome);
      const applyStash = vi.fn(async () => outcome);
      render(
        <App
          client={stashClient({
            listStashes: async () => [{ index: 0, message: "On main: wip", commitId: "s0" }],
            popStash,
            applyStash,
          })}
        />,
      );

      fireEvent.click(await screen.findByRole("button", { name: "Stashes" }));
      const row = (await screen.findByText("On main: wip")).closest("li")!;
      fireEvent.click(within(row).getByRole("button", { name: "Actions for On main: wip" }));
      fireEvent.click(screen.getByRole("menuitem", { name: button }));

      expect(await screen.findByText(toast)).toBeInTheDocument();
      expect(button === "Pop" ? popStash : applyStash).toHaveBeenCalledWith("/repos/a", 0, "s0", true);
      localStorage.clear();
    });

    it("disables stash Apply and Pop while something is staged", async () => {
      localStorage.clear();
      render(
        <App
          client={stashClient({
            getStatus: async () => [{ path: "a.txt", oldPath: null, staged: true, kind: "Modified" }],
            listStashes: async () => [{ index: 0, message: "On main: wip", commitId: "s0" }],
          })}
        />,
      );

      fireEvent.click(await screen.findByRole("button", { name: "Stashes" }));
      const row = (await screen.findByText("On main: wip")).closest("li")!;
      fireEvent.click(within(row).getByRole("button", { name: "Actions for On main: wip" }));
      await waitFor(() => expect(screen.getByRole("menuitem", { name: "Pop" })).toBeDisabled());
      expect(screen.getByRole("menuitem", { name: "Apply" })).toHaveAttribute(
        "title",
        "Unstage or commit staged changes first.",
      );
      localStorage.clear();
    });

    it("stashes an untracked file without offering to leave untracked files out", async () => {
      const saveStash = vi.fn(async () => {});
      render(
        <App
          client={stashClient({ saveStash, getStatus: async () => [{ path: "new.txt", oldPath: null, staged: false, kind: "New" }] })}
        />,
      );

      fireEvent.click(await screen.findByRole("button", { name: "Actions for new.txt" }));
      fireEvent.click(screen.getByRole("menuitem", { name: "Stash file…" }));
      const dialog = await screen.findByRole("dialog", { name: "Stash changes" });
      expect(within(dialog).queryByRole("checkbox", { name: "Include untracked files" })).toBeNull();
      fireEvent.click(within(dialog).getByRole("button", { name: "Stash" }));

      await waitFor(() =>
        expect(saveStash).toHaveBeenCalledWith("/repos/a", {
          message: null,
          keepIndex: false,
          includeUntracked: true,
          paths: ["new.txt"],
        }),
      );
    });

    it("stashes everything from the Stash button", async () => {
      const saveStash = vi.fn(async () => {});
      render(<App client={stashClient({ saveStash })} />);

      const stash = await screen.findByRole("button", { name: "Stash" });
      await waitFor(() => expect(stash).toBeEnabled());
      fireEvent.click(stash);

      await waitFor(() =>
        expect(saveStash).toHaveBeenCalledWith("/repos/a", {
          message: null,
          keepIndex: false,
          includeUntracked: true,
          paths: [],
        }),
      );
    });

    it("stashes one file with a message through the stash dialog", async () => {
      const saveStash = vi.fn(async () => {});
      render(<App client={stashClient({ saveStash })} />);

      fireEvent.click(await screen.findByRole("button", { name: "Actions for a.txt" }));
      fireEvent.click(screen.getByRole("menuitem", { name: "Stash file…" }));
      const dialog = await screen.findByRole("dialog", { name: "Stash changes" });
      expect(dialog).toHaveTextContent("Only: a.txt");
      fireEvent.change(within(dialog).getByRole("textbox", { name: "Stash message" }), { target: { value: "half" } });
      fireEvent.click(within(dialog).getByRole("button", { name: "Stash" }));

      await waitFor(() =>
        expect(saveStash).toHaveBeenCalledWith("/repos/a", {
          message: "half",
          keepIndex: false,
          includeUntracked: true,
          paths: ["a.txt"],
        }),
      );
      expect(screen.queryByRole("dialog", { name: "Stash changes" })).toBeNull();
    });
  });

  it("opens a working-tree file's history in place of the graph and closes it again", async () => {
    const graphCommit = {
      id: "c2",
      shortId: "c2",
      summary: "edit a",
      authorName: "A",
      authorEmail: "a@example.com",
      timestamp: 1_700_000_000,
      parentIds: [],
      branchRefs: [],
      remoteBranchRefs: [],
    };
    const getFileHistory = vi.fn(async () => [{ commit: graphCommit, path: "a.txt", oldPath: null }]);
    const client = fakeClient({
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [{ path: "a.txt", oldPath: null, staged: false, kind: "Modified" }],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
      getFileHistory,
      getCommitFiles: async () => ["a.txt"],
      getCommitMessage: async () => "edit a",
    });

    render(<App client={client} />);
    fireEvent.click(await screen.findByRole("button", { name: "History of a.txt" }));

    expect(await screen.findByRole("toolbar", { name: "File history" })).toHaveTextContent("History: a.txt");
    expect(screen.queryByRole("listbox", { name: "Commit history" })).toBeNull();
    expect(getFileHistory).toHaveBeenCalledWith("/repos/a", "a.txt", 300);
    expect(await screen.findByRole("option", { selected: true })).toHaveTextContent("edit a");

    fireEvent.click(screen.getByRole("button", { name: "Close file history" }));
    expect(await screen.findByRole("listbox", { name: "Commit history" })).toBeInTheDocument();
  });

  it("searches from the graph with Ctrl+F, replaces the graph with results, and × restores it", async () => {
    const hit = {
      id: "s1",
      shortId: "s1",
      summary: "search hit",
      authorName: "Searchy McAuthor",
      authorEmail: "searchy@example.com",
      timestamp: 1_700_000_000,
      parentIds: [],
      branchRefs: [],
      remoteBranchRefs: [],
    };
    const searchCommits = vi.fn(async () => ({ commits: [hit], nextCursor: null, scanned: 1 }));
    const cancelSearch = vi.fn(async () => {});
    const client = fakeClient({
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
      getGraphBranchSelection: async () => ["main"],
      getCommitFiles: async () => [],
      getCommitMessage: async () => "search hit",
      getCommitSignature: async () => ({ status: "unsigned", format: null, signer: null, key: null, detail: null }),
      searchCommits,
      cancelSearch,
    });

    render(<App client={client} />);
    const graph = await screen.findByRole("listbox", { name: "Commit history" });
    graph.focus();
    fireEvent.keyDown(graph, { key: "f", ctrlKey: true });
    const input = screen.getByRole("combobox", { name: "Search commits" });
    await waitFor(() => expect(input).toHaveFocus());

    fireEvent.change(input, { target: { value: "author:searchy" } });
    const results = await screen.findByRole("listbox", { name: "Search results" }, { timeout: 2000 });
    await waitFor(() => expect(searchCommits).toHaveBeenCalledWith("/repos/a", "author:searchy", { local: ["main"], remote: [] }, null));
    expect(screen.queryByRole("listbox", { name: "Commit history" })).toBeNull();
    expect(await within(results).findByRole("option", { selected: true })).toHaveTextContent("search hit");

    fireEvent.click(screen.getByRole("button", { name: "Close search" }));
    expect(await screen.findByRole("listbox", { name: "Commit history" })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Search commits" })).toHaveValue("");
  });

  it("compares two selected commits in place of the diff pane, and × restores the commit view", async () => {
    const base = { authorName: "Rene", authorEmail: "rene@example.com", timestamp: 1_700_000_000, branchRefs: [], remoteBranchRefs: [], tags: [] };
    const newer = { ...base, id: "n".repeat(40), shortId: "nnnnnnn", summary: "newer commit", parentIds: ["o".repeat(40)] };
    const older = { ...base, id: "o".repeat(40), shortId: "ooooooo", summary: "older commit", parentIds: [] };
    const getRangeFiles = vi.fn(async () => [{ path: "a.txt", oldPath: null }]);
    const client = fakeClient({
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [newer, older],
      listBranches: async () => [],
      listStashes: async () => [],
      getGraphBranchSelection: async () => null,
      getCommitFiles: async () => [],
      getCommitMessage: async () => "older commit",
      getCommitSignature: async () => ({ status: "unsigned", format: null, signer: null, key: null, detail: null }),
      getRangeFiles,
      getRangeDiff: async () => [],
    });

    render(<App client={client} />);
    const graph = await screen.findByRole("listbox", { name: "Commit history" });
    fireEvent.click(await within(graph).findByText(/newer commit/));
    fireEvent.click(within(graph).getByText(/older commit/), { ctrlKey: true });
    fireEvent.click(screen.getByRole("button", { name: "Compare these commits" }));

    const comparison = await screen.findByRole("region", { name: "Comparison" });
    expect(within(comparison).getByRole("heading")).toHaveTextContent("Comparing ooooooo..nnnnnnn");
    await waitFor(() => expect(getRangeFiles).toHaveBeenCalledWith("/repos/a", "o".repeat(40), "n".repeat(40)));

    fireEvent.click(screen.getByRole("button", { name: "Close compare" }));
    expect(screen.queryByRole("region", { name: "Comparison" })).toBeNull();
  });

  describe("compare closes on any row click", () => {
    const base = { authorName: "Rene", authorEmail: "rene@example.com", timestamp: 1_700_000_000, branchRefs: [], remoteBranchRefs: [], tags: [] };
    const newer = { ...base, id: "n".repeat(40), shortId: "nnnnnnn", summary: "newer commit", parentIds: ["o".repeat(40)] };
    const older = { ...base, id: "o".repeat(40), shortId: "ooooooo", summary: "older commit", parentIds: [] };
    const stash = { index: 0, message: "wip stash", commitId: "s".repeat(40) };
    function compareClient() {
      return fakeClient({
        listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
        openRepo: async () => {},
        persistOpenRepos: async () => {},
        getStatus: async () => [],
        getCommitGraph: async () => [newer, older],
        listBranches: async () => [],
        listStashes: async () => [stash],
        getStashFiles: async () => ({ tracked: [], untracked: [], untrackedCommitId: null }),
        getGraphBranchSelection: async () => null,
        getCommitFiles: async () => [],
        getCommitMessage: async () => "commit",
        getCommitSignature: async () => ({ status: "unsigned", format: null, signer: null, key: null, detail: null }),
        getRangeFiles: async () => [],
        getWorkingTreeCompareFiles: async () => [],
      });
    }

    it("clicking the already-selected commit closes the compare", async () => {
      render(<App client={compareClient()} />);
      const graph = await screen.findByRole("listbox", { name: "Commit history" });
      fireEvent.click(await within(graph).findByText(/newer commit/));
      fireEvent.click(within(graph).getByText(/older commit/), { ctrlKey: true });
      fireEvent.click(screen.getByRole("button", { name: "Compare these commits" }));
      await screen.findByRole("region", { name: "Comparison" });

      // `older commit` is the focused row after the ctrl-click; clicking it again keeps the
      // same selected row but must still close the compare.
      fireEvent.click(within(graph).getByText(/older commit/));
      expect(screen.queryByRole("region", { name: "Comparison" })).toBeNull();
    });

    it("clicking the already-selected stash closes the compare and shows the stash", async () => {
      render(<App client={compareClient()} />);
      const graph = await screen.findByRole("listbox", { name: "Commit history" });
      await within(graph).findByText(/newer commit/);
      const stashesToggle = screen.getByRole("button", { name: /^Stashes/ });
      if (stashesToggle.getAttribute("aria-expanded") !== "true") fireEvent.click(stashesToggle);
      const stashRow = await screen.findByText("wip stash");
      fireEvent.click(stashRow);
      await screen.findByRole("region", { name: "Stash details" });

      fireEvent.contextMenu(within(graph).getByText(/older commit/));
      fireEvent.click(screen.getByRole("menuitem", { name: "Diff against working tree" }));
      await screen.findByRole("region", { name: "Comparison" });
      expect(screen.queryByRole("region", { name: "Stash details" })).toBeNull();

      fireEvent.click(screen.getByText("wip stash"));
      expect(screen.queryByRole("region", { name: "Comparison" })).toBeNull();
      expect(await screen.findByRole("region", { name: "Stash details" })).toBeInTheDocument();
    });
  });

  describe("undo/redo", () => {
    function undoClient(overrides: Partial<RepoClient> = {}) {
      return fakeClient({
        listOpenRepos: async () => ({ entries: [{ path: "/repo", workspaceId: null }], activePath: "/repo" }),
        openRepo: async () => {},
        persistOpenRepos: async () => {},
        getStatus: async () => [],
        getCommitGraph: async () => [],
        listBranches: async () => [],
        listStashes: async () => [],
        ...overrides,
      });
    }

    it("undoes with Mod+Z and offers Redo in the toast", async () => {
      const undo = vi.fn(async () => ({ kind: "Done" as const, label: "Stage a.txt" }));
      const redo = vi.fn(async () => ({ kind: "Done" as const, label: "Stage a.txt" }));
      render(<App client={undoClient({ getUndoState: async () => ({ undo: "Stage a.txt", redo: null }), undo, redo })} />);
      await screen.findByRole("button", { name: "Undo Stage a.txt" });
      fireEvent.keyDown(window, { key: "z", ctrlKey: true });
      await waitFor(() => expect(undo).toHaveBeenCalled());
      expect(undo).toHaveBeenCalledWith("/repo");
      fireEvent.click(await screen.findByRole("button", { name: "Redo" }));
      expect(redo).toHaveBeenCalledWith("/repo");
      expect(await screen.findByText("Redid Stage a.txt")).toBeInTheDocument();
    });

    it("redoes with Mod+Shift+Z and Ctrl+Y", async () => {
      const redo = vi.fn(async () => ({ kind: "Nothing" as const }));
      render(<App client={undoClient({ getUndoState: async () => ({ undo: null, redo: "Stage a.txt" }), redo })} />);
      await screen.findByRole("button", { name: "Redo Stage a.txt" });
      fireEvent.keyDown(window, { key: "Z", ctrlKey: true, shiftKey: true });
      await waitFor(() => expect(redo).toHaveBeenCalledTimes(1));
      // A second press while the first is still settling is ignored (pending), so wait it out.
      await waitFor(() => {
        fireEvent.keyDown(window, { key: "y", ctrlKey: true });
        expect(redo).toHaveBeenCalledTimes(2);
      });
    });

    it("ignores Mod+Z while typing in the commit message", async () => {
      const undo = vi.fn(async () => ({ kind: "Nothing" as const }));
      render(<App client={undoClient({ getUndoState: async () => ({ undo: "Stage a.txt", redo: null }), undo })} />);
      await screen.findByRole("button", { name: "Undo Stage a.txt" });
      const message = await screen.findByRole("textbox", { name: /commit message/i });
      message.focus();
      expect(message).toHaveFocus();
      fireEvent.keyDown(message, { key: "z", ctrlKey: true });
      expect(undo).not.toHaveBeenCalled();
    });

    it("a toast's Redo does nothing once its repo tab was switched away", async () => {
      const undo = vi.fn(async () => ({ kind: "Done" as const, label: "Stage a.txt" }));
      const redo = vi.fn(async () => ({ kind: "Done" as const, label: "Stage a.txt" }));
      render(
        <App
          client={undoClient({
            listOpenRepos: async () => ({
              entries: [
                { path: "/repos/a", workspaceId: null },
                { path: "/repos/b", workspaceId: null },
              ],
              activePath: "/repos/a",
            }),
            getUndoState: async () => ({ undo: "Stage a.txt", redo: null }),
            undo,
            redo,
          })}
        />,
      );
      await screen.findByRole("button", { name: "Undo Stage a.txt" });
      fireEvent.keyDown(window, { key: "z", ctrlKey: true });
      const toastRedo = await screen.findByRole("button", { name: "Redo" });
      fireEvent.click(screen.getByRole("tab", { name: "b" }));
      await waitFor(() => expect(screen.getByRole("tab", { name: "b" })).toHaveAttribute("aria-selected", "true"));
      fireEvent.click(toastRedo);
      await new Promise((resolve) => setTimeout(resolve, 0));
      expect(redo).not.toHaveBeenCalled();
    });

    it("re-reads the undo state when the window regains focus", async () => {
      const getUndoState = vi.fn(async () => ({ undo: null, redo: null }));
      render(<App client={undoClient({ getUndoState })} />);
      await screen.findByRole("button", { name: "Nothing to undo" });
      const calls = getUndoState.mock.calls.length;
      window.dispatchEvent(new Event("focus"));
      await waitFor(() => expect(getUndoState.mock.calls.length).toBe(calls + 1));
    });
  });
});

describe("App external tools", () => {
  function toolsClient(overrides: Partial<RepoClient> = {}) {
    return fakeClient({
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      listStashes: async () => [],
      ...overrides,
    });
  }

  it("the header gear opens Preferences with the active repo's git tools", async () => {
    const getExternalToolSettings = vi.fn(async () => ({ editor: "code", terminal: null }));
    const getGitTools = vi.fn(async () => ({ diffTool: null, mergeTool: null }));
    render(<App client={toolsClient({ getExternalToolSettings, getGitTools })} />);
    await screen.findByRole("tab", { name: "a" });

    fireEvent.click(screen.getByRole("button", { name: "Preferences" }));
    expect(screen.getByRole("dialog", { name: "Preferences" })).toBeInTheDocument();
    await waitFor(() => expect(screen.getByLabelText("Editor command")).toHaveValue("code"));
    expect(getExternalToolSettings).toHaveBeenCalled();
    expect(getGitTools).toHaveBeenCalledWith("/repos/a");
    expect(screen.getByRole("group", { name: "Commit message" })).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("spinbutton", { name: "Ruler column" })).toHaveValue(72));

    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog", { name: "Preferences" })).toBeNull();
  });

  // #177: an error stays until it is dismissed or the next change starts; a refresh keeps it.
  it("the error banner stays through the palette's Refresh and clears when Stage starts", async () => {
    let releaseStage: () => void = () => {};
    const stageFile = vi
      .fn<RepoClient["stageFile"]>()
      .mockRejectedValueOnce(new Error("boom"))
      .mockImplementationOnce(
        () =>
          new Promise<void>((resolve) => {
            releaseStage = resolve;
          }),
      );
    const getStatus = vi.fn(async () => [{ path: "a.txt", oldPath: null, staged: false, kind: "Modified" as const }]);
    render(
      <App
        client={fakeClient({
          listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
          openRepo: async () => {},
          persistOpenRepos: async () => {},
          getStatus,
          getCommitGraph: async () => [],
          listBranches: async () => [],
          listStashes: async () => [],
          stageFile,
        })}
      />,
    );

    fireEvent.click(await screen.findByRole("button", { name: "Stage a.txt" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("boom");

    const reads = getStatus.mock.calls.length;
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    fireEvent.change(await screen.findByRole("combobox", { name: "Command palette" }), { target: { value: "Refresh" } });
    fireEvent.click(await screen.findByRole("option", { name: "Refresh" }));
    await waitFor(() => expect(getStatus.mock.calls.length).toBeGreaterThan(reads));
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 20));
    });
    expect(screen.getByRole("alert")).toHaveTextContent("boom");

    fireEvent.click(await screen.findByRole("button", { name: "Stage a.txt" }));
    await waitFor(() => expect(stageFile).toHaveBeenCalledTimes(2));
    expect(screen.queryByRole("alert")).toBeNull();
    await act(async () => releaseStage());
  });

  // #573: a failed mutation's banner gets the hint `describeError` gives its kind.
  describe("the error banner after a failed Stage", () => {
    const PERMISSION_HINT = "File permissions blocked this. Check that you can read and write the file or folder.";

    async function stageAndFail(stageFile: RepoClient["stageFile"]) {
      render(
        <App
          client={fakeClient({
            listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
            openRepo: async () => {},
            persistOpenRepos: async () => {},
            getStatus: async () => [{ path: "a.txt", oldPath: null, staged: false, kind: "Modified" as const }],
            getCommitGraph: async () => [],
            listBranches: async () => [],
            listStashes: async () => [],
            stageFile,
          })}
        />,
      );
      fireEvent.click(await screen.findByRole("button", { name: "Stage a.txt" }));
      return screen.findByRole("alert");
    }

    it("shows the PermissionDenied hint under the backend's message, until dismissed", async () => {
      const alert = await stageAndFail(
        vi.fn().mockRejectedValue(new RepoError("PermissionDenied", "failed to open 'a.txt': Permission denied")),
      );
      expect(alert).toHaveTextContent("failed to open 'a.txt': Permission denied");
      expect(alert).toHaveTextContent(PERMISSION_HINT);

      fireEvent.click(within(alert).getByRole("button", { name: "Dismiss error" }));
      expect(screen.queryByRole("alert")).toBeNull();
    });

    it("shows the credential store remediation for CredentialStoreFailure", async () => {
      const alert = await stageAndFail(vi.fn().mockRejectedValue(new RepoError("CredentialStoreFailure", "keyring locked")));
      expect(alert).toHaveTextContent(credentialRemediation("CredentialStoreFailure"));
      expect(alert).not.toHaveTextContent("keyring locked");
    });

    it("shows a plain error's message alone", async () => {
      const alert = await stageAndFail(vi.fn().mockRejectedValue(new Error("boom")));
      expect(alert.textContent).toBe("boom");
    });

    it("drops the hint when the next failure has none", async () => {
      const stageFile = vi
        .fn<RepoClient["stageFile"]>()
        .mockRejectedValueOnce(new RepoError("PermissionDenied", "Permission denied"))
        .mockRejectedValueOnce(new Error("boom"));
      expect(await stageAndFail(stageFile)).toHaveTextContent(PERMISSION_HINT);

      fireEvent.click(screen.getByRole("button", { name: "Stage a.txt" }));
      await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("boom"));
      expect(screen.getByRole("alert")).not.toHaveTextContent(PERMISSION_HINT);
    });
  });

  it("the palette opens Preferences…", async () => {
    const { container } = render(<App client={toolsClient()} />);
    await screen.findByRole("tab", { name: "a" });
    // The palette shortcut lives in the active RepoWorkspace, which mounts after the tabs (#135).
    await waitFor(() => expect(container.querySelector('[data-active-repo="true"]')).not.toBeNull());
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    const input = await screen.findByRole("combobox", { name: "Command palette" });
    fireEvent.change(input, { target: { value: "Preferences" } });
    fireEvent.click(await screen.findByRole("option", { name: "Preferences…" }));
    expect(await screen.findByRole("dialog", { name: "Preferences" })).toBeInTheDocument();
  });

  async function openPalette(container: HTMLElement) {
    await screen.findByRole("tab", { name: "a" });
    await waitFor(() => expect(container.querySelector('[data-active-repo="true"]')).not.toBeNull());
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    return screen.findByRole("combobox", { name: "Command palette" });
  }

  it("the palette's Check for updates runs the updater's check and reports it (#630)", async () => {
    const check = vi.fn<UpdateCheck>().mockResolvedValueOnce({ kind: "none" }).mockResolvedValueOnce({ kind: "failed", message: "offline" });
    const { container } = render(<App client={toolsClient()} updateBanner={<FakeUpdater check={check} />} />);
    let input = await openPalette(container);
    fireEvent.change(input, { target: { value: "Check for updates" } });
    fireEvent.click(await screen.findByRole("option", { name: "Check for updates" }));
    expect(check).toHaveBeenCalledTimes(1);
    expect(await screen.findByText("Browsitory is up to date")).toBeInTheDocument();

    input = await openPalette(container);
    fireEvent.change(input, { target: { value: "Check for updates" } });
    fireEvent.click(await screen.findByRole("option", { name: "Check for updates" }));
    expect(await screen.findByText("Couldn't check for updates: offline")).toBeInTheDocument();
  });

  it("Preferences has an Updates section with the updater only (#630)", async () => {
    const check = vi.fn<UpdateCheck>().mockResolvedValue({ kind: "none" });
    const getUpdateSettings = vi.fn(async () => ({ checkAutomatically: false }));
    const view = render(<App client={toolsClient({ getUpdateSettings })} />);
    await screen.findByRole("tab", { name: "a" });
    fireEvent.click(screen.getByRole("button", { name: "Preferences" }));
    expect(screen.queryByRole("group", { name: "Updates" })).toBeNull();
    expect(getUpdateSettings).not.toHaveBeenCalled();
    view.unmount();

    render(<App client={toolsClient({ getUpdateSettings })} updateBanner={<FakeUpdater check={check} />} />);
    await screen.findByRole("tab", { name: "a" });
    fireEvent.click(screen.getByRole("button", { name: "Preferences" }));
    const group = screen.getByRole("group", { name: "Updates" });
    await waitFor(() =>
      expect(within(group).getByRole("checkbox", { name: "Check for updates automatically" })).not.toBeChecked(),
    );
    fireEvent.click(within(group).getByRole("button", { name: "Check for updates" }));
    expect(check).toHaveBeenCalledTimes(1);
    expect(await within(group).findByText("Browsitory is up to date.")).toBeInTheDocument();
  });

  it("offers no Check for updates without the updater (VS Code)", async () => {
    const { container } = render(<App client={toolsClient()} />);
    const input = await openPalette(container);
    fireEvent.change(input, { target: { value: "Check for updates" } });
    await waitFor(() => expect(screen.queryByRole("option", { name: "Check for updates" })).toBeNull());
  });

  it("the palette and the release notes open the third-party licences", async () => {
    const loadThirdPartyNotices = vi.fn(async () => "libgit2 licence text");
    const { container } = render(<App client={toolsClient()} loadThirdPartyNotices={loadThirdPartyNotices} />);
    await screen.findByRole("tab", { name: "a" });
    await waitFor(() => expect(container.querySelector('[data-active-repo="true"]')).not.toBeNull());
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    const input = await screen.findByRole("combobox", { name: "Command palette" });
    fireEvent.change(input, { target: { value: "licen" } });
    fireEvent.click(await screen.findByRole("option", { name: "Third-party licences" }));
    expect(await screen.findByRole("region", { name: "Licence texts" })).toHaveTextContent("libgit2 licence text");

    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("region", { name: "Licence texts" })).toBeNull());

    fireEvent.click(screen.getByRole("button", { name: "Release notes" }));
    fireEvent.click(await screen.findByRole("button", { name: "Third-party licences" }));
    expect(await screen.findByRole("region", { name: "Licence texts" })).toBeInTheDocument();
    // The licences replace the release notes instead of stacking a second dialog on top.
    expect(screen.queryByRole("region", { name: "Release notes" })).toBeNull();
  });

  it("the palette opens Edit remote while the Branches section is collapsed", async () => {
    localStorage.setItem("sidebar-branches", "closed");
    try {
      const updateRemoteUrls = vi.fn(async () => {});
      const { container } = render(
        <App
          client={toolsClient({
            listRemotes: async () => [
              { name: "origin", fetchUrl: "https://example.com/r.git", pushUrl: null, authMode: null, authUsername: null },
            ],
            updateRemoteUrls,
          })}
        />,
      );
      await screen.findByRole("tab", { name: "a" });
      await waitFor(() => expect(container.querySelector('[data-active-repo="true"]')).not.toBeNull());
      expect(screen.queryByRole("button", { name: "origin" })).toBeNull();
      fireEvent.keyDown(window, { key: "k", ctrlKey: true });
      const input = await screen.findByRole("combobox", { name: "Command palette" });
      fireEvent.change(input, { target: { value: "Edit remote origin" } });
      fireEvent.click(await screen.findByRole("option", { name: "Edit remote origin…" }));
      const dialog = await screen.findByRole("dialog", { name: "Edit origin", hidden: true });
      expect(within(dialog).getByLabelText("Fetch URL")).toHaveValue("https://example.com/r.git");
      fireEvent.click(within(dialog).getByRole("button", { name: "Cancel", hidden: true }));
      await waitFor(() => expect(screen.queryByRole("dialog", { name: "Edit origin", hidden: true })).toBeNull());
      expect(updateRemoteUrls).not.toHaveBeenCalled();
    } finally {
      localStorage.removeItem("sidebar-branches");
    }
  });

  it.each([
    ["Edit remote origin…", "Edit origin"],
    ["Manage credentials for origin…", "Credentials for origin"],
    ["Remove remote origin…", "Remove remote confirmation"],
  ])("the palette's %s dialog is visible while the sidebar is collapsed (narrow window)", async (option, name) => {
    const originalInnerWidth = window.innerWidth;
    Object.defineProperty(window, "innerWidth", { writable: true, configurable: true, value: 800 });
    try {
      const { container } = render(
        <App
          client={toolsClient({
            listRemotes: async () => [
              { name: "origin", fetchUrl: "https://example.com/r.git", pushUrl: null, authMode: null, authUsername: null },
            ],
          })}
        />,
      );
      await screen.findByRole("tab", { name: "a" });
      await waitFor(() => expect(container.querySelector('[data-active-repo="true"]')).not.toBeNull());
      const sidebarPane = screen.getByRole("separator", { name: "Sidebar width", hidden: true }).previousElementSibling;
      await waitFor(() => expect(sidebarPane).toHaveAttribute("hidden"));
      fireEvent.keyDown(window, { key: "k", ctrlKey: true });
      const input = await screen.findByRole("combobox", { name: "Command palette" });
      fireEvent.change(input, { target: { value: option.replace("…", "") } });
      fireEvent.click(await screen.findByRole("option", { name: option }));
      const dialog = await screen.findByRole("dialog", { name, hidden: true });
      // `showModal()` inside a `display: none` ancestor is a 0×0 modal that makes the page inert.
      expect(dialog.closest("[hidden]")).toBeNull();
    } finally {
      Object.defineProperty(window, "innerWidth", { writable: true, configurable: true, value: originalInnerWidth });
    }
  });

  it("closes an open Edit remote dialog with a notice when a refresh no longer lists its remote", async () => {
    let remotes = [{ name: "origin", fetchUrl: "https://example.com/r.git", pushUrl: null, authMode: null, authUsername: null }];
    const { container } = render(<App client={toolsClient({ listRemotes: async () => remotes })} />);
    await screen.findByRole("tab", { name: "a" });
    await waitFor(() => expect(container.querySelector('[data-active-repo="true"]')).not.toBeNull());
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    fireEvent.change(await screen.findByRole("combobox", { name: "Command palette" }), {
      target: { value: "Edit remote origin" },
    });
    fireEvent.click(await screen.findByRole("option", { name: "Edit remote origin…" }));
    await screen.findByRole("dialog", { name: "Edit origin", hidden: true });
    remotes = [];
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    fireEvent.change(await screen.findByRole("combobox", { name: "Command palette", hidden: true }), {
      target: { value: "Refresh" },
    });
    fireEvent.click(await screen.findByRole("option", { name: "Refresh", hidden: true }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Edit origin", hidden: true })).toBeNull());
    expect(await screen.findByText("Remote origin was removed or renamed.")).toBeInTheDocument();
  });

  it("shows only the VSCode note in VSCode", async () => {
    render(<App client={toolsClient()} toolHost="vscode" />);
    await screen.findByRole("tab", { name: "a" });
    fireEvent.click(screen.getByRole("button", { name: "Preferences" }));
    expect(screen.getByText("In VSCode, files, diffs, merges and terminals open in VSCode itself.")).toBeInTheDocument();
    expect(screen.queryByLabelText("Editor command")).toBeNull();
  });

  it("has no theme toggle or theme setting in VSCode, where the editor's theme decides (#676)", async () => {
    render(<App client={toolsClient()} toolHost="vscode" />);
    await screen.findByRole("tab", { name: "a" });
    expect(screen.queryByRole("button", { name: /Switch to (light|dark) theme/ })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Preferences" }));
    expect(screen.queryByRole("combobox", { name: "Theme" })).toBeNull();
  });

  it("switches the theme from the header and goes back to System in Preferences (#691)", async () => {
    localStorage.removeItem("browsitory-theme");
    try {
      render(<App client={toolsClient()} />);
      await screen.findByRole("tab", { name: "a" });
      fireEvent.click(screen.getByRole("button", { name: "Switch to dark theme" }));
      expect(document.documentElement.dataset.theme).toBe("dark");
      fireEvent.click(screen.getByRole("button", { name: "Preferences" }));
      const theme = screen.getByRole("combobox", { name: "Theme" });
      expect(theme).toHaveValue("dark");
      fireEvent.change(theme, { target: { value: "system" } });
      expect(document.documentElement.dataset.theme).toBeUndefined();
      expect(localStorage.getItem("browsitory-theme")).toBeNull();
    } finally {
      localStorage.removeItem("browsitory-theme");
      delete document.documentElement.dataset.theme;
    }
  });

  it("the repo tab menu opens a terminal in that repo and reveals it", async () => {
    const openTerminal = vi.fn(async () => {});
    const revealInFileManager = vi.fn(async () => {});
    render(<App client={toolsClient({ openTerminal, revealInFileManager })} />);
    fireEvent.contextMenu(await screen.findByRole("tab", { name: "a" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Open terminal here" }));
    expect(openTerminal).toHaveBeenCalledWith("/repos/a");
    fireEvent.contextMenu(screen.getByRole("tab", { name: "a" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Reveal repository" }));
    expect(revealInFileManager).toHaveBeenCalledWith("/repos/a", null);
  });

  it("the repo tab menu opens .gitattributes and creates a missing .gitignore before opening it (#624)", async () => {
    let existing: RootFileName[] = [".gitattributes"];
    const getRootFiles = vi.fn(async () => existing);
    const createRootFile = vi.fn(async () => {
      existing = [".gitignore", ".gitattributes"];
    });
    const openInEditor = vi.fn(async () => {});
    render(<App client={toolsClient({ getRootFiles, createRootFile, openInEditor })} />);
    fireEvent.contextMenu(await screen.findByRole("tab", { name: "a" }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Open .gitattributes" }));
    await waitFor(() => expect(openInEditor).toHaveBeenCalledWith("/repos/a", ".gitattributes", null));
    expect(createRootFile).not.toHaveBeenCalled();

    fireEvent.contextMenu(screen.getByRole("tab", { name: "a" }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Create .gitignore" }));
    await waitFor(() => expect(openInEditor).toHaveBeenLastCalledWith("/repos/a", ".gitignore", null));
    expect(createRootFile).toHaveBeenCalledWith("/repos/a", ".gitignore");
    expect(createRootFile.mock.invocationCallOrder[0]).toBeLessThan(openInEditor.mock.invocationCallOrder[1]);

    fireEvent.contextMenu(screen.getByRole("tab", { name: "a" }));
    expect(await screen.findByRole("menuitem", { name: "Open .gitignore" })).toBeInTheDocument();
  });

  it("the tab menu shows an editor failure inline and offers no root files for a bare repository (#624)", async () => {
    const openInEditor = vi.fn(async () => {
      throw new Error("No editor is configured.");
    });
    const { unmount } = render(<App client={toolsClient({ getRootFiles: async () => [".gitignore"], openInEditor })} />);
    fireEvent.contextMenu(await screen.findByRole("tab", { name: "a" }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Open .gitignore" }));
    expect(await screen.findByText("No editor is configured.")).toBeInTheDocument();
    unmount();

    render(<App client={toolsClient({ getRootFiles: async () => null })} />);
    fireEvent.contextMenu(await screen.findByRole("tab", { name: "a" }));
    await screen.findByRole("menuitem", { name: "Open terminal here" });
    expect(screen.queryByRole("menuitem", { name: /\.git(ignore|attributes)/ })).toBeNull();
  });

  it("the palette creates a missing .gitattributes and opens it (#624)", async () => {
    const createRootFile = vi.fn(async () => {});
    const openInEditor = vi.fn(async () => {});
    const { container } = render(
      <App client={toolsClient({ getRootFiles: async () => [".gitignore"], createRootFile, openInEditor })} />,
    );
    await screen.findByRole("tab", { name: "a" });
    await waitFor(() => expect(container.querySelector('[data-active-repo="true"]')).not.toBeNull());
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    fireEvent.change(await screen.findByRole("combobox", { name: "Command palette" }), {
      target: { value: "gitattributes" },
    });
    fireEvent.click(await screen.findByRole("option", { name: "Create .gitattributes" }));
    await waitFor(() => expect(openInEditor).toHaveBeenCalledWith("/repos/a", ".gitattributes", null));
    expect(createRootFile).toHaveBeenCalledWith("/repos/a", ".gitattributes");
  });

  it("a failed tab action shows its error", async () => {
    const openTerminal = vi.fn(async () => {
      throw new Error("No terminal found. Set a terminal command in Preferences.");
    });
    render(<App client={toolsClient({ openTerminal })} />);
    fireEvent.contextMenu(await screen.findByRole("tab", { name: "a" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Open terminal here" }));
    expect(await screen.findByText("No terminal found. Set a terminal command in Preferences.")).toBeInTheDocument();
  });

  it("the palette opens a terminal for the workspace and shows a tool failure in it", async () => {
    const openTerminal = vi.fn(async () => {
      throw new Error("Couldn't find the program 'kitty'.");
    });
    const { container } = render(<App client={toolsClient({ openTerminal })} />);
    await screen.findByRole("tab", { name: "a" });
    // The palette shortcut lives in the active RepoWorkspace, which mounts after the tabs (#135).
    await waitFor(() => expect(container.querySelector('[data-active-repo="true"]')).not.toBeNull());
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    const input = await screen.findByRole("combobox", { name: "Command palette" });
    fireEvent.change(input, { target: { value: "Open terminal here" } });
    fireEvent.click(await screen.findByRole("option", { name: "Open terminal here" }));
    expect(await screen.findByText("Couldn't find the program 'kitty'.")).toBeInTheDocument();
    expect(openTerminal).toHaveBeenCalledWith("/repos/a");
  });

  it("the tab menu renames a tab, an empty name restores the folder name, and a failure shows (#625)", async () => {
    const setRepoAlias = vi.fn(async () => {});
    render(<App client={toolsClient({ setRepoAlias })} />);
    fireEvent.contextMenu(await screen.findByRole("tab", { name: "a" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Rename tab…" }));
    const dialog = screen.getByRole("dialog", { name: "Rename tab" });
    const field = within(dialog).getByLabelText("Tab name");
    expect(field).toHaveValue("");
    expect(field).toHaveAttribute("placeholder", "a");
    fireEvent.change(field, { target: { value: "alpha" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Rename" }));

    const renamed = await screen.findByRole("tab", { name: "alpha" });
    expect(setRepoAlias).toHaveBeenCalledWith("/repos/a", "alpha");
    expect(renamed).toHaveAttribute("title", "/repos/a");
    expect(screen.queryByRole("dialog", { name: "Rename tab" })).toBeNull();

    // F2 on the tab opens the dialog with the current name.
    fireEvent.keyDown(renamed, { key: "F2" });
    const again = within(screen.getByRole("dialog", { name: "Rename tab" })).getByLabelText("Tab name");
    expect(again).toHaveValue("alpha");
    fireEvent.change(again, { target: { value: "" } });
    fireEvent.submit(again);
    expect(await screen.findByRole("tab", { name: "a" })).toBeInTheDocument();
    expect(setRepoAlias).toHaveBeenLastCalledWith("/repos/a", null);

    setRepoAlias.mockRejectedValueOnce(new Error("config.toml is read-only"));
    fireEvent.contextMenu(screen.getByRole("tab", { name: "a" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Rename tab…" }));
    const failing = within(screen.getByRole("dialog", { name: "Rename tab" })).getByLabelText("Tab name");
    fireEvent.change(failing, { target: { value: "beta" } });
    fireEvent.submit(failing);
    expect(await screen.findByText("Couldn't save the tab name: config.toml is read-only")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "a" })).toBeInTheDocument();
  });

  it("the palette's Rename tab… renames the active tab (#625)", async () => {
    const setRepoAlias = vi.fn(async () => {});
    const { container } = render(<App client={toolsClient({ setRepoAlias })} />);
    await screen.findByRole("tab", { name: "a" });
    await waitFor(() => expect(container.querySelector('[data-active-repo="true"]')).not.toBeNull());
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    fireEvent.change(await screen.findByRole("combobox", { name: "Command palette" }), { target: { value: "Rename tab" } });
    fireEvent.click(await screen.findByRole("option", { name: "Rename tab…" }));
    const field = within(await screen.findByRole("dialog", { name: "Rename tab" })).getByLabelText("Tab name");
    fireEvent.change(field, { target: { value: "alpha" } });
    fireEvent.submit(field);
    expect(await screen.findByRole("tab", { name: "alpha" })).toBeInTheDocument();
    expect(setRepoAlias).toHaveBeenCalledWith("/repos/a", "alpha");
  });
});

describe("App release notes on a new version", () => {
  const notes = (version: string, text: string): ReleaseNotesEntry => ({
    version,
    date: "2026-01-01",
    groups: [{ title: "Topic", blocks: [{ items: [{ text }] }] }],
  });
  const internalOnly = (version: string): ReleaseNotesEntry => ({ version, date: "2026-01-01", groups: [] });

  async function withReleaseNotes(entries: ReleaseNotesEntry[], run: () => Promise<void>) {
    const saved = releaseNotes.splice(0, releaseNotes.length, ...entries);
    try {
      await run();
    } finally {
      releaseNotes.splice(0, releaseNotes.length, ...saved);
    }
  }

  it("shows every release after a last-seen release that had nothing for users", async () => {
    await withReleaseNotes(
      [
        notes("0.5.0", "Notes for 0.5.0"),
        notes("0.4.0", "Notes for 0.4.0"),
        internalOnly("0.3.5"),
        notes("0.3.0", "Notes for 0.3.0"),
      ],
      async () => {
        render(
          <App client={fakeClient({ getAppVersion: async () => "0.5.0", getLastSeenVersion: async () => "0.3.5" })} />,
        );
        const dialog = await screen.findByRole("dialog", { name: "Release notes" });
        expect(within(dialog).getByText("Notes for 0.5.0")).toBeInTheDocument();
        expect(within(dialog).getByText("Notes for 0.4.0")).toBeInTheDocument();
        expect(within(dialog).queryByText("Notes for 0.3.0")).toBeNull();
      },
    );
  });

  it("on a fresh install shows the newest release that has notes", async () => {
    await withReleaseNotes(
      [internalOnly("0.5.0"), notes("0.4.0", "Notes for 0.4.0"), notes("0.3.0", "Notes for 0.3.0")],
      async () => {
        render(<App client={fakeClient({ getAppVersion: async () => "0.5.0", getLastSeenVersion: async () => null })} />);
        const dialog = await screen.findByRole("dialog", { name: "Release notes" });
        expect(within(dialog).getByText("Notes for 0.4.0")).toBeInTheDocument();
        expect(within(dialog).queryByText("Notes for 0.3.0")).toBeNull();
      },
    );
  });

  it("records the new version without opening when the new releases have nothing for users", async () => {
    const setLastSeenVersion = vi.fn(async () => {});
    await withReleaseNotes([internalOnly("0.3.5"), notes("0.3.0", "Notes for 0.3.0")], async () => {
      render(
        <App
          client={fakeClient({
            getAppVersion: async () => "0.3.5",
            getLastSeenVersion: async () => "0.3.0",
            setLastSeenVersion,
          })}
        />,
      );
      await waitFor(() => expect(setLastSeenVersion).toHaveBeenCalledWith("0.3.5"));
      expect(screen.queryByRole("dialog", { name: "Release notes" })).toBeNull();
    });
  });
});

describe("App back and forward through the selected commits (#618)", () => {
  const graph = [
    { id: "c3", shortId: "c3", summary: "three", authorName: "A", authorEmail: "a@x", timestamp: 3, parentIds: ["c2"], branchRefs: ["main"], remoteBranchRefs: [] },
    { id: "c2", shortId: "c2", summary: "two", authorName: "A", authorEmail: "a@x", timestamp: 2, parentIds: ["c1"], branchRefs: [], remoteBranchRefs: [] },
    { id: "c1", shortId: "c1", summary: "one", authorName: "A", authorEmail: "a@x", timestamp: 1, parentIds: [], branchRefs: [], remoteBranchRefs: [] },
  ];
  function navClient() {
    return fakeClient({
      listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
      openRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => graph,
      listBranches: async () => [{ name: "main", isCurrent: true }],
      listStashes: async () => [],
      getGraphBranchSelection: async () => null,
      getCommitFiles: async () => [],
      getCommitMessage: async () => "message",
      getCommitSignature: async () => ({ status: "unsigned", format: null, signer: null, key: null, detail: null }),
    });
  }
  const row = (summary: RegExp) => screen.getByText(summary).closest("li")!;
  const isSelected = (summary: RegExp) => row(summary).getAttribute("aria-selected") === "true";

  it("goes back and forward with Alt+Left/Right, the mouse buttons and the toolbar", async () => {
    const original = Element.prototype.scrollIntoView;
    Element.prototype.scrollIntoView = () => {};
    try {
      render(<App client={navClient()} />);
      fireEvent.click((await screen.findByText(/c3 three/)).closest("li")!);
      const list = screen.getByRole("listbox", { name: "Commit history" });
      list.focus();
      // `[` twice: two jumps to the parents, each its own entry.
      fireEvent.keyDown(list, { key: "[" });
      fireEvent.keyDown(list, { key: "[" });
      await waitFor(() => expect(isSelected(/c1 one/)).toBe(true));
      expect(screen.getByRole("button", { name: "Forward" })).toBeDisabled();

      fireEvent.keyDown(list, { key: "ArrowLeft", altKey: true });
      await waitFor(() => expect(isSelected(/c2 two/)).toBe(true));
      fireEvent.mouseUp(window, { button: 3 });
      await waitFor(() => expect(isSelected(/c3 three/)).toBe(true));
      fireEvent.click(screen.getByRole("button", { name: "Back" }));
      await waitFor(() => expect(screen.getByText(/Uncommitted changes/).closest("li")).toHaveAttribute("aria-selected", "true"));
      expect(screen.getByRole("button", { name: "Back" })).toBeDisabled();

      fireEvent.mouseUp(window, { button: 4 });
      await waitFor(() => expect(isSelected(/c3 three/)).toBe(true));
      fireEvent.keyDown(window, { key: "ArrowRight", altKey: true });
      await waitFor(() => expect(isSelected(/c2 two/)).toBe(true));
      fireEvent.click(screen.getByRole("button", { name: "Forward" }));
      await waitFor(() => expect(isSelected(/c1 one/)).toBe(true));
    } finally {
      Element.prototype.scrollIntoView = original;
    }
  });

  it("merges keyboard moves into one step, and the palette offers Back", async () => {
    const original = Element.prototype.scrollIntoView;
    Element.prototype.scrollIntoView = () => {};
    try {
      render(<App client={navClient()} />);
      fireEvent.click((await screen.findByText(/c3 three/)).closest("li")!);
      const list = screen.getByRole("listbox", { name: "Commit history" });
      fireEvent.keyDown(list, { key: "ArrowDown" });
      fireEvent.keyDown(list, { key: "ArrowDown" });
      await waitFor(() => expect(isSelected(/c1 one/)).toBe(true));

      fireEvent.keyDown(window, { key: "k", ctrlKey: true });
      fireEvent.change(await screen.findByRole("combobox", { name: "Command palette" }), { target: { value: "Back to" } });
      fireEvent.click(await screen.findByRole("option", { name: "Back to previous commit" }));
      await waitFor(() => expect(isSelected(/c3 three/)).toBe(true));
    } finally {
      Element.prototype.scrollIntoView = original;
    }
  });

  it("leaves Alt+Left to a text field", async () => {
    render(<App client={navClient()} />);
    fireEvent.click((await screen.findByText(/c3 three/)).closest("li")!);
    fireEvent.click(screen.getByText(/c2 two/).closest("li")!);
    const search = screen.getByRole("combobox", { name: "Search commits" });
    fireEvent.keyDown(search, { key: "ArrowLeft", altKey: true });
    expect(isSelected(/c2 two/)).toBe(true);
  });
});

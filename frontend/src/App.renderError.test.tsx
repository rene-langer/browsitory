import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import type { RepoClient } from "./ipc/RepoClient";
import { fakeRepoClient, graphPage } from "./test/fakeRepoClient";

// A render error in one view (#846): which part of the app throws is switched per test.
const failing = vi.hoisted(() => ({ graph: false, help: false }));
vi.mock("./components/CommitGraph", async (importOriginal) => {
  const original = await importOriginal<typeof import("./components/CommitGraph")>();
  return {
    ...original,
    CommitGraph: (props: Parameters<typeof original.CommitGraph>[0]) => {
      if (failing.graph) throw new Error("graph exploded");
      return <original.CommitGraph {...props} />;
    },
  };
});
vi.mock("./components/HelpMenuButton", async (importOriginal) => {
  const original = await importOriginal<typeof import("./components/HelpMenuButton")>();
  return {
    ...original,
    HelpMenuButton: (props: Parameters<typeof original.HelpMenuButton>[0]) => {
      if (failing.help) throw new Error("header exploded");
      return <original.HelpMenuButton {...props} />;
    },
  };
});

vi.setConfig({ testTimeout: 20_000 });

beforeEach(() => {
  failing.graph = false;
  failing.help = false;
  // React reports every caught render error on the console; keep the test output readable.
  vi.spyOn(console, "error").mockImplementation(() => {});
});
afterEach(() => vi.restoreAllMocks());

function client(overrides: Partial<RepoClient> = {}): RepoClient {
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
      closeRepo: async () => {},
      persistOpenRepos: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => graphPage([]),
      listBranches: async () => [],
      listStashes: async () => [],
      logFrontendError: vi.fn(async () => {}),
      ...overrides,
    },
    "empty",
  );
}

describe("App render errors (#846)", () => {
  it("a render error in one repository tab shows that tab's fallback; the rest keeps working", async () => {
    failing.graph = true;
    const logFrontendError = vi.fn(async (_context: string, _error: unknown) => {});
    render(<App client={client({ logFrontendError })} />);

    const alert = await screen.findByRole("alert", { name: "Couldn't show a" });
    expect(within(alert).getByText("graph exploded")).toBeInTheDocument();
    expect(within(alert).getByText(/Reload the tab to read the repository again\./)).toBeInTheDocument();
    // The fallback is the tab's panel, so the tab still controls it.
    const panel = alert.closest('[role="tabpanel"]');
    expect(panel).not.toBeNull();
    expect(screen.getByRole("tab", { name: "a" })).toHaveAttribute("aria-controls", panel!.id);
    // The header and the tabs are still there.
    expect(screen.getByRole("button", { name: "Help" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "b" })).toBeInTheDocument();
    await waitFor(() =>
      expect(logFrontendError).toHaveBeenCalledWith("Render error in repository tab", expect.stringContaining("graph exploded")),
    );
    expect(logFrontendError).toHaveBeenCalledWith("Render error in repository tab", expect.stringContaining("Component stack:"));
    expect(logFrontendError.mock.calls.filter(([context]) => context === "Render error in repository tab")).toHaveLength(1);

    // Another tab works.
    failing.graph = false;
    fireEvent.click(screen.getByRole("tab", { name: "b" }));
    await waitFor(() => expect(screen.getByRole("tab", { name: "b" })).toHaveAttribute("aria-selected", "true"));
    await waitFor(() => expect(document.querySelector('[data-active-repo="true"]')).not.toBeNull());
    expect(screen.queryByRole("alert", { name: "Couldn't show a" })).toBeNull();
  });

  it("Reload tab mounts the tab's workspace again", async () => {
    failing.graph = true;
    const getStatus = vi.fn(async (_path: string) => []);
    render(<App client={client({ getStatus })} />);
    const alert = await screen.findByRole("alert", { name: "Couldn't show a" });
    failing.graph = false;
    const readsBefore = getStatus.mock.calls.filter(([path]) => path === "/repos/a").length;
    fireEvent.click(within(alert).getByRole("button", { name: "Reload tab" }));
    await waitFor(() => expect(document.querySelector('[data-active-repo="true"]')).not.toBeNull());
    expect(screen.queryByRole("alert", { name: "Couldn't show a" })).toBeNull();
    await waitFor(() =>
      expect(getStatus.mock.calls.filter(([path]) => path === "/repos/a").length).toBeGreaterThan(readsBefore),
    );
  });

  it("Close tab closes the tab that failed", async () => {
    failing.graph = true;
    render(<App client={client()} />);
    const alert = await screen.findByRole("alert", { name: "Couldn't show a" });
    failing.graph = false;
    fireEvent.click(within(alert).getByRole("button", { name: "Close tab" }));
    await waitFor(() => expect(screen.queryByRole("tab", { name: "a" })).toBeNull());
    expect(screen.getByRole("tab", { name: "b" })).toBeInTheDocument();
  });

  it("a render error outside the tabs shows the window's fallback, and Reload brings the app back", async () => {
    failing.help = true;
    const logFrontendError = vi.fn(async (_context: string, _error: unknown) => {});
    render(<App client={client({ logFrontendError })} />);
    const alert = await screen.findByRole("alert", { name: "Browsitory couldn't show this window" });
    expect(within(alert).getByText("header exploded")).toBeInTheDocument();
    expect(within(alert).getByText(/Reload to open your repositories again\./)).toBeInTheDocument();
    expect(within(alert).getAllByRole("button").map((button) => button.textContent)).toEqual(["Reload", "Copy details"]);
    await waitFor(() => expect(logFrontendError).toHaveBeenCalledWith("Render error", expect.stringContaining("header exploded")));

    failing.help = false;
    fireEvent.click(within(alert).getByRole("button", { name: "Reload" }));
    expect(await screen.findByRole("tab", { name: "a" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Help" })).toBeInTheDocument();
    expect(screen.queryByRole("alert", { name: "Browsitory couldn't show this window" })).toBeNull();
  });
});

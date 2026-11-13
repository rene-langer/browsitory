import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { OperationLogChange, OperationLogEntry, OperationLogPage, RepoClient } from "../ipc/RepoClient";
import { fakeRepoClient } from "../test/fakeRepoClient";
import { OperationLogDialog } from "./OperationLogDialog";

function entry(id: number, name: string, overrides: Partial<OperationLogEntry> = {}): OperationLogEntry {
  return {
    id,
    name,
    target: null,
    params: [],
    startedAt: Date.UTC(2026, 10, 13, 12, 0, id),
    durationMs: 1500,
    outcome: "Succeeded",
    message: null,
    errorKind: null,
    transferError: null,
    remoteFailures: [],
    runs: [],
    rerun: null,
    ...overrides,
  };
}

function page(entries: OperationLogEntry[], hasMore = false): OperationLogPage {
  return { repo: "/key/repo", entries, hasMore };
}

/** A client whose log answers `pages` in turn (the last one again after that). */
function client(pages: OperationLogPage[], overrides: Partial<RepoClient> = {}) {
  let listener: ((change: OperationLogChange) => void) | null = null;
  const remaining = [...pages];
  const listOperations = vi.fn(async () => (remaining.length > 1 ? remaining.shift()! : remaining[0]));
  const unsubscribe = vi.fn();
  const repoClient = fakeRepoClient({
    listOperations,
    subscribeOperationLog: (next) => {
      listener = next;
      return unsubscribe;
    },
    ...overrides,
  });
  const emit = (change: OperationLogChange) => act(() => listener?.(change));
  return { repoClient, listOperations, emit, unsubscribe };
}

function renderDialog(repoClient: RepoClient, focus: "latest" | "latestFailed" = "latest") {
  const onClose = vi.fn();
  const view = render(<OperationLogDialog client={repoClient} repoPath="/repo" focus={focus} onClose={onClose} />);
  return { ...view, onClose };
}

function rows() {
  return within(screen.getByRole("list", { name: "Operations" }))
    .getAllByRole("button")
    .map((button) => button.textContent);
}

describe("OperationLogDialog (#889)", () => {
  it("lists the operations newest first with their name, target, result and duration", async () => {
    const { repoClient, listOperations } = client([
      page([
        entry(3, "Push", { target: "origin", outcome: "Failed", message: "rejected" }),
        entry(2, "Commit", { target: "Add a file", durationMs: 250 }),
        entry(1, "Fetch", { target: "origin", outcome: "Running", durationMs: null }),
      ]),
    ]);
    renderDialog(repoClient);

    expect(await screen.findByRole("dialog", { name: "Operation log" })).toBeInTheDocument();
    expect(listOperations).toHaveBeenCalledWith("/repo", null, 100);
    const list = await screen.findByRole("list", { name: "Operations" });
    const buttons = within(list).getAllByRole("button");
    expect(buttons).toHaveLength(3);
    expect(buttons[0]).toHaveAccessibleName(/^Push origin, Failed/);
    expect(buttons[0]).toHaveTextContent("1.5 s");
    expect(buttons[1]).toHaveAccessibleName(/^Commit Add a file, Succeeded/);
    expect(buttons[1]).toHaveTextContent("250 ms");
    expect(buttons[2]).toHaveAccessibleName(/^Fetch origin, Running/);
  });

  it("says so when nothing was logged yet", async () => {
    const { repoClient } = client([page([])]);
    renderDialog(repoClient);

    expect(await screen.findByText("No operations yet.")).toBeInTheDocument();
  });

  it("shows the selected operation's parameters, error and program output, which can be copied", async () => {
    const writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
    const { repoClient } = client([
      page([
        entry(2, "Push", {
          target: "origin",
          outcome: "Failed",
          message: "The pre-push hook failed.",
          transferError: "HookFailed",
          params: [{ name: "Skip hooks", value: "no" }],
          runs: [
            {
              program: "/repo/.git/hooks/pre-push",
              args: ["origin", "https://***@example.test/r.git"],
              output: "tests failed\n",
              status: "Failed",
            },
          ],
        }),
        entry(1, "Commit", { target: "First" }),
      ]),
    ]);
    renderDialog(repoClient);

    fireEvent.click(await screen.findByRole("button", { name: /^Push origin/ }));

    const details = screen.getByRole("region", { name: "Operation details" });
    expect(within(details).getByRole("heading", { name: "Push origin" })).toBeInTheDocument();
    expect(details).toHaveTextContent("Failed");
    expect(details).toHaveTextContent("The pre-push hook failed.");
    expect(details).toHaveTextContent("Skip hooks");
    expect(details).toHaveTextContent("no");
    expect(details).toHaveTextContent("/repo/.git/hooks/pre-push origin https://***@example.test/r.git");
    expect(within(details).getByLabelText("Output of pre-push")).toHaveTextContent("tests failed");

    fireEvent.click(within(details).getByRole("button", { name: "Copy output of pre-push" }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("tests failed\n"));
  });

  it("opens on the newest failed operation when asked to show an error's details", async () => {
    const { repoClient } = client([
      page([
        entry(3, "Stage", { target: "a.txt" }),
        entry(2, "Fetch", { target: "origin", outcome: "Failed", message: "could not resolve host" }),
        entry(1, "Merge", { outcome: "Failed", message: "older failure" }),
      ]),
    ]);
    renderDialog(repoClient, "latestFailed");

    const details = await screen.findByRole("region", { name: "Operation details" });
    await waitFor(() => expect(within(details).getByRole("heading", { name: "Fetch origin" })).toBeInTheDocument());
    expect(details).toHaveTextContent("could not resolve host");
  });

  it("filters to failed operations and hides automatic fetches", async () => {
    const { repoClient } = client([
      page([
        entry(4, "Fetch all (automatic)", { outcome: "Failed" }),
        entry(3, "Push", { outcome: "Failed" }),
        entry(2, "Fetch all (automatic)"),
        entry(1, "Commit", { outcome: "Cancelled" }),
      ]),
    ]);
    renderDialog(repoClient);
    await screen.findByRole("list", { name: "Operations" });
    expect(rows()).toHaveLength(4);

    fireEvent.click(screen.getByRole("checkbox", { name: "Failed only" }));
    expect(rows()).toHaveLength(2);
    expect(rows()[0]).toContain("Fetch all (automatic)");
    expect(rows()[1]).toContain("Push");

    fireEvent.click(screen.getByRole("checkbox", { name: "Hide automatic fetches" }));
    expect(rows()).toHaveLength(1);
    expect(rows()[0]).toContain("Push");

    fireEvent.click(screen.getByRole("checkbox", { name: "Failed only" }));
    expect(rows()).toHaveLength(2);
    expect(rows()[1]).toContain("Commit");
  });

  it("updates live when this repository's log changes, and ignores other repositories", async () => {
    const { repoClient, listOperations, emit } = client([
      page([entry(1, "Commit")]),
      page([entry(2, "Push", { outcome: "Running", durationMs: null }), entry(1, "Commit")]),
      page([entry(2, "Push", { outcome: "Succeeded" }), entry(1, "Commit")]),
    ]);
    renderDialog(repoClient);
    await screen.findByRole("button", { name: /^Commit/ });

    emit({ repo: "/key/other", entryId: 9 });
    expect(listOperations).toHaveBeenCalledTimes(1);

    emit({ repo: "/key/repo", entryId: 2 });
    expect(await screen.findByRole("button", { name: /^Push, Running/ })).toBeInTheDocument();

    emit({ repo: "/key/repo", entryId: 2 });
    expect(await screen.findByRole("button", { name: /^Push, Succeeded/ })).toBeInTheDocument();
    expect(rows()).toHaveLength(2);
  });

  it("loads older operations before the oldest one shown", async () => {
    const listOperations = vi
      .fn<RepoClient["listOperations"]>()
      .mockResolvedValueOnce(page([entry(5, "Stage"), entry(4, "Stage")], true))
      .mockResolvedValueOnce(page([entry(3, "Commit")], false));
    const { repoClient } = client([], { listOperations });
    renderDialog(repoClient);
    await screen.findByRole("list", { name: "Operations" });

    fireEvent.click(screen.getByRole("button", { name: "Load older operations" }));

    await waitFor(() => expect(rows()).toHaveLength(3));
    expect(listOperations).toHaveBeenLastCalledWith("/repo", 4, 100);
    expect(screen.queryByRole("button", { name: "Load older operations" })).toBeNull();
  });

  it("closes and stops listening", async () => {
    const { repoClient, unsubscribe } = client([page([])]);
    const { onClose, unmount } = renderDialog(repoClient);
    await screen.findByText("No operations yet.");

    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalled();
    unmount();
    expect(unsubscribe).toHaveBeenCalled();
  });
});

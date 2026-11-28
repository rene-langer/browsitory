import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useState } from "react";
import { DEFAULT_DIFF_SETTINGS, type DiffHunk, type RepoClient, type StatusEntry } from "../ipc/RepoClient";
import { DiffSettingsContext } from "../state/useDiffSettings";
import { WorkspaceActionsContext, type WorkspaceActions } from "../state/useWorkspaceActions";
import { testWorkspaceActions } from "../test/workspaceActions";
import { fakeRepoClient } from "../test/fakeRepoClient";
import { DiffPane, type DiscardRequest } from "./DiffPane";

// #793: the working tree's optional stacked view: every shown file's diff one below the other,
// the file list above as a jump index, each diff loaded once it comes near the view.

const REPO = "/repo";

const hunk = (content: string): DiffHunk => ({
  oldStart: 1,
  oldLines: 1,
  newStart: 1,
  newLines: 1,
  lines: [{ origin: "Add", content }],
});

// Each file's diff says whose it is, so a test can tell which file a section shows.
const diffNamingFile = async (_repo: string, path: string, staged: boolean) => [
  hunk(`${staged ? "staged" : "unstaged"} ${path}`),
];

function Pane({
  client,
  status,
  actions = {},
  refreshGeneration = 0,
}: {
  client: RepoClient;
  status: StatusEntry[];
  actions?: Partial<WorkspaceActions>;
  refreshGeneration?: number;
}) {
  const [discardRequest, setDiscardRequest] = useState<DiscardRequest | null>(null);
  return (
    <DiffSettingsContext.Provider
      value={{ settings: DEFAULT_DIFF_SETTINGS, update: () => {}, error: null, dismissError: () => {} }}
    >
      <WorkspaceActionsContext.Provider value={testWorkspaceActions(actions)}>
        <DiffPane
          repoPath={REPO}
          client={client}
          selectedRow="uncommitted"
          status={status}
          mergeMessage={null}
          rebaseProgress={null}
          discardRequest={discardRequest}
          onDiscardRequestChange={setDiscardRequest}
          refreshGeneration={refreshGeneration}
        />
      </WorkspaceActionsContext.Provider>
    </DiffSettingsContext.Provider>
  );
}

const entry = (path: string, staged: boolean, kind: StatusEntry["kind"] = "Modified"): StatusEntry => ({
  path,
  oldPath: null,
  staged,
  kind,
});

const status = [entry("a.txt", false), entry("b.txt", false), entry("c.txt", true, "New")];

// An IntersectionObserver the test drives: nothing is near the view until `setNear` says so.
interface Observed {
  callback: IntersectionObserverCallback;
  targets: Set<Element>;
}
let observers: Observed[] = [];
class ControlledObserver {
  private readonly observed: Observed;
  readonly root = null;
  readonly rootMargin = "";
  readonly scrollMargin = "";
  readonly thresholds: ReadonlyArray<number> = [];
  constructor(callback: IntersectionObserverCallback) {
    this.observed = { callback, targets: new Set() };
    observers.push(this.observed);
  }
  observe(target: Element) {
    this.observed.targets.add(target);
  }
  unobserve(target: Element) {
    this.observed.targets.delete(target);
  }
  disconnect() {
    this.observed.targets.clear();
  }
  takeRecords(): IntersectionObserverEntry[] {
    return [];
  }
}

/** Reports every observed element in `section` (or `section` itself) as near the view or not. */
function setNear(section: HTMLElement, near: boolean) {
  act(() => {
    for (const observer of observers) {
      for (const target of observer.targets) {
        if (target !== section && !section.contains(target)) continue;
        observer.callback(
          [{ isIntersecting: near, target } as IntersectionObserverEntry],
          {} as IntersectionObserver,
        );
      }
    }
  });
}

const section = (name: string) => screen.getByRole("region", { name: `Changes in ${name}` });
const sectionNames = () =>
  Array.from(document.querySelectorAll<HTMLElement>('section[aria-label^="Changes in "]')).map((el) =>
    el.getAttribute("aria-label"),
  );
const stacked = () => fireEvent.click(screen.getByRole("button", { name: "Stacked" }));
const announcement = () => screen.getByTestId("file-selection-announcement");

describe("stacked view of all diffs (#793)", () => {
  beforeEach(() => {
    localStorage.clear();
    observers = [];
    vi.stubGlobal("IntersectionObserver", ControlledObserver);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("offers a List | Stacked toggle, List by default, and remembers Stacked on this computer", () => {
    render(<Pane client={fakeRepoClient({ getWorkingDiff: diffNamingFile }, "empty")} status={status} />);
    const toggle = screen.getByRole("group", { name: "Diff layout" });
    expect(within(toggle).getByRole("button", { name: "List" })).toHaveAttribute("aria-pressed", "true");
    expect(within(toggle).getByRole("button", { name: "Stacked" })).toHaveAttribute("aria-pressed", "false");
    expect(sectionNames()).toHaveLength(1);

    stacked();

    expect(within(toggle).getByRole("button", { name: "Stacked" })).toHaveAttribute("aria-pressed", "true");
    expect(within(toggle).getByRole("button", { name: "List" })).toHaveAttribute("aria-pressed", "false");
    expect(localStorage.getItem("working-tree-diff-layout")).toBe("stacked");
    // Every file, in list order (Changes, then Staged), under the one file list and toolbar.
    expect(sectionNames()).toEqual([
      "Changes in a.txt (unstaged)",
      "Changes in b.txt (unstaged)",
      "Changes in c.txt (staged)",
    ]);
    expect(screen.getByRole("listbox", { name: "Unstaged changes" })).toBeInTheDocument();
    expect(screen.getByRole("listbox", { name: "Staged changes" })).toBeInTheDocument();
    expect(screen.getAllByRole("group", { name: "Diff view options" })).toHaveLength(1);

    fireEvent.click(within(toggle).getByRole("button", { name: "List" }));
    expect(localStorage.getItem("working-tree-diff-layout")).toBe("list");
    expect(sectionNames()).toEqual(["Changes in a.txt (unstaged)"]);
  });

  it("opens stacked when that was the last choice", () => {
    localStorage.setItem("working-tree-diff-layout", "stacked");
    render(<Pane client={fakeRepoClient({ getWorkingDiff: diffNamingFile }, "empty")} status={status} />);
    expect(screen.getByRole("button", { name: "Stacked" })).toHaveAttribute("aria-pressed", "true");
    expect(sectionNames()).toHaveLength(3);
  });

  it("names each stacked file in a heading, for moving between them with a screen reader", () => {
    localStorage.setItem("working-tree-diff-layout", "stacked");
    render(<Pane client={fakeRepoClient({ getWorkingDiff: diffNamingFile }, "empty")} status={status} />);
    expect(screen.getAllByRole("heading", { level: 3 }).map((heading) => heading.textContent)).toEqual([
      "a.txt",
      "b.txt",
      "c.txt",
    ]);
  });

  it("loads a file's diff only once its section comes near the view", async () => {
    localStorage.setItem("working-tree-diff-layout", "stacked");
    const getWorkingDiff = vi.fn(diffNamingFile);
    render(<Pane client={fakeRepoClient({ getWorkingDiff }, "empty")} status={status} />);
    expect(getWorkingDiff).not.toHaveBeenCalled();

    setNear(section("b.txt (unstaged)"), true);

    expect(await within(section("b.txt (unstaged)")).findByText("unstaged b.txt")).toBeInTheDocument();
    expect(getWorkingDiff).toHaveBeenCalledTimes(1);
    expect(getWorkingDiff).toHaveBeenCalledWith(REPO, "b.txt", false, expect.anything());
    expect(screen.queryByText("unstaged a.txt")).toBeNull();
  });

  it("stages a hunk from any stacked diff, not only the current file's", async () => {
    localStorage.setItem("working-tree-diff-layout", "stacked");
    const onStageHunk = vi.fn();
    const onUnstageHunk = vi.fn();
    render(
      <Pane
        client={fakeRepoClient({ getWorkingDiff: diffNamingFile }, "empty")}
        status={status}
        actions={{ onStageHunk, onUnstageHunk }}
      />,
    );
    setNear(section("b.txt (unstaged)"), true);
    setNear(section("c.txt (staged)"), true);
    // a.txt is the current file.
    expect(announcement()).toHaveTextContent("a.txt selected");

    fireEvent.click(await within(section("b.txt (unstaged)")).findByRole("button", { name: "Stage hunk" }));
    await waitFor(() =>
      expect(onStageHunk).toHaveBeenCalledWith("b.txt", 1, 1, { contextLines: 3, algorithm: "Default" }),
    );

    fireEvent.click(await within(section("c.txt (staged)")).findByRole("button", { name: "Unstage hunk" }));
    await waitFor(() =>
      expect(onUnstageHunk).toHaveBeenCalledWith("c.txt", 1, 1, { contextLines: 3, algorithm: "Default" }),
    );
  });

  it("stages selected lines from a stacked diff", async () => {
    localStorage.setItem("working-tree-diff-layout", "stacked");
    const lineHunk: DiffHunk = {
      oldStart: 1,
      oldLines: 2,
      newStart: 1,
      newLines: 3,
      lines: [
        { origin: "Context", content: "a\n" },
        { origin: "Add", content: "b\n" },
        { origin: "Context", content: "c\n" },
      ],
    };
    const onStageLines = vi.fn();
    render(
      <Pane
        client={fakeRepoClient({ getWorkingDiff: async () => [lineHunk] }, "empty")}
        status={status}
        actions={{ onStageLines }}
      />,
    );
    const b = section("b.txt (unstaged)");
    setNear(b, true);
    await within(b).findByRole("button", { name: "Stage hunk" });

    fireEvent.click(b.querySelector<HTMLElement>('[data-hunk-index="0"][data-line-index="1"]')!);
    fireEvent.click(within(b).getByRole("button", { name: "Stage lines (1)" }));

    await waitFor(() =>
      expect(onStageLines).toHaveBeenCalledWith(
        "b.txt",
        1,
        1,
        { contextLines: 3, algorithm: "Default" },
        [1],
        ["a\n", "b\n", "c\n"],
      ),
    );
  });

  it("jumps to a file's diff when it is picked in the list", () => {
    localStorage.setItem("working-tree-diff-layout", "stacked");
    render(<Pane client={fakeRepoClient({ getWorkingDiff: diffNamingFile }, "empty")} status={status} />);
    const c = section("c.txt (staged)");
    c.scrollIntoView = vi.fn();

    fireEvent.click(screen.getByRole("option", { name: /c\.txt/ }));

    expect(c.scrollIntoView).toHaveBeenCalledWith({ block: "start" });
    expect(announcement()).toHaveTextContent("c.txt selected");
  });

  it("doesn't scroll the stack when it first opens", () => {
    const scrolled = vi.fn();
    const original = Element.prototype.scrollIntoView;
    Element.prototype.scrollIntoView = scrolled;
    try {
      localStorage.setItem("working-tree-diff-layout", "stacked");
      render(<Pane client={fakeRepoClient({ getWorkingDiff: diffNamingFile }, "empty")} status={status} />);
      const sectionCalls = scrolled.mock.contexts.filter(
        (element) => element instanceof HTMLElement && element.tagName === "SECTION",
      );
      expect(sectionCalls).toHaveLength(0);
    } finally {
      Element.prototype.scrollIntoView = original;
    }
  });

  it("moves focus from the list into the current file's stacked diff with Enter", async () => {
    localStorage.setItem("working-tree-diff-layout", "stacked");
    render(<Pane client={fakeRepoClient({ getWorkingDiff: diffNamingFile }, "empty")} status={status} />);
    const list = screen.getByRole("listbox", { name: "Unstaged changes" });
    list.focus();
    fireEvent.keyDown(list, { key: "ArrowDown" });
    expect(announcement()).toHaveTextContent("b.txt selected");

    const b = section("b.txt (unstaged)");
    b.scrollIntoView = vi.fn();
    fireEvent.keyDown(list, { key: "Enter" });

    await waitFor(() => expect(b).toContainElement(document.activeElement as HTMLElement));
    expect(b.scrollIntoView).toHaveBeenCalledWith({ block: "start" });
  });

  it("goes to the next and previous file's diff with j and k inside a stacked diff", async () => {
    localStorage.setItem("working-tree-diff-layout", "stacked");
    render(<Pane client={fakeRepoClient({ getWorkingDiff: diffNamingFile }, "empty")} status={status} />);
    const a = section("a.txt (unstaged)");
    const b = section("b.txt (unstaged)");
    b.scrollIntoView = vi.fn();
    a.scrollIntoView = vi.fn();
    a.focus();

    fireEvent.keyDown(a, { key: "j" });

    await waitFor(() => expect(b).toContainElement(document.activeElement as HTMLElement));
    expect(b.scrollIntoView).toHaveBeenCalledWith({ block: "start" });
    expect(announcement()).toHaveTextContent("b.txt selected");

    fireEvent.keyDown(b, { key: "k" });

    await waitFor(() => expect(a).toContainElement(document.activeElement as HTMLElement));
    expect(announcement()).toHaveTextContent("a.txt selected");
  });

  it("makes a stacked diff's file the current one when focus moves into it", () => {
    localStorage.setItem("working-tree-diff-layout", "stacked");
    render(<Pane client={fakeRepoClient({ getWorkingDiff: diffNamingFile }, "empty")} status={status} />);
    const c = section("c.txt (staged)");
    c.scrollIntoView = vi.fn();

    act(() => c.focus());

    expect(announcement()).toHaveTextContent("c.txt selected");
    // Focus is already there: no jump.
    expect(c.scrollIntoView).not.toHaveBeenCalled();
  });

  it("refetches only the diffs near the view on a refresh, the others once they come back", async () => {
    localStorage.setItem("working-tree-diff-layout", "stacked");
    const getWorkingDiff = vi.fn(diffNamingFile);
    const client = fakeRepoClient({ getWorkingDiff }, "empty");
    const { rerender } = render(<Pane client={client} status={status} />);
    setNear(section("a.txt (unstaged)"), true);
    setNear(section("b.txt (unstaged)"), true);
    await within(section("b.txt (unstaged)")).findByText("unstaged b.txt");
    await within(section("a.txt (unstaged)")).findByText("unstaged a.txt");
    setNear(section("b.txt (unstaged)"), false);
    const callsFor = (path: string) => getWorkingDiff.mock.calls.filter((call) => call[1] === path).length;
    expect(callsFor("a.txt")).toBe(1);
    expect(callsFor("b.txt")).toBe(1);

    rerender(<Pane client={client} status={status} refreshGeneration={1} />);

    await waitFor(() => expect(callsFor("a.txt")).toBe(2));
    expect(callsFor("b.txt")).toBe(1);
    // Its old diff stays on screen meanwhile.
    expect(within(section("b.txt (unstaged)")).getByText("unstaged b.txt")).toBeInTheDocument();

    setNear(section("b.txt (unstaged)"), true);
    await waitFor(() => expect(callsFor("b.txt")).toBe(2));

    // Back again with nothing changed: no new fetch.
    setNear(section("b.txt (unstaged)"), false);
    setNear(section("b.txt (unstaged)"), true);
    await act(async () => {
      await Promise.resolve();
    });
    expect(callsFor("b.txt")).toBe(2);
  });

  it("stacks 100 files at a time, more on request or up to a file picked in the list", () => {
    localStorage.setItem("working-tree-diff-layout", "stacked");
    const many = [
      ...Array.from({ length: 250 }, (_, i) => entry(`f${String(i).padStart(3, "0")}.txt`, false)),
      entry("s.txt", true),
    ];
    render(<Pane client={fakeRepoClient({ getWorkingDiff: diffNamingFile }, "empty")} status={many} />);
    expect(sectionNames()).toHaveLength(100);
    expect(sectionNames()[99]).toBe("Changes in f099.txt (unstaged)");
    expect(screen.getByText("Showing the diffs of the first 100 of 251 files.")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Show 100 more" }));
    expect(sectionNames()).toHaveLength(200);
    expect(screen.getByText("Showing the diffs of the first 200 of 251 files.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Show 51 more" })).toBeInTheDocument();

    // The staged file is the 251st: picking it stacks every file up to it.
    fireEvent.click(screen.getByRole("option", { name: /^s\.txt/ }));
    expect(sectionNames()).toHaveLength(251);
    expect(screen.queryByText(/^Showing the diffs of the first/)).toBeNull();
    expect(screen.queryByRole("button", { name: /^Show \d+ more$/ })).toBeNull();
  });

  it("shows an untracked folder entry's actions in its place in the stack", () => {
    localStorage.setItem("working-tree-diff-layout", "stacked");
    const withFolder = [entry("a.txt", false), { ...entry("big/", false, "New"), untrackedFolder: true }];
    render(<Pane client={fakeRepoClient({ getWorkingDiff: diffNamingFile }, "empty")} status={withFolder} />);
    const folder = screen.getByRole("region", { name: "Untracked folder big/" });
    expect(within(folder).getByRole("button", { name: "Stage folder" })).toBeInTheDocument();
    expect(sectionNames()).toEqual(["Changes in a.txt (unstaged)"]);
  });
});

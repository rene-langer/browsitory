import { fireEvent, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { GraphCommit, HeadInfo } from "../ipc/RepoClient";
import { CommitGraph } from "./CommitGraph";

// How often the graph itself renders (#837): it calls `useVirtualRows` once per render.
const renders = vi.hoisted(() => ({ count: 0 }));
vi.mock("./useVirtualRows", async (importOriginal) => {
  const original = await importOriginal<typeof import("./useVirtualRows")>();
  return {
    ...original,
    useVirtualRows: ((...args) => {
      renders.count += 1;
      return original.useVirtualRows(...args);
    }) as typeof original.useVirtualRows,
  };
});

const NO_HEAD: HeadInfo = {
  headId: null,
  detachedAt: null,
  headPushed: false,
  operation: null,
  operationLeftover: false,
  squashMessage: null,
  commitHooks: [],
  sequence: null,
  rebase: null,
  bisect: null,
  terminalSequence: null,
  terminalRebase: null,
};

function commit(id: string, parentIds: string[]): GraphCommit {
  return {
    id,
    shortId: id,
    summary: `summary ${id}`,
    authorName: "A",
    authorEmail: "a@example.com",
    timestamp: 0,
    parentIds,
    branchRefs: [],
    remoteBranchRefs: [],
  };
}

// A fork: F1 and M2 are both children of M1, then a main line below.
const forkCommits = [
  commit("F1", ["M1"]),
  commit("M2", ["M1"]),
  commit("M1", ["M0"]),
  commit("M0", []),
];

const props = {
  headInfo: NO_HEAD,
  currentBranch: "main",
  onCherryPick: vi.fn(),
  onRevert: vi.fn(),
  onCheckoutCommit: vi.fn(),
  onChooseParent: vi.fn(),
  onRequestReset: vi.fn(),
  onEditHeadMessage: vi.fn(),
  status: [],
  selectedRow: "uncommitted" as const,
  operationsBlocked: false,
  onSelectRow: vi.fn(),
  onBranchFromCommit: vi.fn(),
  onRebaseFromCommit: vi.fn(),
};

describe("CommitGraph render work (#837)", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("a hover dims the other lanes without rendering the graph again", () => {
    const { container } = render(<CommitGraph {...props} commits={forkCommits} />);
    const row = (id: string) => container.querySelector(`[data-commit-id="${id}"]`)!;
    renders.count = 0;

    fireEvent.mouseEnter(row("M2"));
    expect(row("F1").querySelector("circle")!.getAttribute("opacity")).toBe("0.25");
    expect(row("M2").querySelector("circle")!.getAttribute("opacity")).toBe("1");
    fireEvent.mouseLeave(row("M2"));
    expect(row("F1").querySelector("circle")!.getAttribute("opacity")).toBe("1");
    fireEvent.mouseEnter(row("F1"));
    expect(row("M2").querySelector("circle")!.getAttribute("opacity")).toBe("0.25");

    expect(renders.count).toBe(0);
  });

  it("watches the rows' visibility with one observer, also for rows a new page adds", () => {
    const observers: { observed: Element[]; report: (records: Partial<IntersectionObserverEntry>[]) => void }[] = [];
    vi.stubGlobal(
      "IntersectionObserver",
      class {
        observed: Element[] = [];
        report: (records: Partial<IntersectionObserverEntry>[]) => void;
        constructor(callback: (records: Partial<IntersectionObserverEntry>[]) => void) {
          this.report = callback;
          observers.push(this);
        }
        observe(element: Element) {
          this.observed.push(element);
        }
        unobserve() {}
        disconnect() {}
      },
    );
    const onCommitsVisible = vi.fn();
    const page = forkCommits.slice(0, 3);
    const { rerender, container } = render(<CommitGraph {...props} commits={page} onCommitsVisible={onCommitsVisible} />);
    rerender(<CommitGraph {...props} commits={page} selectedRow={{ commitId: "M2" }} onCommitsVisible={onCommitsVisible} />);
    rerender(<CommitGraph {...props} commits={forkCommits} selectedRow={{ commitId: "M2" }} onCommitsVisible={onCommitsVisible} />);

    expect(observers).toHaveLength(1);
    const [observer] = observers;
    const ids = observer.observed.map((row) => row.getAttribute("data-commit-id"));
    expect(new Set(ids)).toEqual(new Set(["F1", "M2", "M1", "M0"]));

    const rowOf = (id: string) => container.querySelector(`[data-commit-id="${id}"]`)!;
    observer.report([
      { target: rowOf("M0"), isIntersecting: true },
      { target: rowOf("F1"), isIntersecting: true },
    ]);
    // In graph order.
    expect(onCommitsVisible).toHaveBeenLastCalledWith(["F1", "M0"]);
  });
});

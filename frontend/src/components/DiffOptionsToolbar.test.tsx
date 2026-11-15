import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useState, type ReactNode } from "react";
import { DEFAULT_DIFF_SETTINGS, type DiffHunk, type DiffSettings } from "../ipc/RepoClient";
import { DiffSettingsContext } from "../state/useDiffSettings";
import { DiffViewOptionsProvider } from "../state/DiffViewOptionsProvider";
import { useDiffViewOptions } from "../state/useDiffViewOptions";
import { DiffOptionsToolbar } from "./DiffOptionsToolbar";
import { DiffView } from "./DiffView";
import { chooseViewOption, viewOptionNames, viewOptionState } from "../test/viewMenu";

const hunks: DiffHunk[] = [
  {
    oldStart: 1,
    oldLines: 1,
    newStart: 1,
    newLines: 1,
    lines: [
      { origin: "Remove", content: "old" },
      { origin: "Add", content: "new" },
    ],
  },
];

function Settings({ initial = {}, onUpdate, children }: { initial?: Partial<DiffSettings>; onUpdate?: (p: Partial<DiffSettings>) => void; children: ReactNode }) {
  const [settings, setSettings] = useState<DiffSettings>({ ...DEFAULT_DIFF_SETTINGS, ...initial });
  return (
    <DiffSettingsContext.Provider
      value={{
        settings,
        update: (partial) => {
          onUpdate?.(partial);
          setSettings((prev) => ({ ...prev, ...partial }));
        },
        error: null,
        dismissError: () => {},
      }}
    >
      {children}
    </DiffSettingsContext.Provider>
  );
}

function WholeFileProbe() {
  const { wholeFile } = useDiffViewOptions();
  return <span data-testid="whole-file">{String(wholeFile)}</span>;
}

beforeEach(() => localStorage.clear());
afterEach(() => localStorage.clear());

describe("DiffOptionsToolbar (#640)", () => {
  it("renders one toolbar for several diffs, and Split applies to all of them", () => {
    const { container } = render(
      <DiffViewOptionsProvider>
        <DiffOptionsToolbar />
        <DiffView hunks={hunks} path="a.txt" />
        <DiffView hunks={hunks} path="b.txt" />
      </DiffViewOptionsProvider>,
    );

    expect(screen.getAllByRole("group", { name: "Diff view options" })).toHaveLength(1);
    expect(screen.getByRole("button", { name: "Unified view" })).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(screen.getByRole("button", { name: "Split view" }));

    expect(screen.getByRole("button", { name: "Split view" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "Unified view" })).toHaveAttribute("aria-pressed", "false");
    expect(container.querySelectorAll('[data-diff-column="old"]')).toHaveLength(2);
  });

  it("keeps Split view across a remount, and in a new provider (persisted)", () => {
    const { unmount } = render(
      <DiffViewOptionsProvider>
        <DiffOptionsToolbar />
      </DiffViewOptionsProvider>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Split view" }));
    unmount();

    render(
      <DiffViewOptionsProvider>
        <DiffOptionsToolbar />
      </DiffViewOptionsProvider>,
    );
    expect(screen.getByRole("button", { name: "Split view" })).toHaveAttribute("aria-pressed", "true");
  });

  it("keeps Show whole file across a remount, and shares it with every reader", () => {
    const { unmount } = render(
      <DiffViewOptionsProvider>
        <DiffOptionsToolbar />
        <WholeFileProbe />
      </DiffViewOptionsProvider>,
    );
    chooseViewOption("Show whole file");
    expect(screen.getByTestId("whole-file")).toHaveTextContent("true");
    unmount();

    render(
      <DiffViewOptionsProvider>
        <DiffOptionsToolbar />
      </DiffViewOptionsProvider>,
    );
    expect(viewOptionState("Show whole file")).toBe("true");
  });

  it("a nested provider shares its parent's options", () => {
    render(
      <DiffViewOptionsProvider>
        <DiffOptionsToolbar />
        <DiffViewOptionsProvider>
          <WholeFileProbe />
        </DiffViewOptionsProvider>
      </DiffViewOptionsProvider>,
    );
    chooseViewOption("Show whole file");
    expect(screen.getByTestId("whole-file")).toHaveTextContent("true");
  });

  it("survives storage that throws", () => {
    const getItem = vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    const setItem = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    try {
      render(
        <DiffViewOptionsProvider>
          <DiffOptionsToolbar />
        </DiffViewOptionsProvider>,
      );
      fireEvent.click(screen.getByRole("button", { name: "Split view" }));
      expect(screen.getByRole("button", { name: "Split view" })).toHaveAttribute("aria-pressed", "true");
    } finally {
      getItem.mockRestore();
      setItem.mockRestore();
    }
  });

  it("Wrap lines persists through the wordWrap setting: Auto is mixed, a click turns it on, then off", () => {
    const onUpdate = vi.fn();
    render(
      <Settings onUpdate={onUpdate}>
        <DiffOptionsToolbar />
      </Settings>,
    );
    expect(viewOptionState("Wrap lines")).toBe("mixed");

    chooseViewOption("Wrap lines");
    expect(onUpdate).toHaveBeenLastCalledWith({ wordWrap: "On" });
    expect(viewOptionState("Wrap lines")).toBe("true");

    chooseViewOption("Wrap lines");
    expect(onUpdate).toHaveBeenLastCalledWith({ wordWrap: "Off" });
    expect(viewOptionState("Wrap lines")).toBe("false");
  });

  it("Wrap lines on wraps every diff below it", () => {
    const { container } = render(
      <Settings initial={{ wordWrap: "Off" }}>
        <DiffOptionsToolbar />
        <DiffView hunks={hunks} path="a.ts" />
        <DiffView hunks={hunks} path="b.ts" />
      </Settings>,
    );
    expect(container.querySelectorAll('[data-wrap="true"]')).toHaveLength(0);
    chooseViewOption("Wrap lines");
    expect(container.querySelectorAll('[data-wrap="true"]')).toHaveLength(2);
  });

  it("gives every icon button and View's toggles a tooltip", () => {
    render(
      <DiffViewOptionsProvider>
        <DiffOptionsToolbar />
      </DiffViewOptionsProvider>,
    );
    for (const name of ["Unified view", "Split view", "Diff view"]) {
      expect(screen.getByRole("button", { name })).toHaveAttribute("title");
    }
    fireEvent.click(screen.getByRole("button", { name: "Diff view" }));
    for (const name of ["Wrap lines", "Show whole file", "Ignore whitespace", "Show whitespace", "Show syntax highlighting"]) {
      expect(screen.getByRole("menuitemcheckbox", { name })).toHaveAttribute("title");
    }
  });

  it("omits Show whole file when the pane has no such toggle", () => {
    render(<DiffOptionsToolbar wholeFile={false} />);
    expect(viewOptionNames()).not.toContain("Show whole file");
  });

  it("disables the context stepper while the whole file is shown", () => {
    render(
      <DiffViewOptionsProvider>
        <DiffOptionsToolbar />
      </DiffViewOptionsProvider>,
    );
    // Not inside act(): the menu has to render before its item is picked.
    chooseViewOption("Show whole file");
    expect(screen.getByRole("button", { name: "More context lines" })).toBeDisabled();
    cleanup();
  });
});

describe("the View menu (#1007)", () => {
  it("keeps the layout and the context lines in the row, and the rarely changed toggles in View, in words", () => {
    render(
      <DiffViewOptionsProvider>
        <DiffOptionsToolbar />
      </DiffViewOptionsProvider>,
    );
    for (const name of ["Unified view", "Split view", "Fewer context lines", "More context lines"]) {
      expect(screen.getByRole("button", { name })).toBeInTheDocument();
    }
    for (const name of ["Wrap lines", "Show whole file", "Ignore whitespace", "Show whitespace", "Syntax highlighting"]) {
      expect(screen.queryByRole("button", { name })).toBeNull();
    }
    expect(screen.getByRole("button", { name: "Diff view" })).toHaveTextContent("View");
    expect(viewOptionNames()).toEqual(["Wrap lines", "Show whole file", "Ignore whitespace", "Show whitespace", "Show syntax highlighting"]);
  });

  it("marks Wrap lines on Auto as mixed (an A badge), and a choice turns it on", () => {
    const onUpdate = vi.fn();
    render(
      <Settings onUpdate={onUpdate}>
        <DiffOptionsToolbar />
      </Settings>,
    );
    expect(viewOptionState("Wrap lines")).toBe("mixed");
    chooseViewOption("Wrap lines");
    expect(onUpdate).toHaveBeenLastCalledWith({ wordWrap: "On" });
    expect(viewOptionState("Wrap lines")).toBe("true");
  });

  it("toggles syntax highlighting from the menu", () => {
    const onUpdate = vi.fn();
    render(
      <Settings onUpdate={onUpdate}>
        <DiffOptionsToolbar />
      </Settings>,
    );
    expect(viewOptionState("Show syntax highlighting")).toBe("true");
    chooseViewOption("Show syntax highlighting");
    expect(onUpdate).toHaveBeenLastCalledWith({ syntaxHighlight: false });
  });
});

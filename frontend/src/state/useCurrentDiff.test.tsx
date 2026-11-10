import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DEFAULT_DIFF_SETTINGS, type DiffHunk, type DiffSettings } from "../ipc/RepoClient";
import { DiffSettingsContext } from "./useDiffSettings";
import { DiffView, type WholeFileToggle } from "../components/DiffView";
import { CurrentDiffContext, useCurrentDiffRegistry, type CurrentDiffRegistry } from "./useCurrentDiff";

const hunk: DiffHunk = {
  oldStart: 1,
  oldLines: 1,
  newStart: 1,
  newLines: 1,
  lines: [{ origin: "Add", content: "x\n" }],
};

// The first diff is prose, which Auto wraps; the second is code, which it doesn't. The palette's
// Wrap lines sets the setting (#640) to the opposite of what the current diff shows.
function Harness({
  onRegistry,
  update,
  currentSecond = false,
  wholeFile,
}: {
  onRegistry: (registry: CurrentDiffRegistry) => void;
  update: (partial: Partial<DiffSettings>) => void;
  currentSecond?: boolean;
  wholeFile?: WholeFileToggle;
}) {
  const registry = useCurrentDiffRegistry();
  onRegistry(registry);
  return (
    <DiffSettingsContext.Provider value={{ settings: DEFAULT_DIFF_SETTINGS, update, error: null, dismissError: () => {} }}>
      <CurrentDiffContext.Provider value={registry}>
        <section aria-label="first">
          <DiffView hunks={[hunk]} path="docs/first.md" />
        </section>
        <section aria-label="second">
          <DiffView hunks={[hunk]} path="src/second.rs" wholeFile={wholeFile} current={currentSecond} />
        </section>
      </CurrentDiffContext.Provider>
    </DiffSettingsContext.Provider>
  );
}

function setup(props: { currentSecond?: boolean; wholeFile?: WholeFileToggle } = {}) {
  let latest!: CurrentDiffRegistry;
  const update = vi.fn();
  const utils = render(<Harness onRegistry={(r) => (latest = r)} update={update} {...props} />);
  return { ...utils, registry: () => latest, update };
}

describe("useCurrentDiffRegistry", () => {
  it("falls back to the first diff shown", () => {
    const { registry, update } = setup();
    expect(registry().current).toEqual({ path: "docs/first.md", canShowWholeFile: false });
    act(() => registry().toggleWrap());
    // The first diff (prose) wraps under Auto: the palette turns wrapping off.
    expect(update).toHaveBeenCalledWith({ wordWrap: "Off" });
  });

  it("makes the diff clicked last the current one", () => {
    const { registry, update, container } = setup();
    fireEvent.pointerDown(container.querySelector('section[aria-label="second"] [data-wrap]')!);
    expect(registry().current?.path).toBe("src/second.rs");
    act(() => registry().toggleWrap());
    // The second diff (code) doesn't wrap under Auto: the palette turns wrapping on.
    expect(update).toHaveBeenCalledWith({ wordWrap: "On" });
  });

  it("follows the current prop and toggles that diff's whole file", () => {
    const wholeFile = { on: false, onToggle: vi.fn(), tooLongLines: null };
    const { registry } = setup({ currentSecond: true, wholeFile });
    expect(registry().current).toEqual({ path: "src/second.rs", canShowWholeFile: true });
    act(() => registry().toggleWholeFile());
    expect(wholeFile.onToggle).toHaveBeenCalledTimes(1);
  });

  it("drops a diff that unmounts", () => {
    let latest!: CurrentDiffRegistry;
    const onRegistry = (registry: CurrentDiffRegistry) => (latest = registry);
    function One({ show }: { show: boolean }) {
      const registry = useCurrentDiffRegistry();
      onRegistry(registry);
      return (
        <CurrentDiffContext.Provider value={registry}>
          {show && <DiffView hunks={[hunk]} path="a.txt" />}
        </CurrentDiffContext.Provider>
      );
    }
    const { rerender } = render(<One show />);
    expect(latest.current?.path).toBe("a.txt");
    rerender(<One show={false} />);
    expect(latest.current).toBeNull();
    expect(screen.queryByText("x")).toBeNull();
  });
});

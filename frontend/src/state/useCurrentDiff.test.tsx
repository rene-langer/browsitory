import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { DiffHunk } from "../ipc/RepoClient";
import { DiffView, type WholeFileToggle } from "../components/DiffView";
import { CurrentDiffContext, useCurrentDiffRegistry, type CurrentDiffRegistry } from "./useCurrentDiff";

const hunk: DiffHunk = {
  oldStart: 1,
  oldLines: 1,
  newStart: 1,
  newLines: 1,
  lines: [{ origin: "Add", content: "x\n" }],
};

function Harness({
  onRegistry,
  currentSecond = false,
  wholeFile,
}: {
  onRegistry: (registry: CurrentDiffRegistry) => void;
  currentSecond?: boolean;
  wholeFile?: WholeFileToggle;
}) {
  const registry = useCurrentDiffRegistry();
  onRegistry(registry);
  return (
    <CurrentDiffContext.Provider value={registry}>
      <section aria-label="first">
        <DiffView hunks={[hunk]} path="src/first.rs" />
      </section>
      <section aria-label="second">
        <DiffView hunks={[hunk]} path="src/second.rs" wholeFile={wholeFile} current={currentSecond} />
      </section>
    </CurrentDiffContext.Provider>
  );
}

function setup(props: { currentSecond?: boolean; wholeFile?: WholeFileToggle } = {}) {
  let latest!: CurrentDiffRegistry;
  const utils = render(<Harness onRegistry={(r) => (latest = r)} {...props} />);
  const wrapOf = (label: string) =>
    utils.container.querySelector(`section[aria-label="${label}"] [data-wrap]`)!.getAttribute("data-wrap");
  return { ...utils, registry: () => latest, wrapOf };
}

describe("useCurrentDiffRegistry", () => {
  it("falls back to the first diff shown", () => {
    const { registry, wrapOf } = setup();
    expect(registry().current).toEqual({ path: "src/first.rs", canShowWholeFile: false });
    act(() => registry().toggleWrap());
    // Code in the unified view starts unwrapped (Auto); the palette flips only the first diff.
    expect(wrapOf("first")).toBe("true");
    expect(wrapOf("second")).toBe("false");
  });

  it("makes the diff clicked last the current one", () => {
    const { registry, wrapOf, container } = setup();
    fireEvent.pointerDown(container.querySelector('section[aria-label="second"] [data-wrap]')!);
    expect(registry().current?.path).toBe("src/second.rs");
    act(() => registry().toggleWrap());
    expect(wrapOf("second")).toBe("true");
    expect(wrapOf("first")).toBe("false");
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

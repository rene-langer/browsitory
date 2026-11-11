import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { EARLIER_ERRORS_KEPT, useEarlierErrors } from "./useEarlierErrors";

describe("useEarlierErrors (#663)", () => {
  function setup(initial: string | null = null) {
    const onDismiss = vi.fn();
    const hook = renderHook(({ error }: { error: string | null }) => useEarlierErrors(error, onDismiss), {
      initialProps: { error: initial },
    });
    return { ...hook, onDismiss };
  }

  it("keeps an error the next action cleared, and shows it under the next one", () => {
    const { result, rerender } = setup("Fetch failed");
    expect(result.current.earlier).toEqual([]);
    rerender({ error: null }); // the next action started
    rerender({ error: "Push failed" });
    expect(result.current.earlier).toEqual(["Fetch failed"]);
    rerender({ error: "Pull failed" });
    expect(result.current.earlier).toEqual(["Push failed", "Fetch failed"]);
  });

  it("never lists the current error, nor one twice", () => {
    const { result, rerender } = setup("Same");
    rerender({ error: null });
    rerender({ error: "Same" });
    expect(result.current.earlier).toEqual([]);
    rerender({ error: "Other" });
    rerender({ error: "Same" });
    expect(result.current.earlier).toEqual(["Other"]);
  });

  it("keeps at most EARLIER_ERRORS_KEPT", () => {
    const { result, rerender } = setup("e0");
    for (let n = 1; n <= EARLIER_ERRORS_KEPT + 2; n += 1) rerender({ error: `e${n}` });
    expect(result.current.earlier).toHaveLength(EARLIER_ERRORS_KEPT);
    expect(result.current.earlier[0]).toBe(`e${EARLIER_ERRORS_KEPT + 1}`);
  });

  it("forgets them all on dismiss", () => {
    const { result, rerender, onDismiss } = setup("a");
    rerender({ error: "b" });
    act(() => result.current.dismiss());
    expect(onDismiss).toHaveBeenCalledOnce();
    rerender({ error: null });
    rerender({ error: "c" });
    expect(result.current.earlier).toEqual([]);
  });
});

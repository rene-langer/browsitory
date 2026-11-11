import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { __resetHostVisibilityForTests, publishHostVisibility } from "../ipc/hostVisibility";
import { CHANGE_CHECK_INTERVAL_MS, CHANGE_CHECK_MAX_INTERVAL_MS, useExternalChanges } from "./useExternalChanges";

type Props = { active: boolean; busy: boolean; checkForChanges: (wanted: () => boolean) => Promise<void> };

function setup(initial: Partial<Props> = {}) {
  const checkForChanges = vi.fn(async (_wanted: () => boolean) => {});
  const props: Props = { active: true, busy: false, checkForChanges, ...initial };
  const view = renderHook((p: Props) => useExternalChanges(p), { initialProps: props });
  return { ...view, checkForChanges: props.checkForChanges as typeof checkForChanges, props };
}

// Lets the check's promise settle, so the next timer is scheduled.
async function settle() {
  await act(async () => {
    await Promise.resolve();
  });
}

describe("useExternalChanges (#650)", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
    __resetHostVisibilityForTests();
  });

  it("checks at once and then every few seconds while the tab is shown", async () => {
    const { checkForChanges } = setup();
    await settle();
    expect(checkForChanges).toHaveBeenCalledTimes(1);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(CHANGE_CHECK_INTERVAL_MS);
    });
    expect(checkForChanges).toHaveBeenCalledTimes(2);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(CHANGE_CHECK_INTERVAL_MS);
    });
    expect(checkForChanges).toHaveBeenCalledTimes(3);
  });

  it("doesn't check a tab in the background, and checks it once it is shown again", async () => {
    const { checkForChanges, rerender, props } = setup({ active: false });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(CHANGE_CHECK_INTERVAL_MS * 3);
    });
    expect(checkForChanges).not.toHaveBeenCalled();
    rerender({ ...props, active: true });
    await settle();
    expect(checkForChanges).toHaveBeenCalledTimes(1);
  });

  it("doesn't check while an action runs, and checks once it is done", async () => {
    const { checkForChanges, rerender, props } = setup({ busy: true });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(CHANGE_CHECK_INTERVAL_MS * 3);
    });
    expect(checkForChanges).not.toHaveBeenCalled();
    rerender({ ...props, busy: false });
    await settle();
    expect(checkForChanges).toHaveBeenCalledTimes(1);
  });

  it("stops while the app is hidden and checks when it is visible again", async () => {
    const { checkForChanges } = setup();
    await settle();
    expect(checkForChanges).toHaveBeenCalledTimes(1);
    act(() => publishHostVisibility(false));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(CHANGE_CHECK_INTERVAL_MS * 3);
    });
    expect(checkForChanges).toHaveBeenCalledTimes(1);
    act(() => publishHostVisibility(true));
    await settle();
    expect(checkForChanges).toHaveBeenCalledTimes(2);
  });

  it("checks when the window gets the focus back", async () => {
    const { checkForChanges } = setup();
    await settle();
    await act(async () => {
      window.dispatchEvent(new Event("focus"));
      await Promise.resolve();
    });
    expect(checkForChanges).toHaveBeenCalledTimes(2);
  });

  it("runs one check at a time", async () => {
    let finish: () => void = () => {};
    const checkForChanges = vi.fn(() => new Promise<void>((resolve) => (finish = resolve)));
    setup({ checkForChanges });
    await settle();
    await act(async () => {
      window.dispatchEvent(new Event("focus"));
      await vi.advanceTimersByTimeAsync(CHANGE_CHECK_INTERVAL_MS * 2);
    });
    expect(checkForChanges).toHaveBeenCalledTimes(1);
    await act(async () => {
      finish();
      await Promise.resolve();
    });
    // That check took 6 s, so the next one waits the longest interval.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(CHANGE_CHECK_MAX_INTERVAL_MS);
    });
    expect(checkForChanges).toHaveBeenCalledTimes(2);
  });

  it("waits longer after a slow check, up to a minute", async () => {
    const checkForChanges = vi.fn(async () => {
      await new Promise<void>((resolve) => setTimeout(resolve, 2_000));
    });
    setup({ checkForChanges });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2_000);
    });
    expect(checkForChanges).toHaveBeenCalledTimes(1);
    // 2 s per check: the next one waits 20 s, not 3 s.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(19_000);
    });
    expect(checkForChanges).toHaveBeenCalledTimes(1);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1_000);
    });
    expect(checkForChanges).toHaveBeenCalledTimes(2);
    expect(CHANGE_CHECK_MAX_INTERVAL_MS).toBe(60_000);
  });

  it("tells the check whether it is still wanted", async () => {
    const seen: boolean[] = [];
    let wantedRef: () => boolean = () => true;
    const checkForChanges = vi.fn(async (wanted: () => boolean) => {
      wantedRef = wanted;
    });
    const { rerender, props } = setup({ checkForChanges });
    await settle();
    seen.push(wantedRef());
    rerender({ ...props, checkForChanges, busy: true });
    seen.push(wantedRef());
    expect(seen).toEqual([true, false]);
  });
});

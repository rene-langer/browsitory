import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { __resetHostVisibilityForTests, publishHostVisibility } from "../ipc/hostVisibility";
import type { RepoClient, RepoWatch } from "../ipc/RepoClient";
import { fakeRepoClient } from "../test/fakeRepoClient";
import {
  CHANGE_CHECK_INTERVAL_MS,
  CHANGE_CHECK_MAX_INTERVAL_MS,
  WATCHED_CHECK_INTERVAL_MS,
  useExternalChanges,
} from "./useExternalChanges";

type Props = {
  client: RepoClient;
  repoPath: string;
  active: boolean;
  busy: boolean;
  checkForChanges: (wanted: () => boolean) => Promise<void>;
};

const REPO = "/repos/app";

/** Not watched, as when the backend's watcher can't start: the checks poll. */
const unwatchedClient = () => fakeRepoClient({ watchRepo: async () => false, unwatchRepo: async () => {}, subscribeRepoWatch: () => () => {} });

function setup(initial: Partial<Props> = {}) {
  const checkForChanges = vi.fn(async (_wanted: () => boolean) => {});
  const props: Props = { client: unwatchedClient(), repoPath: REPO, active: true, busy: false, checkForChanges, ...initial };
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

/** A client whose watcher starts (or not), with a way to send its reports. */
function watchedClient(started = true) {
  const listeners = new Set<(event: RepoWatch) => void>();
  const watchRepo = vi.fn(async (_repoPath: string) => started);
  const unwatchRepo = vi.fn(async (_repoPath: string) => {});
  const client = fakeRepoClient({
    watchRepo,
    unwatchRepo,
    subscribeRepoWatch: (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  });
  const report = (event: RepoWatch) => {
    for (const listener of listeners) listener(event);
  };
  return { client, watchRepo, unwatchRepo, report, listeners };
}

describe("useExternalChanges with a watcher (#756)", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
    __resetHostVisibilityForTests();
  });

  it("watches the repository while mounted and stops when unmounted", async () => {
    const { client, watchRepo, unwatchRepo, listeners } = watchedClient();
    const { unmount } = setup({ client });
    await settle();
    expect(watchRepo).toHaveBeenCalledWith(REPO);
    expect(listeners.size).toBe(1);
    unmount();
    expect(unwatchRepo).toHaveBeenCalledWith(REPO);
    expect(listeners.size).toBe(0);
  });

  it("checks at once when the watcher reports a change", async () => {
    const { client, report } = watchedClient();
    const { checkForChanges } = setup({ client });
    await settle();
    expect(checkForChanges).toHaveBeenCalledTimes(1);
    await act(async () => {
      report({ repoPath: REPO, kind: "Changed" });
      await Promise.resolve();
    });
    expect(checkForChanges).toHaveBeenCalledTimes(2);
  });

  // #841: the watcher reports what changed while the window was away; the slow poll covers what
  // it misses.
  it("doesn't check when the window gets the focus back while watched", async () => {
    const { client } = watchedClient();
    const { checkForChanges } = setup({ client });
    await settle();
    await settle();
    expect(checkForChanges).toHaveBeenCalledTimes(1);
    await act(async () => {
      window.dispatchEvent(new Event("focus"));
      await Promise.resolve();
    });
    expect(checkForChanges).toHaveBeenCalledTimes(1);
  });

  it("ignores reports for other repositories", async () => {
    const { client, report } = watchedClient();
    const { checkForChanges } = setup({ client });
    await settle();
    await act(async () => {
      report({ repoPath: "/repos/other", kind: "Changed" });
      await Promise.resolve();
    });
    expect(checkForChanges).toHaveBeenCalledTimes(1);
  });

  it("doesn't check a hidden tab on a report; showing it checks", async () => {
    const { client, report } = watchedClient();
    const { checkForChanges, rerender, props } = setup({ client, active: false });
    await settle();
    await act(async () => {
      report({ repoPath: REPO, kind: "Changed" });
      await Promise.resolve();
    });
    expect(checkForChanges).not.toHaveBeenCalled();
    rerender({ ...props, active: true });
    await settle();
    expect(checkForChanges).toHaveBeenCalledTimes(1);
  });

  it("polls only every 30 s while watched", async () => {
    const { client } = watchedClient();
    const { checkForChanges } = setup({ client });
    await settle();
    await settle();
    expect(checkForChanges).toHaveBeenCalledTimes(1);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(WATCHED_CHECK_INTERVAL_MS - 1);
    });
    expect(checkForChanges).toHaveBeenCalledTimes(1);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(checkForChanges).toHaveBeenCalledTimes(2);
    expect(WATCHED_CHECK_INTERVAL_MS).toBe(30_000);
  });

  it("polls every few seconds again once the watcher stops", async () => {
    const { client, report } = watchedClient();
    const { checkForChanges } = setup({ client });
    await settle();
    await settle();
    await act(async () => {
      report({ repoPath: REPO, kind: "Stopped" });
      await Promise.resolve();
    });
    expect(checkForChanges).toHaveBeenCalledTimes(1);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(CHANGE_CHECK_INTERVAL_MS);
    });
    expect(checkForChanges).toHaveBeenCalledTimes(2);
  });

  it("polls every few seconds when the watcher can't start", async () => {
    const { client } = watchedClient(false);
    const { checkForChanges } = setup({ client });
    await settle();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(CHANGE_CHECK_INTERVAL_MS);
    });
    expect(checkForChanges).toHaveBeenCalledTimes(2);
  });

  it("checks again after a check that a report came in during", async () => {
    let finish: () => void = () => {};
    const checkForChanges = vi.fn(() => new Promise<void>((resolve) => (finish = resolve)));
    const { client, report } = watchedClient();
    setup({ client, checkForChanges });
    await settle();
    expect(checkForChanges).toHaveBeenCalledTimes(1);
    await act(async () => {
      report({ repoPath: REPO, kind: "Changed" });
      await Promise.resolve();
    });
    expect(checkForChanges).toHaveBeenCalledTimes(1);
    await act(async () => {
      finish();
      await Promise.resolve();
    });
    await settle();
    expect(checkForChanges).toHaveBeenCalledTimes(2);
  });
});

import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { __resetHostVisibilityForTests, publishHostVisibility } from "../ipc/hostVisibility";
import type { RepoClient, TransferErrorKind, TransferProgress } from "../ipc/RepoClient";
import { __resetAutoFetchForTests, lastFetchAt, recordFetch } from "../lib/autoFetch";
import { useAutoFetch, type AutoFetchOptions } from "./useAutoFetch";

const REPO = "/repo";
const MINUTE = 60_000;

function event(
  operationId: string,
  phase: TransferProgress["phase"],
  extra: Partial<TransferProgress> = {},
): TransferProgress {
  return {
    operationId,
    operation: "BackgroundFetch",
    phase,
    errorKind: null,
    current: 0,
    total: 0,
    receivedBytes: 0,
    message: null,
    hookOutput: null,
    ...extra,
  };
}

/** A `RepoClient` fake for the three methods the hook uses; each fetch completes as `script` says. */
function fakeClient(script: (id: string, emit: (progress: TransferProgress) => void) => void = (id, emit) => emit(event(id, "Completed"))) {
  const listeners = new Set<(progress: TransferProgress) => void>();
  const emit = (progress: TransferProgress) => listeners.forEach((listener) => listener(progress));
  let next = 1;
  const client = {
    backgroundFetch: vi.fn(async (_repoPath: string, _prune: boolean | null): Promise<string | null> => {
      const id = `background-fetch-${next++}`;
      script(id, emit);
      return id;
    }),
    cancelTransfer: vi.fn(async () => {}),
    subscribeTransferProgress: (listener: (progress: TransferProgress) => void) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
  return { client, emit, asClient: client as unknown as RepoClient };
}

function render(client: RepoClient, overrides: Partial<AutoFetchOptions> = {}) {
  const options: AutoFetchOptions = {
    client,
    repoPath: REPO,
    minutes: 5,
    prune: null,
    busy: false,
    refresh: vi.fn(async () => {}),
    notify: vi.fn(),
    openCredentials: vi.fn(),
    ...overrides,
  };
  const hook = renderHook((props: AutoFetchOptions) => useAutoFetch(props), { initialProps: options });
  return { ...hook, options };
}

const advance = (ms: number) => act(() => vi.advanceTimersByTimeAsync(ms));

describe("useAutoFetch", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-11-08T12:00:00Z"));
  });
  afterEach(() => {
    vi.useRealTimers();
    __resetAutoFetchForTests();
    __resetHostVisibilityForTests();
  });

  it("never fetches while the setting is off", async () => {
    const { client, asClient } = fakeClient();
    render(asClient, { minutes: 0 });

    await advance(120 * MINUTE);

    expect(client.backgroundFetch).not.toHaveBeenCalled();
  });

  it("fetches all remotes once per interval with the repository's Prune choice, then refreshes", async () => {
    const { client, asClient } = fakeClient();
    const { options } = render(asClient, { minutes: 5, prune: true });

    await advance(5 * MINUTE - 1);
    expect(client.backgroundFetch).not.toHaveBeenCalled();

    await advance(1);
    expect(client.backgroundFetch).toHaveBeenCalledExactlyOnceWith(REPO, true);
    expect(options.refresh).toHaveBeenCalledOnce();
    expect(lastFetchAt(REPO)).toBe(Date.now());

    await advance(5 * MINUTE);
    expect(client.backgroundFetch).toHaveBeenCalledTimes(2);
  });

  it("waits while another action runs and fetches once it is done", async () => {
    const { client, asClient } = fakeClient();
    const { rerender, options } = render(asClient, { busy: true });

    await advance(30 * MINUTE);
    expect(client.backgroundFetch).not.toHaveBeenCalled();

    rerender({ ...options, busy: false });
    await advance(0);
    expect(client.backgroundFetch).toHaveBeenCalledOnce();
  });

  it("pauses while the app is hidden", async () => {
    const { client, asClient } = fakeClient();
    render(asClient);

    act(() => publishHostVisibility(false));
    await advance(30 * MINUTE);
    expect(client.backgroundFetch).not.toHaveBeenCalled();

    act(() => publishHostVisibility(true));
    await advance(0);
    expect(client.backgroundFetch).toHaveBeenCalledOnce();
  });

  it("stops a running background fetch when the user starts an action", async () => {
    // Never completes on its own: the fetch is still running when the user acts.
    const { client, emit, asClient } = fakeClient(() => {});
    const { rerender, options } = render(asClient);
    await advance(5 * MINUTE);
    expect(client.backgroundFetch).toHaveBeenCalledOnce();

    rerender({ ...options, busy: true });
    expect(client.cancelTransfer).toHaveBeenCalledExactlyOnceWith(REPO, "background-fetch-1");

    act(() => emit(event("background-fetch-1", "Failed", { errorKind: "Cancelled" })));
    expect(options.refresh).not.toHaveBeenCalled();
    expect(options.notify).not.toHaveBeenCalled();
    expect(lastFetchAt(REPO)).toBeNull();
  });

  it("starts nothing new while a background fetch is still running", async () => {
    const { client, asClient } = fakeClient(() => {});
    render(asClient, { minutes: 5 });

    await advance(60 * MINUTE);

    expect(client.backgroundFetch).toHaveBeenCalledOnce();
  });

  it("tries again an interval later when the backend skipped the tick", async () => {
    const { client, asClient } = fakeClient();
    client.backgroundFetch.mockResolvedValueOnce(null);
    const { options } = render(asClient);

    await advance(5 * MINUTE);
    expect(client.backgroundFetch).toHaveBeenCalledOnce();
    expect(options.refresh).not.toHaveBeenCalled();

    await advance(5 * MINUTE);
    expect(client.backgroundFetch).toHaveBeenCalledTimes(2);
    expect(options.refresh).toHaveBeenCalledOnce();
  });

  it("counts a manual fetch: the next background fetch is an interval after it", async () => {
    const { client, asClient } = fakeClient();
    render(asClient);

    await advance(4 * MINUTE);
    act(() => recordFetch(REPO));
    await advance(1 * MINUTE);
    expect(client.backgroundFetch).not.toHaveBeenCalled();

    await advance(4 * MINUTE);
    expect(client.backgroundFetch).toHaveBeenCalledOnce();
  });

  it("names a remote that needs a credential once, as a notice offering its credentials", async () => {
    const failing = (kind: TransferErrorKind) => (id: string, emit: (progress: TransferProgress) => void) => {
      emit(event(id, "RemoteFailed", { remoteName: "origin", errorKind: kind }));
      emit(event(id, "Failed", { errorKind: kind }));
    };
    const { client, asClient } = fakeClient(failing("MissingCredential"));
    const { options } = render(asClient);

    await advance(5 * MINUTE);
    expect(options.notify).toHaveBeenCalledExactlyOnceWith(
      "Fetch automatically skipped origin: no HTTPS token is available without asking you. Fetch once yourself, or save a token.",
      expect.objectContaining({ label: "Credentials…" }),
    );
    vi.mocked(options.notify).mock.calls[0]![1]!.run();
    expect(options.openCredentials).toHaveBeenCalledWith("origin");
    // Nothing fetched: no refresh, no "last fetch".
    expect(options.refresh).not.toHaveBeenCalled();
    expect(lastFetchAt(REPO)).toBeNull();

    await advance(5 * MINUTE);
    expect(client.backgroundFetch).toHaveBeenCalledTimes(2);
    expect(options.notify).toHaveBeenCalledOnce();
  });

  it("keeps a transport failure quiet and still refreshes for the remotes that were fetched", async () => {
    const { asClient } = fakeClient((id, emit) => {
      emit(event(id, "RemoteFailed", { remoteName: "backup", errorKind: "TransferFailed" }));
      emit(event(id, "Completed"));
    });
    const { options } = render(asClient);

    await advance(5 * MINUTE);

    expect(options.notify).not.toHaveBeenCalled();
    expect(options.refresh).toHaveBeenCalledOnce();
    expect(lastFetchAt(REPO)).not.toBeNull();
  });

  it("handles the completion when it arrives after the fetch started", async () => {
    const { emit, asClient } = fakeClient(() => {});
    const { options } = render(asClient);
    await advance(5 * MINUTE);
    expect(options.refresh).not.toHaveBeenCalled();

    // Another operation's events are not this fetch's.
    act(() => emit({ ...event("fetch-all-9", "Completed"), operation: "FetchAll" }));
    expect(options.refresh).not.toHaveBeenCalled();

    act(() => emit(event("background-fetch-1", "Completed")));
    expect(options.refresh).toHaveBeenCalledOnce();
  });
});

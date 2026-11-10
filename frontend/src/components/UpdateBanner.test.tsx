import { act, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { UpdateBanner } from "./UpdateBanner";
import * as updater from "../ipc/updater";
import { UpdateCheckContext, type UpdateCheck, type UpdateCheckRegistry } from "../state/updateCheck";
import { UpdateSettingsContext, type UpdateSettingsApi, type UpdateSettingsStatus } from "../state/useUpdateSettings";

vi.mock("../ipc/updater", () => ({
  checkForUpdate: vi.fn(),
  relaunchApp: vi.fn(),
}));

describe("UpdateBanner", () => {
  beforeEach(() => {
    vi.mocked(updater.checkForUpdate).mockReset();
    vi.mocked(updater.relaunchApp).mockReset();
  });

  afterEach(() => {
    vi.unstubAllEnvs();
  });

  it("checks for an update in a normal build", async () => {
    vi.stubEnv("VITE_E2E_REPO_PATH", undefined);
    vi.mocked(updater.checkForUpdate).mockResolvedValue(null);

    render(<UpdateBanner />);

    await waitFor(() => expect(updater.checkForUpdate).toHaveBeenCalledTimes(1));
  });

  it("never checks for an update in an E2E build (#576)", async () => {
    vi.stubEnv("VITE_E2E_REPO_PATH", "/tmp/browsitory-e2e-repo");
    vi.mocked(updater.checkForUpdate).mockResolvedValue({
      version: "1.2.3",
      download: vi.fn().mockResolvedValue(undefined),
      installAndRelaunch: vi.fn().mockResolvedValue(undefined),
    });

    render(<UpdateBanner />);

    await act(async () => {
      await Promise.resolve();
    });
    expect(updater.checkForUpdate).not.toHaveBeenCalled();
    expect(screen.queryByRole("status")).toBeNull();
  });

  it("renders nothing when no update is found", async () => {
    vi.mocked(updater.checkForUpdate).mockResolvedValue(null);

    render(<UpdateBanner />);

    await waitFor(() => expect(updater.checkForUpdate).toHaveBeenCalledTimes(1));
    expect(screen.queryByRole("status")).toBeNull();
  });

  it("auto-downloads and shows a restart banner when an update is found", async () => {
    const download = vi.fn().mockResolvedValue(undefined);
    const installAndRelaunch = vi.fn().mockResolvedValue(undefined);
    vi.mocked(updater.checkForUpdate).mockResolvedValue({
      version: "1.2.3",
      download,
      installAndRelaunch,
    });

    render(<UpdateBanner />);

    await waitFor(() => expect(download).toHaveBeenCalledTimes(1));
    expect(await screen.findByText(/1\.2\.3/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /restart/i })).toBeInTheDocument();
  });

  it("calls installAndRelaunch when the restart button is clicked", async () => {
    const download = vi.fn().mockResolvedValue(undefined);
    const installAndRelaunch = vi.fn().mockResolvedValue(undefined);
    vi.mocked(updater.checkForUpdate).mockResolvedValue({
      version: "1.2.3",
      download,
      installAndRelaunch,
    });

    render(<UpdateBanner />);

    const button = await screen.findByRole("button", { name: /restart/i });
    await act(async () => {
      button.click();
    });

    expect(installAndRelaunch).toHaveBeenCalledTimes(1);
  });

  it("swallows a rejection from installAndRelaunch without throwing", async () => {
    const download = vi.fn().mockResolvedValue(undefined);
    const installAndRelaunch = vi.fn().mockRejectedValue(new Error("relaunch failed"));
    vi.mocked(updater.checkForUpdate).mockResolvedValue({
      version: "1.2.3",
      download,
      installAndRelaunch,
    });

    render(<UpdateBanner />);

    const button = await screen.findByRole("button", { name: /restart/i });
    await act(async () => {
      button.click();
    });

    expect(installAndRelaunch).toHaveBeenCalledTimes(1);
    // No unhandled rejection should propagate out of the click handler.
  });

  it("stays hidden when download fails", async () => {
    const download = vi.fn().mockRejectedValue(new Error("download failed"));
    const installAndRelaunch = vi.fn().mockResolvedValue(undefined);
    vi.mocked(updater.checkForUpdate).mockResolvedValue({
      version: "1.2.3",
      download,
      installAndRelaunch,
    });

    render(<UpdateBanner />);

    await waitFor(() => expect(download).toHaveBeenCalledTimes(1));
    expect(screen.queryByRole("button", { name: /restart/i })).toBeNull();
  });
});

describe("UpdateBanner check schedule (#477)", () => {
  const MINUTE = 60 * 1000;
  const HOUR = 60 * MINUTE;

  beforeEach(() => {
    vi.useFakeTimers();
    vi.mocked(updater.checkForUpdate).mockReset();
    vi.spyOn(console, "error").mockImplementation(() => {});
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  // Advances the fake clock and lets the checks it starts settle.
  async function advance(ms: number) {
    await act(async () => {
      await vi.advanceTimersByTimeAsync(ms);
    });
  }

  function foundUpdate(download = vi.fn().mockResolvedValue(undefined)) {
    return { version: "1.2.3", download, installAndRelaunch: vi.fn() };
  }

  const checks = () => vi.mocked(updater.checkForUpdate).mock.calls.length;

  it("checks again only after 6 hours while no update is found", async () => {
    vi.mocked(updater.checkForUpdate).mockResolvedValue(null);

    render(<UpdateBanner />);
    await advance(0);
    expect(checks()).toBe(1);

    await advance(6 * HOUR - 1);
    expect(checks()).toBe(1);
    await advance(1);
    expect(checks()).toBe(2);
  });

  it("retries a failed check after 10 minutes, not 6 hours", async () => {
    vi.mocked(updater.checkForUpdate)
      .mockRejectedValueOnce(new Error("504"))
      .mockResolvedValue(foundUpdate());

    render(<UpdateBanner />);
    await advance(0);
    expect(checks()).toBe(1);
    expect(screen.queryByRole("status")).toBeNull();

    await advance(10 * MINUTE - 1);
    expect(checks()).toBe(1);
    await advance(1);
    expect(checks()).toBe(2);
    expect(screen.getByText(/1\.2\.3/)).toBeInTheDocument();
  });

  it("doubles the retry delay after each failure in a row, up to 3 hours", async () => {
    vi.mocked(updater.checkForUpdate).mockRejectedValue(new Error("504"));

    render(<UpdateBanner />);
    await advance(0);
    expect(checks()).toBe(1);

    // 10, 20, 40, 80 and 160 minutes, then capped at 3 hours.
    const delays = [10, 20, 40, 80, 160, 180, 180].map((m) => m * MINUTE);
    for (const [i, delay] of delays.entries()) {
      await advance(delay - 1);
      expect(checks()).toBe(i + 1);
      await advance(1);
      expect(checks()).toBe(i + 2);
    }
  });

  it("goes back to the 6-hour interval and the 10-minute first retry after a success", async () => {
    vi.mocked(updater.checkForUpdate)
      .mockRejectedValueOnce(new Error("504"))
      .mockRejectedValueOnce(new Error("504"))
      .mockResolvedValueOnce(null)
      .mockRejectedValueOnce(new Error("504"))
      .mockResolvedValue(null);

    render(<UpdateBanner />);
    await advance(0);
    await advance(10 * MINUTE);
    await advance(20 * MINUTE);
    expect(checks()).toBe(3); // the third check succeeds

    await advance(6 * HOUR - 1);
    expect(checks()).toBe(3);
    await advance(1);
    expect(checks()).toBe(4); // fails again

    await advance(10 * MINUTE);
    expect(checks()).toBe(5);
  });

  it("retries a failed download with the same backoff, then shows the banner", async () => {
    const download = vi
      .fn()
      .mockRejectedValueOnce(new Error("download failed"))
      .mockResolvedValue(undefined);
    vi.mocked(updater.checkForUpdate).mockResolvedValue(foundUpdate(download));

    render(<UpdateBanner />);
    await advance(0);
    expect(download).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("status")).toBeNull();

    await advance(10 * MINUTE);
    expect(download).toHaveBeenCalledTimes(2);
    expect(screen.getByRole("button", { name: /restart/i })).toBeInTheDocument();
  });

  it("stops checking once an update is ready", async () => {
    vi.mocked(updater.checkForUpdate).mockResolvedValue(foundUpdate());

    render(<UpdateBanner />);
    await advance(0);
    expect(screen.getByRole("status")).toBeInTheDocument();

    await advance(24 * HOUR);
    expect(checks()).toBe(1);
  });

  it("schedules no more checks after unmounting", async () => {
    vi.mocked(updater.checkForUpdate).mockRejectedValue(new Error("504"));

    const { unmount } = render(<UpdateBanner />);
    await advance(0);
    unmount();

    await advance(24 * HOUR);
    expect(checks()).toBe(1);
  });
});

describe("UpdateBanner with the update settings (#630)", () => {
  const HOUR = 60 * 60 * 1000;

  beforeEach(() => {
    vi.useFakeTimers();
    vi.mocked(updater.checkForUpdate).mockReset();
    vi.spyOn(console, "error").mockImplementation(() => {});
    vi.stubEnv("VITE_E2E_REPO_PATH", undefined);
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
    vi.unstubAllEnvs();
  });

  async function advance(ms: number) {
    await act(async () => {
      await vi.advanceTimersByTimeAsync(ms);
    });
  }

  const checks = () => vi.mocked(updater.checkForUpdate).mock.calls.length;

  function settingsApi(checkAutomatically: boolean, status: UpdateSettingsStatus = "loaded"): UpdateSettingsApi {
    return { settings: { checkAutomatically }, status, update: () => {}, error: null, dismissError: () => {} };
  }

  // Renders the banner with the given settings and returns the check it registers.
  function renderBanner(settings: UpdateSettingsApi) {
    let registered: UpdateCheck | null = null;
    const registry: UpdateCheckRegistry = {
      register(check) {
        registered = check;
        return () => {
          if (registered === check) registered = null;
        };
      },
    };
    const ui = (api: UpdateSettingsApi) => (
      <UpdateSettingsContext.Provider value={api}>
        <UpdateCheckContext.Provider value={registry}>
          <UpdateBanner />
        </UpdateCheckContext.Provider>
      </UpdateSettingsContext.Provider>
    );
    const result = render(ui(settings));
    return {
      checkNow: () => {
        if (registered === null) throw new Error("no check registered");
        return registered();
      },
      rerenderWith: (api: UpdateSettingsApi) => result.rerender(ui(api)),
      unmount: result.unmount,
      isRegistered: () => registered !== null,
    };
  }

  it("checks at start and every 6 hours while the setting is on", async () => {
    vi.mocked(updater.checkForUpdate).mockResolvedValue(null);
    renderBanner(settingsApi(true));
    await advance(0);
    expect(checks()).toBe(1);
    await advance(6 * HOUR);
    expect(checks()).toBe(2);
  });

  it("never checks on its own while the setting is off", async () => {
    vi.mocked(updater.checkForUpdate).mockResolvedValue(null);
    renderBanner(settingsApi(false));
    await advance(48 * HOUR);
    expect(checks()).toBe(0);
  });

  it("doesn't check before the saved setting is read, nor when it can't be read", async () => {
    vi.mocked(updater.checkForUpdate).mockResolvedValue(null);
    const { rerenderWith } = renderBanner(settingsApi(true, "loading"));
    await advance(HOUR);
    expect(checks()).toBe(0);
    rerenderWith(settingsApi(true, "failed"));
    await advance(48 * HOUR);
    expect(checks()).toBe(0);
  });

  it("checks on demand while the setting is off, and shows a found update", async () => {
    vi.mocked(updater.checkForUpdate).mockResolvedValueOnce(null).mockResolvedValue({
      version: "1.2.3",
      download: vi.fn().mockResolvedValue(undefined),
      installAndRelaunch: vi.fn(),
    });
    const { checkNow } = renderBanner(settingsApi(false));

    let outcome: unknown;
    await act(async () => {
      outcome = await checkNow();
    });
    expect(outcome).toEqual({ kind: "none" });
    expect(checks()).toBe(1);

    await act(async () => {
      outcome = await checkNow();
    });
    expect(outcome).toEqual({ kind: "ready", version: "1.2.3" });
    expect(screen.getByRole("button", { name: /restart/i })).toBeInTheDocument();

    // Once an update is ready, asking again doesn't contact the update host.
    await act(async () => {
      outcome = await checkNow();
    });
    expect(outcome).toEqual({ kind: "ready", version: "1.2.3" });
    expect(checks()).toBe(2);
    // ...and nothing was scheduled.
    await advance(48 * HOUR);
    expect(checks()).toBe(2);
  });

  it("reports a failed check on demand with its message", async () => {
    vi.mocked(updater.checkForUpdate).mockRejectedValue(new Error("no endpoint answered"));
    const { checkNow } = renderBanner(settingsApi(false));
    let outcome: unknown;
    await act(async () => {
      outcome = await checkNow();
    });
    expect(outcome).toEqual({ kind: "failed", message: "no endpoint answered" });
  });

  it("checks on demand in an E2E build", async () => {
    vi.stubEnv("VITE_E2E_REPO_PATH", "/tmp/browsitory-e2e-repo");
    vi.mocked(updater.checkForUpdate).mockResolvedValue(null);
    const { checkNow } = renderBanner(settingsApi(true));
    await advance(HOUR);
    expect(checks()).toBe(0);
    await act(async () => {
      await checkNow();
    });
    expect(checks()).toBe(1);
  });

  it("joins a check already running instead of starting a second one", async () => {
    let answer: (value: null) => void = () => {};
    vi.mocked(updater.checkForUpdate).mockReturnValue(new Promise((resolve) => (answer = resolve)));
    const { checkNow } = renderBanner(settingsApi(true));
    await advance(0);
    expect(checks()).toBe(1);

    const manual = checkNow();
    expect(checks()).toBe(1);
    await act(async () => {
      answer(null);
      await manual;
    });
    await expect(manual).resolves.toEqual({ kind: "none" });
    expect(checks()).toBe(1);
  });

  it("starts checking when the setting is turned on and stops when it is turned off", async () => {
    vi.mocked(updater.checkForUpdate).mockResolvedValue(null);
    const { rerenderWith } = renderBanner(settingsApi(false));
    await advance(HOUR);
    expect(checks()).toBe(0);

    rerenderWith(settingsApi(true));
    await advance(0);
    expect(checks()).toBe(1);

    rerenderWith(settingsApi(false));
    await advance(48 * HOUR);
    expect(checks()).toBe(1);
  });

  it("withdraws its check when unmounted", async () => {
    vi.mocked(updater.checkForUpdate).mockResolvedValue(null);
    const { isRegistered, unmount } = renderBanner(settingsApi(false));
    expect(isRegistered()).toBe(true);
    unmount();
    expect(isRegistered()).toBe(false);
  });
});

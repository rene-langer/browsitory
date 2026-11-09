import { act, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { UpdateBanner } from "./UpdateBanner";
import * as updater from "../ipc/updater";

vi.mock("../ipc/updater", () => ({
  checkForUpdate: vi.fn(),
  relaunchApp: vi.fn(),
}));

describe("UpdateBanner", () => {
  beforeEach(() => {
    vi.mocked(updater.checkForUpdate).mockReset();
    vi.mocked(updater.relaunchApp).mockReset();
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

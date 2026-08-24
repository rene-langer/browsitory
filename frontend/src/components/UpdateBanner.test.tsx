import { act, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
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

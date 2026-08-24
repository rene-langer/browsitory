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
    const install = vi.fn().mockResolvedValue(undefined);
    vi.mocked(updater.checkForUpdate).mockResolvedValue({ version: "1.2.3", install });

    render(<UpdateBanner />);

    await waitFor(() => expect(install).toHaveBeenCalledTimes(1));
    expect(await screen.findByText(/1\.2\.3/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /restart/i })).toBeInTheDocument();
  });

  it("calls relaunchApp when the restart button is clicked", async () => {
    const install = vi.fn().mockResolvedValue(undefined);
    vi.mocked(updater.checkForUpdate).mockResolvedValue({ version: "1.2.3", install });
    vi.mocked(updater.relaunchApp).mockResolvedValue(undefined);

    render(<UpdateBanner />);

    const button = await screen.findByRole("button", { name: /restart/i });
    await act(async () => {
      button.click();
    });

    expect(updater.relaunchApp).toHaveBeenCalledTimes(1);
  });

  it("stays hidden when download fails", async () => {
    const install = vi.fn().mockRejectedValue(new Error("download failed"));
    vi.mocked(updater.checkForUpdate).mockResolvedValue({ version: "1.2.3", install });

    render(<UpdateBanner />);

    await waitFor(() => expect(install).toHaveBeenCalledTimes(1));
    expect(screen.queryByRole("button", { name: /restart/i })).toBeNull();
  });
});

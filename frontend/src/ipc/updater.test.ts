import { beforeEach, describe, expect, it, vi } from "vitest";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { checkForUpdate, relaunchApp } from "./updater";

vi.mock("@tauri-apps/plugin-updater", () => ({
  check: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-process", () => ({
  relaunch: vi.fn(),
}));

describe("checkForUpdate", () => {
  beforeEach(() => {
    vi.mocked(check).mockReset();
  });

  it("returns null when no update is available", async () => {
    vi.mocked(check).mockResolvedValue(null);

    const result = await checkForUpdate();

    expect(result).toBeNull();
  });

  it("returns version and an install function when an update is found", async () => {
    const downloadAndInstall = vi.fn().mockResolvedValue(undefined);
    vi.mocked(check).mockResolvedValue({
      version: "1.2.3",
      downloadAndInstall,
    } as never);

    const result = await checkForUpdate();

    expect(result?.version).toBe("1.2.3");
    await result?.install();
    expect(downloadAndInstall).toHaveBeenCalledTimes(1);
  });

  it("returns null when the check rejects", async () => {
    vi.mocked(check).mockRejectedValue(new Error("network error"));

    const result = await checkForUpdate();

    expect(result).toBeNull();
  });
});

describe("relaunchApp", () => {
  it("calls the plugin's relaunch", async () => {
    vi.mocked(relaunch).mockResolvedValue(undefined);

    await relaunchApp();

    expect(relaunch).toHaveBeenCalledTimes(1);
  });
});

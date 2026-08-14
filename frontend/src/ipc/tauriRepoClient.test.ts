import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { tauriRepoClient } from "./tauriRepoClient";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("tauriRepoClient remote URL validation", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockClear();
  });

  it.each([
    [
      "fetch URL passed to addRemote",
      () => tauriRepoClient.addRemote("origin", "https://user:secret@example.com/repo.git", null),
    ],
    [
      "push URL passed to updateRemoteUrls",
      () =>
        tauriRepoClient.updateRemoteUrls(
          "origin",
          "https://example.com/repo.git",
          "HTTPS://user@example.com/repo.git",
        ),
    ],
  ])("rejects HTTP(S) userinfo in the %s before invoking Tauri", async (_description, operation) => {
    await expect(async () => operation()).rejects.toThrow(
      "Remote URLs must not contain embedded credentials",
    );
    expect(invoke).not.toHaveBeenCalled();
  });
});

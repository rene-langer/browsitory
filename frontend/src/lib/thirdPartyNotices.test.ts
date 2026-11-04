import { describe, expect, it, vi } from "vitest";
import { THIRD_PARTY_NOTICES_FILE, thirdPartyNoticesLoader, thirdPartyNoticesUrl } from "./thirdPartyNotices";

describe("thirdPartyNoticesUrl", () => {
  it("resolves next to the page when there is no classic script (Tauri's module entry)", () => {
    expect(thirdPartyNoticesUrl(null, "tauri://localhost/index.html")).toBe(
      `tauri://localhost/${THIRD_PARTY_NOTICES_FILE}`,
    );
  });

  it("resolves at the build root, one folder above the classic script (VSCode webview)", () => {
    const script = { src: "https://file+.vscode-resource.vscode-cdn.net/ext/webview/assets/vscode-main.js" };
    expect(thirdPartyNoticesUrl(script, "vscode-webview://abc/index.html")).toBe(
      `https://file+.vscode-resource.vscode-cdn.net/ext/webview/${THIRD_PARTY_NOTICES_FILE}`,
    );
  });
});

describe("thirdPartyNoticesLoader", () => {
  it("fetches the file once and returns its text", async () => {
    const fetchImpl = vi.fn(async () => new Response("notices text", { status: 200 }));
    const load = thirdPartyNoticesLoader("https://host/notices.md", fetchImpl);

    await expect(load()).resolves.toBe("notices text");
    await expect(load()).resolves.toBe("notices text");
    expect(fetchImpl).toHaveBeenCalledTimes(1);
    expect(fetchImpl).toHaveBeenCalledWith("https://host/notices.md");
  });

  it("rejects on an HTTP error and fetches again on the next call", async () => {
    const fetchImpl = vi
      .fn<(url: string) => Promise<Response>>()
      .mockResolvedValueOnce(new Response("", { status: 404, statusText: "Not Found" }))
      .mockResolvedValueOnce(new Response("ok", { status: 200 }));
    const load = thirdPartyNoticesLoader("https://host/notices.md", fetchImpl);

    await expect(load()).rejects.toThrow("Could not load the licences (HTTP 404)");
    await expect(load()).resolves.toBe("ok");
  });
});

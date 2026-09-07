import { afterEach, describe, expect, it, vi } from "vitest";
import type { RepoClient } from "../ipc/RepoClient";
import { installGlobalErrorLogging } from "./logger";

function fakeClient(): RepoClient {
  return {
    logFrontendError: vi.fn(async () => {}),
  } as unknown as RepoClient;
}

describe("installGlobalErrorLogging", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("routes uncaught errors and unhandled rejections through RepoClient.logFrontendError", () => {
    const client = fakeClient();
    const dispose = installGlobalErrorLogging(client);
    const uncaught = new Error("render failed");
    const rejected = new Error("request failed");

    window.dispatchEvent(new ErrorEvent("error", { error: uncaught, message: uncaught.message }));
    const rejectionEvent = new Event("unhandledrejection");
    Object.defineProperty(rejectionEvent, "reason", { value: rejected });
    window.dispatchEvent(rejectionEvent);

    expect(client.logFrontendError).toHaveBeenNthCalledWith(1, "Uncaught error", uncaught);
    expect(client.logFrontendError).toHaveBeenNthCalledWith(2, "Unhandled rejection", rejected);

    dispose();
  });

  it("stops listening once disposed", () => {
    const client = fakeClient();
    const dispose = installGlobalErrorLogging(client);
    dispose();

    // Once disposed, nothing is left listening for `error` on `window`, so jsdom would
    // otherwise report this as an unhandled exception; a throwaway listener that calls
    // `preventDefault` suppresses that default reporting, matching real browser semantics.
    const safetyNet = (event: ErrorEvent) => event.preventDefault();
    window.addEventListener("error", safetyNet);
    window.dispatchEvent(new ErrorEvent("error", { error: new Error("after dispose") }));
    window.removeEventListener("error", safetyNet);

    expect(client.logFrontendError).not.toHaveBeenCalled();
  });
});

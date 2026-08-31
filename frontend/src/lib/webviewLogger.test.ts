import { afterEach, describe, expect, it, vi } from "vitest";
import { installWebviewErrorLogging } from "./webviewLogger";

describe("webview error logging", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("reports global failures through the browser console", () => {
    const log = vi.spyOn(console, "error").mockImplementation(() => {});
    const dispose = installWebviewErrorLogging();
    const uncaught = new Error("render failed");
    const rejected = new Error("request failed");

    window.dispatchEvent(
      new ErrorEvent("error", { error: uncaught, message: uncaught.message }),
    );
    const rejectionEvent = new Event("unhandledrejection");
    Object.defineProperty(rejectionEvent, "reason", { value: rejected });
    window.dispatchEvent(rejectionEvent);

    expect(log).toHaveBeenNthCalledWith(1, "Uncaught error", uncaught);
    expect(log).toHaveBeenNthCalledWith(2, "Unhandled rejection", rejected);

    dispose();
  });
});

import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useClipboardFeedback } from "./useCopyToClipboard";

const stubClipboard = (writeText: (text: string) => Promise<void>) =>
  Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });

describe("useClipboardFeedback (#614)", () => {
  it("confirms a copy with a toast", async () => {
    const writeText = vi.fn(async () => {});
    stubClipboard(writeText);
    const pushToast = vi.fn();
    const { result } = renderHook(() => useClipboardFeedback(pushToast));

    act(() => result.current.copy("feat/foo", "branch name"));

    expect(writeText).toHaveBeenCalledWith("feat/foo");
    await waitFor(() => expect(pushToast).toHaveBeenCalledWith("Copied branch name"));
    expect(result.current.error).toBeNull();
  });

  it("shows an error instead of a toast when the clipboard refuses, until dismissed or the next copy", async () => {
    stubClipboard(vi.fn(async () => Promise.reject(new Error("Write permission denied."))));
    const pushToast = vi.fn();
    const { result } = renderHook(() => useClipboardFeedback(pushToast));

    act(() => result.current.copy("v1.0", "tag name"));
    await waitFor(() =>
      expect(result.current.error).toBe("Couldn't copy the tag name: Write permission denied."),
    );
    expect(pushToast).not.toHaveBeenCalled();

    act(() => result.current.dismissError());
    expect(result.current.error).toBeNull();

    act(() => result.current.copy("v1.0", "tag name"));
    await waitFor(() => expect(result.current.error).not.toBeNull());
    stubClipboard(vi.fn(async () => {}));
    act(() => result.current.copy("v1.0", "tag name"));
    expect(result.current.error).toBeNull();
    await waitFor(() => expect(pushToast).toHaveBeenCalledWith("Copied tag name"));
  });
});

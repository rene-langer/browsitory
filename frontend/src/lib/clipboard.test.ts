import { afterEach, describe, expect, it, vi } from "vitest";
import { copyText } from "./clipboard";

const original = Object.getOwnPropertyDescriptor(navigator, "clipboard");
const stubClipboard = (value: unknown) =>
  Object.defineProperty(navigator, "clipboard", { value, configurable: true });

describe("copyText (#614)", () => {
  afterEach(() => {
    if (original !== undefined) Object.defineProperty(navigator, "clipboard", original);
    else stubClipboard(undefined);
  });

  it("writes the text as plain text", async () => {
    const writeText = vi.fn(async () => {});
    stubClipboard({ writeText });
    await copyText("feat/foo");
    expect(writeText).toHaveBeenCalledWith("feat/foo");
  });

  it("rejects when the clipboard refuses the write", async () => {
    stubClipboard({ writeText: vi.fn(async () => Promise.reject(new Error("Write permission denied."))) });
    await expect(copyText("x")).rejects.toThrow("Write permission denied.");
  });

  it("rejects when there is no clipboard", async () => {
    stubClipboard(undefined);
    await expect(copyText("x")).rejects.toThrow("The clipboard isn't available.");
  });
});

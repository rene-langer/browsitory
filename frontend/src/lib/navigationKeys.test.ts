import { describe, expect, it } from "vitest";
import { navigationKeyDirection, navigationKeyLabels } from "./navigationKeys";

const key = (key: string, mods: Partial<Record<"altKey" | "ctrlKey" | "metaKey" | "shiftKey", boolean>> = {}) => ({
  key,
  altKey: false,
  ctrlKey: false,
  metaKey: false,
  shiftKey: false,
  ...mods,
});

describe("Back/Forward keys (#618)", () => {
  it("are Alt+Left / Alt+Right outside macOS", () => {
    expect(navigationKeyDirection(key("ArrowLeft", { altKey: true }), false)).toBe(-1);
    expect(navigationKeyDirection(key("ArrowRight", { altKey: true }), false)).toBe(1);
    expect(navigationKeyDirection(key("ArrowLeft"), false)).toBeNull();
    expect(navigationKeyDirection(key("ArrowLeft", { altKey: true, shiftKey: true }), false)).toBeNull();
    expect(navigationKeyDirection(key("ArrowLeft", { altKey: true, ctrlKey: true }), false)).toBeNull();
    expect(navigationKeyDirection(key("[", { metaKey: true }), false)).toBeNull();
  });

  it("are Cmd+[ / Cmd+] on macOS, where Alt+Left moves by word", () => {
    expect(navigationKeyDirection(key("[", { metaKey: true }), true)).toBe(-1);
    expect(navigationKeyDirection(key("]", { metaKey: true }), true)).toBe(1);
    expect(navigationKeyDirection(key("ArrowLeft", { altKey: true }), true)).toBeNull();
    expect(navigationKeyDirection(key("["), true)).toBeNull();
  });

  it("are named for the platform", () => {
    expect(navigationKeyLabels(false)).toEqual({ back: "Alt+Left", forward: "Alt+Right" });
    expect(navigationKeyLabels(true)).toEqual({ back: "⌘[", forward: "⌘]" });
  });
});

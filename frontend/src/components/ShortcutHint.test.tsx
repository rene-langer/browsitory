import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ShortcutHint } from "./ShortcutHint";

describe("ShortcutHint", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("advertises Ctrl+K on non-Apple platforms", () => {
    vi.stubGlobal("navigator", { platform: "Linux x86_64", userAgent: "X11; Linux" });
    render(<ShortcutHint />);
    const hint = screen.getByText("Ctrl+K");
    expect(hint.tagName).toBe("KBD");
    expect(hint.parentElement).toHaveAttribute("title", "Open the command palette");
  });

  it("advertises ⌘K on Apple platforms", () => {
    vi.stubGlobal("navigator", { platform: "MacIntel", userAgent: "Macintosh" });
    render(<ShortcutHint />);
    expect(screen.getByText("⌘K")).toBeInTheDocument();
  });
});

import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ShortcutHint } from "./ShortcutHint";

describe("ShortcutHint", () => {
  it("advertises Ctrl/Cmd+K for the command palette", () => {
    render(<ShortcutHint />);
    const hint = screen.getByText("Ctrl/Cmd+K");
    expect(hint.tagName).toBe("KBD");
    expect(hint.parentElement).toHaveAttribute("title", "Open the command palette");
  });
});

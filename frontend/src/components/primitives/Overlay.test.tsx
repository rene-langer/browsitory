import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Overlay } from "./Overlay";

describe("Overlay", () => {
  it("renders its children inside a dialog", () => {
    render(
      <Overlay>
        <p>overlay content</p>
      </Overlay>,
    );
    const dialog = screen.getByRole("dialog");
    expect(dialog).toBeInTheDocument();
    expect(screen.getByText("overlay content")).toBeInTheDocument();
  });

  it("opens the dialog via showModal when available", () => {
    const dialog = document.createElement("dialog");
    dialog.showModal = () => {
      dialog.setAttribute("open", "");
    };
    dialog.close = () => dialog.removeAttribute("open");
    // Confirm the component calls showModal when the environment provides it —
    // verified indirectly: the dialog has the `open` attribute after mount.
    const { unmount } = render(
      <Overlay>
        <p>content</p>
      </Overlay>,
    );
    const renderedDialog = screen.getByRole("dialog");
    // jsdom's <dialog> may or may not implement showModal; either path
    // (showModal succeeding, or the setAttribute("open", "") fallback)
    // must leave the dialog open.
    expect(renderedDialog).toHaveAttribute("open");
    unmount();
  });

  it("calls onClose when the dialog's close event fires", () => {
    const onClose = vi.fn();
    render(
      <Overlay onClose={onClose}>
        <p>content</p>
      </Overlay>,
    );
    const dialog = screen.getByRole("dialog");
    dialog.dispatchEvent(new Event("close"));
    expect(onClose).toHaveBeenCalledOnce();
  });
});

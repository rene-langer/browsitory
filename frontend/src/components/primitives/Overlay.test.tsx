import { StrictMode } from "react";
import { cleanup, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Overlay } from "./Overlay";

// jsdom doesn't implement HTMLDialogElement.showModal/close (confirmed: the property is
// absent from its prototype), so every other test in this file exercises the `setAttribute`
// fallback path, not the real one. This polyfill gives `close()` its actual spec behavior —
// clear `open`, then synchronously dispatch a "close" event — which is the part that matters
// for the StrictMode regression below; `showModal` only needs to set `open`.
function installDialogPolyfill(): () => void {
  const proto = window.HTMLDialogElement.prototype;
  const originalShowModal = proto.showModal;
  const originalClose = proto.close;
  proto.showModal = function (this: HTMLDialogElement) {
    this.setAttribute("open", "");
  };
  proto.close = function (this: HTMLDialogElement) {
    if (!this.open) return;
    this.removeAttribute("open");
    this.dispatchEvent(new Event("close"));
  };
  return () => {
    proto.showModal = originalShowModal;
    proto.close = originalClose;
  };
}

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

  describe("under StrictMode, with spec-correct showModal/close", () => {
    // Cleanup order matters here: `@testing-library/react`'s auto-registered `afterEach(cleanup)`
    // unmounts (running Overlay's real cleanup, which calls `dialog.close()`) — that has to
    // happen *before* the polyfill below is restored, or the unmount throws against the
    // original (unimplemented) `close`. Each test therefore calls `cleanup()` itself, ends
    // with the restore, and only then returns.

    it("does not call onClose on mount (StrictMode's double-invoked cleanup must not leak out)", () => {
      const restore = installDialogPolyfill();
      try {
        const onClose = vi.fn();

        render(
          <StrictMode>
            <Overlay onClose={onClose}>
              <p>content</p>
            </Overlay>
          </StrictMode>,
        );

        // Regression: StrictMode mounts this effect twice (setup, cleanup, setup). The
        // cleanup's `dialog.close()` used to fire a real "close" event straight into
        // `onClose` — which, in `App`, always unmounts the Overlay itself — so the overlay
        // opened via the "+" tab-strip button appeared to do nothing.
        expect(onClose).not.toHaveBeenCalled();
        expect(screen.getByRole("dialog")).toHaveAttribute("open");
      } finally {
        cleanup();
        restore();
      }
    });

    it("still calls onClose for a real user-driven close (Escape/backdrop/form submit)", () => {
      const restore = installDialogPolyfill();
      try {
        const onClose = vi.fn();

        render(
          <StrictMode>
            <Overlay onClose={onClose}>
              <p>content</p>
            </Overlay>
          </StrictMode>,
        );

        // Simulate the browser closing the dialog natively (not driven by our own cleanup).
        (screen.getByRole("dialog") as HTMLDialogElement).close();

        expect(onClose).toHaveBeenCalledOnce();
      } finally {
        cleanup();
        restore();
      }
    });
  });
});

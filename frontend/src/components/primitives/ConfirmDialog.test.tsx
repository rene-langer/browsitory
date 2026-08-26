import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ConfirmDialog } from "./ConfirmDialog";

describe("ConfirmDialog", () => {
  it("renders as an accessible dialog with the given message and button labels", () => {
    render(
      <ConfirmDialog
        ariaLabel="Remove thing"
        message={<p>Remove thing?</p>}
        confirmLabel="Remove"
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    const dialog = screen.getByRole("dialog", { name: "Remove thing" });
    expect(within(dialog).getByText("Remove thing?")).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Remove" })).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeInTheDocument();
  });

  it("supports a custom cancel label", () => {
    render(
      <ConfirmDialog
        ariaLabel="Remove thing"
        message="Remove thing?"
        confirmLabel="Remove"
        cancelLabel="Never mind"
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    expect(screen.getByRole("button", { name: "Never mind" })).toBeInTheDocument();
  });

  it("calls onConfirm when the confirm button is clicked", () => {
    const onConfirm = vi.fn();
    render(
      <ConfirmDialog
        ariaLabel="Remove thing"
        message="Remove thing?"
        confirmLabel="Remove"
        onConfirm={onConfirm}
        onCancel={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Remove" }));

    expect(onConfirm).toHaveBeenCalledOnce();
  });

  it("calls onCancel when the cancel button is clicked", () => {
    const onCancel = vi.fn();
    render(
      <ConfirmDialog
        ariaLabel="Remove thing"
        message="Remove thing?"
        confirmLabel="Remove"
        onConfirm={vi.fn()}
        onCancel={onCancel}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(onCancel).toHaveBeenCalledOnce();
  });

  it("calls onCancel (not the browser default) when the dialog's cancel event fires (Escape)", () => {
    const onCancel = vi.fn();
    render(
      <ConfirmDialog
        ariaLabel="Remove thing"
        message="Remove thing?"
        confirmLabel="Remove"
        onConfirm={vi.fn()}
        onCancel={onCancel}
      />,
    );

    const dialog = screen.getByRole("dialog", { name: "Remove thing" });
    fireEvent(dialog, new Event("cancel", { cancelable: true }));

    expect(onCancel).toHaveBeenCalledOnce();
  });

  it("autofocuses the cancel button on mount", async () => {
    render(
      <ConfirmDialog
        ariaLabel="Remove thing"
        message="Remove thing?"
        confirmLabel="Remove"
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus();
    });
  });

  it("disables the confirm button when confirmDisabled is true", () => {
    render(
      <ConfirmDialog
        ariaLabel="Remove thing"
        message="Remove thing?"
        confirmLabel="Remove"
        confirmDisabled
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    expect(screen.getByRole("button", { name: "Remove" })).toBeDisabled();
  });

  it("opens the dialog (via showModal when available, else the open attribute)", () => {
    // jsdom's <dialog> may or may not implement showModal; either path (showModal succeeding,
    // or the setAttribute("open", "") fallback) must leave the dialog open — mirrors
    // Overlay.test.tsx's equivalent assertion for the same reasoning.
    render(
      <ConfirmDialog
        ariaLabel="Remove thing"
        message="Remove thing?"
        confirmLabel="Remove"
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    expect(screen.getByRole("dialog", { name: "Remove thing" })).toHaveAttribute("open");
  });
});

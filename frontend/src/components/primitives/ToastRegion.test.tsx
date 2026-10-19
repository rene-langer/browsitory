import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ToastRegion } from "./ToastRegion";

describe("ToastRegion", () => {
  it("is an always-present polite live region", () => {
    render(<ToastRegion toasts={[]} onDismiss={vi.fn()} />);
    expect(screen.getByRole("status", { name: "Notifications" })).toHaveAttribute("aria-live", "polite");
  });

  it("shows messages and dismisses them", () => {
    const onDismiss = vi.fn();
    render(<ToastRegion toasts={[{ id: 7, message: "Committed" }]} onDismiss={onDismiss} />);
    expect(screen.getByText("Committed")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Dismiss notification" }));
    expect(onDismiss).toHaveBeenCalledWith(7);
  });

  it("renders an action button that runs the action and dismisses the toast", () => {
    const run = vi.fn();
    const onDismiss = vi.fn();
    render(
      <ToastRegion toasts={[{ id: 1, message: "Undid Stage a.txt", action: { label: "Redo", run } }]} onDismiss={onDismiss} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Redo" }));
    expect(run).toHaveBeenCalledOnce();
    expect(onDismiss).toHaveBeenCalledWith(1);
  });
});

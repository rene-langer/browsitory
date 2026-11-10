import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { Toast } from "../../state/useToasts";
import { ToastRegion } from "./ToastRegion";

const toast = (overrides: Partial<Toast> = {}): Toast => ({ id: 7, message: "Committed", tone: "success", ...overrides });

describe("ToastRegion", () => {
  it("is an always-present polite live region", () => {
    render(<ToastRegion toasts={[]} onDismiss={vi.fn()} onPauseChange={vi.fn()} />);
    expect(screen.getByRole("status", { name: "Notifications" })).toHaveAttribute("aria-live", "polite");
  });

  it("shows messages and dismisses them", () => {
    const onDismiss = vi.fn();
    render(<ToastRegion toasts={[toast()]} onDismiss={onDismiss} onPauseChange={vi.fn()} />);
    expect(screen.getByText("Committed")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Dismiss notification" }));
    expect(onDismiss).toHaveBeenCalledWith(7);
  });

  it("renders an action button that runs the action and dismisses the toast", () => {
    const run = vi.fn();
    const onDismiss = vi.fn();
    render(
      <ToastRegion
        toasts={[toast({ id: 1, message: "Undid Stage a.txt", action: { label: "Redo", run } })]}
        onDismiss={onDismiss}
        onPauseChange={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Redo" }));
    expect(run).toHaveBeenCalledOnce();
    expect(onDismiss).toHaveBeenCalledWith(1);
  });

  it("marks each toast with its tone, and says so in words for warnings and errors", () => {
    render(
      <ToastRegion
        toasts={[
          toast({ id: 1, message: "Pulled origin/main", tone: "success" }),
          toast({ id: 2, message: "Checking for updates", tone: "info" }),
          toast({ id: 3, message: "Fetch from origin failed", tone: "warning" }),
          toast({ id: 4, message: "Push failed", tone: "error" }),
        ]}
        onDismiss={vi.fn()}
        onPauseChange={vi.fn()}
      />,
    );
    const items = screen.getAllByTestId("toast");
    expect(items.map((item) => item.dataset.tone)).toEqual(["success", "info", "warning", "error"]);
    expect(items[2]).toHaveTextContent("Warning: Fetch from origin failed");
    expect(items[3]).toHaveTextContent("Error: Push failed");
    expect(items[0]).toHaveTextContent(/^Pulled origin\/main/);
  });

  it("pauses while the pointer is over a toast", () => {
    const onPauseChange = vi.fn();
    render(<ToastRegion toasts={[toast()]} onDismiss={vi.fn()} onPauseChange={onPauseChange} />);
    fireEvent.mouseEnter(screen.getByTestId("toast"));
    expect(onPauseChange).toHaveBeenLastCalledWith(true);
    fireEvent.mouseLeave(screen.getByTestId("toast"));
    expect(onPauseChange).toHaveBeenLastCalledWith(false);
  });

  it("pauses while focus is in a toast, and never takes focus itself", () => {
    const onPauseChange = vi.fn();
    render(
      <>
        <button type="button">Elsewhere</button>
        <ToastRegion toasts={[toast({ action: { label: "Undo", run: vi.fn() } })]} onDismiss={vi.fn()} onPauseChange={onPauseChange} />
      </>,
    );
    expect(screen.getByRole("button", { name: "Undo" })).not.toHaveFocus();
    act(() => screen.getByRole("button", { name: "Undo" }).focus());
    expect(onPauseChange).toHaveBeenLastCalledWith(true);
    act(() => screen.getByRole("button", { name: "Dismiss notification" }).focus());
    expect(onPauseChange).toHaveBeenLastCalledWith(true);
    act(() => screen.getByRole("button", { name: "Elsewhere" }).focus());
    expect(onPauseChange).toHaveBeenLastCalledWith(false);
  });

  it("resumes once the last toast is gone, even if the pointer or focus never left", () => {
    const onPauseChange = vi.fn();
    const { rerender } = render(<ToastRegion toasts={[toast()]} onDismiss={vi.fn()} onPauseChange={onPauseChange} />);
    fireEvent.mouseEnter(screen.getByTestId("toast"));
    act(() => screen.getByRole("button", { name: "Dismiss notification" }).focus());
    expect(onPauseChange).toHaveBeenLastCalledWith(true);
    rerender(<ToastRegion toasts={[]} onDismiss={vi.fn()} onPauseChange={onPauseChange} />);
    expect(onPauseChange).toHaveBeenLastCalledWith(false);
  });
});

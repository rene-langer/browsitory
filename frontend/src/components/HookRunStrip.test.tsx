import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { HookRunStrip } from "./HookRunStrip";

describe("HookRunStrip", () => {
  it("shows a checkout's running post-checkout hook with Cancel", () => {
    const onCancel = vi.fn();
    render(
      <HookRunStrip running={{ operationId: "switch-1", hook: "post-checkout", source: "Checkout" }} onCancel={onCancel} />,
    );

    const strip = screen.getByRole("status", { name: "Hook status" });
    expect(strip).toHaveTextContent("Running post-checkout hook…");
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it("shows merge and pick hooks too", () => {
    const { rerender } = render(
      <HookRunStrip running={{ operationId: "merge-1", hook: "post-merge", source: "Merge" }} onCancel={() => {}} />,
    );
    expect(screen.getByRole("status", { name: "Hook status" })).toHaveTextContent("Running post-merge hook…");

    rerender(<HookRunStrip running={{ operationId: "pick-2", hook: "signing", source: "Pick" }} onCancel={() => {}} />);
    expect(screen.getByRole("status", { name: "Hook status" })).toHaveTextContent("Waiting for the signing program…");
  });

  it("renders nothing for a commit's hooks or when no hook runs", () => {
    const { container, rerender } = render(
      <HookRunStrip running={{ operationId: "commit-1", hook: "pre-commit", source: "Commit" }} onCancel={() => {}} />,
    );
    expect(container).toBeEmptyDOMElement();

    rerender(<HookRunStrip running={null} onCancel={() => {}} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("latches Cancel for the operation and re-arms for the next one", () => {
    const onCancel = vi.fn();
    const { rerender } = render(
      <HookRunStrip running={{ operationId: "switch-1", hook: "post-checkout", source: "Checkout" }} onCancel={onCancel} />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    const latched = screen.getByRole("button", { name: "Cancelling…" });
    expect(latched).toBeDisabled();
    fireEvent.click(latched);
    expect(onCancel).toHaveBeenCalledTimes(1);

    rerender(
      <HookRunStrip running={{ operationId: "switch-2", hook: "post-checkout", source: "Checkout" }} onCancel={onCancel} />,
    );
    expect(screen.getByRole("button", { name: "Cancel" })).toBeEnabled();
  });
});

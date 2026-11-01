import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { HookRunStrip } from "./HookRunStrip";

describe("HookRunStrip", () => {
  it("shows a checkout's running post-checkout hook with a named Cancel", () => {
    const onCancel = vi.fn();
    render(
      <HookRunStrip running={{ operationId: "switch-1", hook: "post-checkout", source: "Checkout" }} onCancel={onCancel} />,
    );

    const strip = screen.getByRole("status", { name: "Hook status" });
    expect(strip).toHaveTextContent("Running post-checkout hook…");
    const cancel = screen.getByRole("button", { name: "Cancel post-checkout hook" });
    expect(cancel).toHaveTextContent("Cancel");
    fireEvent.click(cancel);
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it("shows merge and pick hooks too", () => {
    const { rerender } = render(
      <HookRunStrip running={{ operationId: "merge-1", hook: "post-merge", source: "Merge" }} onCancel={() => {}} />,
    );
    expect(screen.getByRole("status", { name: "Hook status" })).toHaveTextContent("Running post-merge hook…");

    rerender(<HookRunStrip running={{ operationId: "pick-2", hook: "signing", source: "Pick" }} onCancel={() => {}} />);
    expect(screen.getByRole("status", { name: "Hook status" })).toHaveTextContent("Waiting for the signing program…");
    expect(screen.getByRole("button", { name: "Cancel signing" })).toBeEnabled();
  });

  it("keeps an empty live region mounted for a commit's hooks or when no hook runs", () => {
    const { rerender } = render(
      <HookRunStrip running={{ operationId: "commit-1", hook: "pre-commit", source: "Commit" }} onCancel={() => {}} />,
    );
    const strip = screen.getByRole("status", { name: "Hook status" });
    expect(strip).toBeEmptyDOMElement();

    rerender(<HookRunStrip running={null} onCancel={() => {}} />);
    expect(screen.getByRole("status", { name: "Hook status" })).toBe(strip);
    expect(strip).toBeEmptyDOMElement();

    rerender(
      <HookRunStrip running={{ operationId: "switch-1", hook: "post-checkout", source: "Checkout" }} onCancel={() => {}} />,
    );
    // The same region announces the new text.
    expect(screen.getByRole("status", { name: "Hook status" })).toBe(strip);
    expect(strip).toHaveTextContent("Running post-checkout hook…");
  });

  it("latches Cancel for the operation and re-arms for the next one", () => {
    const onCancel = vi.fn();
    const { rerender } = render(
      <HookRunStrip running={{ operationId: "switch-1", hook: "post-checkout", source: "Checkout" }} onCancel={onCancel} />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Cancel post-checkout hook" }));
    const latched = screen.getByRole("button", { name: "Cancel post-checkout hook" });
    expect(latched).toHaveTextContent("Cancelling…");
    expect(latched).toBeDisabled();
    fireEvent.click(latched);
    expect(onCancel).toHaveBeenCalledTimes(1);

    rerender(
      <HookRunStrip running={{ operationId: "switch-2", hook: "post-checkout", source: "Checkout" }} onCancel={onCancel} />,
    );
    expect(screen.getByRole("button", { name: "Cancel post-checkout hook" })).toBeEnabled();
  });

  it("says the hook was cancelled and moves focus to that visible text when it ends", () => {
    const { rerender } = render(
      <HookRunStrip running={{ operationId: "switch-1", hook: "post-checkout", source: "Checkout" }} onCancel={() => {}} />,
    );
    const cancel = screen.getByRole("button", { name: "Cancel post-checkout hook" });
    cancel.focus();
    fireEvent.click(cancel);

    rerender(<HookRunStrip running={null} onCancel={() => {}} />);

    const strip = screen.getByRole("status", { name: "Hook status" });
    expect(strip).toHaveTextContent("The post-checkout hook was cancelled.");
    expect(document.activeElement).toBe(strip);

    // The note goes once focus moves on.
    fireEvent.blur(strip);
    expect(strip).toBeEmptyDOMElement();
  });

  it("drops the cancelled note when the next hook starts", () => {
    const { rerender } = render(
      <HookRunStrip running={{ operationId: "switch-1", hook: "post-checkout", source: "Checkout" }} onCancel={() => {}} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Cancel post-checkout hook" }));
    rerender(<HookRunStrip running={null} onCancel={() => {}} />);
    expect(screen.getByRole("status", { name: "Hook status" })).toHaveTextContent("The post-checkout hook was cancelled.");

    rerender(<HookRunStrip running={{ operationId: "merge-2", hook: "post-merge", source: "Merge" }} onCancel={() => {}} />);

    expect(screen.getByRole("status", { name: "Hook status" })).toHaveTextContent("Running post-merge hook…");
    expect(screen.queryByText("The post-checkout hook was cancelled.")).toBeNull();
  });

  it("shows no cancelled note for a hook that ended on its own", () => {
    const { rerender } = render(
      <HookRunStrip running={{ operationId: "switch-1", hook: "post-checkout", source: "Checkout" }} onCancel={() => {}} />,
    );
    rerender(<HookRunStrip running={null} onCancel={() => {}} />);
    expect(screen.getByRole("status", { name: "Hook status" })).toBeEmptyDOMElement();
  });
});

import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { InlineError } from "./InlineError";

describe("InlineError", () => {
  it("renders the message as an alert", () => {
    render(<InlineError message="Something failed" onDismiss={() => {}} />);
    expect(screen.getByRole("alert")).toHaveTextContent("Something failed");
  });

  it("calls onDismiss when the dismiss button is clicked", () => {
    const onDismiss = vi.fn();
    render(<InlineError message="Something failed" onDismiss={onDismiss} />);
    fireEvent.click(screen.getByRole("button", { name: "Dismiss error" }));
    expect(onDismiss).toHaveBeenCalledTimes(1);
  });

  it("offers Retry and a next-step hint when provided", () => {
    const onRetry = vi.fn();
    render(<InlineError message="Boom" hint="Check your connection." onRetry={onRetry} onDismiss={() => {}} />);
    expect(screen.getByText("Check your connection.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(onRetry).toHaveBeenCalledTimes(1);
  });

  it("omits Retry by default", () => {
    render(<InlineError message="Boom" onDismiss={() => {}} />);
    expect(screen.queryByRole("button", { name: "Retry" })).toBeNull();
  });

  it("copies the message, hint and earlier errors with Copy details (#663)", async () => {
    const writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    try {
      render(<InlineError message="Push failed" hint="Fetch first." copyDetails earlier={["Fetch failed"]} onDismiss={() => {}} />);
      fireEvent.click(screen.getByRole("button", { name: "Copy details" }));
      await waitFor(() => expect(screen.getByRole("button", { name: "Copied" })).toBeInTheDocument());
      expect(writeText).toHaveBeenCalledWith("Push failed\nFetch first.\n\nEarlier errors:\n- Fetch failed");
    } finally {
      Reflect.deleteProperty(navigator, "clipboard");
    }
  });

  it("says when the clipboard refused the copy", async () => {
    render(<InlineError message="Boom" copyDetails onDismiss={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: "Copy details" }));
    expect(await screen.findByRole("button", { name: "Couldn't copy" })).toBeInTheDocument();
  });

  it("offers no Copy details by default", () => {
    render(<InlineError message="Boom" onDismiss={() => {}} />);
    expect(screen.queryByRole("button", { name: "Copy details" })).toBeNull();
  });

  it("lists the earlier errors it replaced, collapsed (#663)", () => {
    render(<InlineError message="Third" earlier={["Second", "First"]} onDismiss={() => {}} />);
    expect(screen.getByText("2 earlier errors")).toBeInTheDocument();
    expect(screen.getByText("Second")).toBeInTheDocument();
    expect(screen.getByText("First")).toBeInTheDocument();
  });
});

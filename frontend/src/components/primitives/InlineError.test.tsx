import { fireEvent, render, screen } from "@testing-library/react";
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
});

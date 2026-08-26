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
});

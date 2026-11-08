import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { GoToNotice } from "./GoToNotice";

describe("GoToNotice", () => {
  it("shows the message with its action and a dismiss button", () => {
    const run = vi.fn();
    const onDismiss = vi.fn();
    render(
      <GoToNotice
        notice={{ message: "v1 is not in the graph.", commitId: "c1", action: { label: "Show feature", run } }}
        onDismiss={onDismiss}
      />,
    );

    expect(screen.getByRole("status")).toHaveTextContent("v1 is not in the graph.");
    fireEvent.click(screen.getByRole("button", { name: "Show feature" }));
    fireEvent.click(screen.getByRole("button", { name: "Dismiss notice" }));
    expect(run).toHaveBeenCalledOnce();
    expect(onDismiss).toHaveBeenCalledOnce();
  });

  it("has no action button without an action", () => {
    render(<GoToNotice notice={{ message: "No commit matches “x”.", commitId: null, action: null }} onDismiss={vi.fn()} />);
    expect(screen.getAllByRole("button")).toHaveLength(1);
  });
});

import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { SplitView } from "./SplitView";

describe("SplitView", () => {
  it("renders both panes", () => {
    render(<SplitView left={<div>left pane</div>} right={<div>right pane</div>} />);
    expect(screen.getByText("left pane")).toBeInTheDocument();
    expect(screen.getByText("right pane")).toBeInTheDocument();
  });
});

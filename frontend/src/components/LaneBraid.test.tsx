import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { LaneBraid } from "./LaneBraid";

describe("LaneBraid", () => {
  it("renders one segment per lane color", () => {
    render(<LaneBraid />);
    const braid = screen.getByRole("presentation");
    expect(braid.children).toHaveLength(6);
  });
});

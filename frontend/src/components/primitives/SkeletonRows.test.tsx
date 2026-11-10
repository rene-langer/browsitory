import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { SkeletonRows } from "./SkeletonRows";

describe("SkeletonRows", () => {
  it("is a named, busy status with decorative rows", () => {
    const { container } = render(<SkeletonRows label="Loading history…" rows={5} />);
    const status = screen.getByRole("status", { name: "Loading history…" });
    expect(status).toHaveAttribute("aria-busy", "true");
    expect(status).toHaveTextContent("Loading history…");
    expect(container.querySelectorAll("[aria-hidden='true']")).toHaveLength(5);
  });
});

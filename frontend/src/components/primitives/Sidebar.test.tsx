import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Sidebar } from "./Sidebar";

describe("Sidebar", () => {
  it("renders its children inside a labeled navigation landmark", () => {
    render(
      <Sidebar>
        <div>section one</div>
        <div>section two</div>
      </Sidebar>,
    );
    const nav = screen.getByRole("navigation", { name: "Repository sections" });
    expect(nav).toBeInTheDocument();
    expect(screen.getByText("section one")).toBeInTheDocument();
    expect(screen.getByText("section two")).toBeInTheDocument();
  });
});

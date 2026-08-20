import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Sidebar } from "./Sidebar";

describe("Sidebar", () => {
  it("renders its children inside a labeled complementary landmark", () => {
    render(
      <Sidebar>
        <div>section one</div>
        <div>section two</div>
      </Sidebar>,
    );
    // aside, not nav: this sidebar holds panels for branches/remotes/tags/etc, not a set of
    // page-to-page navigation links, so "complementary" is the accurate landmark role.
    const aside = screen.getByRole("complementary", { name: "Repository sections" });
    expect(aside).toBeInTheDocument();
    expect(screen.getByText("section one")).toBeInTheDocument();
    expect(screen.getByText("section two")).toBeInTheDocument();
  });
});

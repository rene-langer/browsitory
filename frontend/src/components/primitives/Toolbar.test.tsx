import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Toolbar } from "./Toolbar";

describe("Toolbar", () => {
  it("renders its children in a plain layout wrapper", () => {
    render(
      <Toolbar>
        <button>One</button>
        <button>Two</button>
      </Toolbar>,
    );
    expect(screen.getByRole("button", { name: "One" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Two" })).toBeInTheDocument();
  });

  it("claims role=\"group\" (a plain grouping semantic), never \"toolbar\" — no toolbar keyboard semantics are implemented", () => {
    const { container } = render(
      <Toolbar>
        <button>One</button>
      </Toolbar>,
    );
    expect(screen.queryByRole("toolbar")).not.toBeInTheDocument();
    expect(container.firstElementChild?.tagName).toBe("DIV");
    expect(container.firstElementChild).toHaveAttribute("role", "group");
  });

  it("has no accessible name when no label is given", () => {
    render(
      <Toolbar>
        <button>One</button>
      </Toolbar>,
    );
    expect(screen.getByRole("group")).not.toHaveAttribute("aria-label");
  });

  it("takes an optional aria-label", () => {
    render(
      <Toolbar aria-label="Branch actions">
        <button>One</button>
      </Toolbar>,
    );
    expect(screen.getByRole("group", { name: "Branch actions" })).toBeInTheDocument();
  });
});

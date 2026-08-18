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

  it("claims no ARIA role, since it implements no toolbar keyboard semantics", () => {
    const { container } = render(
      <Toolbar>
        <button>One</button>
      </Toolbar>,
    );
    expect(screen.queryByRole("toolbar")).not.toBeInTheDocument();
    expect(container.firstElementChild?.tagName).toBe("DIV");
    expect(container.firstElementChild).not.toHaveAttribute("role");
  });
});

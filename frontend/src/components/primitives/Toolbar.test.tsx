import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Toolbar } from "./Toolbar";

describe("Toolbar", () => {
  it("renders its children inside a toolbar landmark", () => {
    render(
      <Toolbar>
        <button>One</button>
        <button>Two</button>
      </Toolbar>,
    );
    const toolbar = screen.getByRole("toolbar");
    expect(toolbar).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "One" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Two" })).toBeInTheDocument();
  });
});

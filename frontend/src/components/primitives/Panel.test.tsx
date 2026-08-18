import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Panel } from "./Panel";

describe("Panel", () => {
  it("renders a title and children", () => {
    render(
      <Panel title="Remotes">
        <p>content</p>
      </Panel>,
    );
    expect(screen.getByRole("heading", { name: "Remotes" })).toBeInTheDocument();
    expect(screen.getByText("content")).toBeInTheDocument();
  });

  it("renders actions next to the title when provided", () => {
    render(
      <Panel title="Remotes" actions={<button>Add</button>}>
        <p>content</p>
      </Panel>,
    );
    expect(screen.getByRole("button", { name: "Add" })).toBeInTheDocument();
  });

  it("renders without a title", () => {
    render(<Panel>{<p>content</p>}</Panel>);
    expect(screen.queryByRole("heading")).not.toBeInTheDocument();
    expect(screen.getByText("content")).toBeInTheDocument();
  });

  it("passes through aria-live and aria-label when provided", () => {
    render(
      <Panel ariaLive="polite" ariaLabel="Status">
        <p>content</p>
      </Panel>,
    );
    expect(screen.getByLabelText("Status")).toHaveAttribute("aria-live", "polite");
  });

  it("defaults aria-label to title when ariaLabel not given", () => {
    render(
      <Panel title="Remotes">
        <p>content</p>
      </Panel>,
    );
    expect(screen.getByLabelText("Remotes")).toBeInTheDocument();
  });
});

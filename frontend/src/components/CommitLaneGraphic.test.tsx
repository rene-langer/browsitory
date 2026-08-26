import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { CommitLayout } from "../lib/commitGraphLayout";
import { CommitLaneGraphic } from "./CommitLaneGraphic";

describe("CommitLaneGraphic", () => {
  it("renders a dot at the commit's lane position", () => {
    const layout: CommitLayout = {
      commitId: "a",
      lane: 1,
      parentConnections: [],
      passThroughLanes: [],
    };

    const { container } = render(<CommitLaneGraphic layout={layout} totalLanes={2} />);

    const circle = container.querySelector("circle");
    expect(circle).not.toBeNull();
    expect(circle?.getAttribute("cx")).toBe(String(1 * 16 + 8));
  });

  it("renders one line per pass-through lane and one per parent connection", () => {
    const layout: CommitLayout = {
      commitId: "a",
      lane: 0,
      parentConnections: [
        { parentId: "b", lane: 0 },
        { parentId: "c", lane: 1 },
      ],
      passThroughLanes: [0],
    };

    const { container } = render(<CommitLaneGraphic layout={layout} totalLanes={2} />);

    const lines = container.querySelectorAll("line");
    // 1 upper pass-through line (lane 0) + 2 lower connector lines (to lane 0 and lane 1).
    expect(lines.length).toBe(3);
  });

  it("sizes the svg to totalLanes", () => {
    const layout: CommitLayout = {
      commitId: "a",
      lane: 0,
      parentConnections: [],
      passThroughLanes: [],
    };

    const { container } = render(<CommitLaneGraphic layout={layout} totalLanes={3} />);

    const svg = container.querySelector("svg");
    expect(svg?.getAttribute("width")).toBe(String(3 * 16));
  });

  it("stretches to fill the row's actual rendered height via a viewBox, instead of a fixed pixel height", () => {
    // A row's real height (with CSS padding bled in) isn't the same as the 24px logical row
    // height the line coordinates are drawn in. Without a viewBox+stretch, the svg is clipped to
    // (or leaves a gap at) exactly 24px regardless of the row's actual height, which is what
    // breaks the line-to-line connection between adjacent rows.
    const layout: CommitLayout = {
      commitId: "a",
      lane: 0,
      parentConnections: [],
      passThroughLanes: [],
    };

    const { container } = render(<CommitLaneGraphic layout={layout} totalLanes={2} />);

    const svg = container.querySelector("svg");
    expect(svg?.getAttribute("viewBox")).toBe(`0 0 ${2 * 16} 24`);
    expect(svg?.getAttribute("preserveAspectRatio")).toBe("none");
    expect(svg?.getAttribute("height")).toBe("100%");
  });

  it("dims lines and the dot belonging to a lane other than the hovered one", () => {
    const layout: CommitLayout = {
      commitId: "a",
      lane: 1,
      parentConnections: [],
      passThroughLanes: [0, 1],
    };

    const { container } = render(
      <CommitLaneGraphic layout={layout} totalLanes={2} hoveredLane={0} />,
    );

    const lane0Line = Array.from(container.querySelectorAll("line")).find(
      (line) => line.getAttribute("x1") === String(0 * 16 + 8),
    );
    const lane1Line = Array.from(container.querySelectorAll("line")).find(
      (line) => line.getAttribute("x1") === String(1 * 16 + 8),
    );
    const circle = container.querySelector("circle");

    expect(lane0Line?.getAttribute("opacity")).toBe("1");
    expect(lane1Line?.getAttribute("opacity")).toBe("0.25");
    expect(circle?.getAttribute("opacity")).toBe("0.25");
  });

  it("dims nothing when no lane is hovered", () => {
    const layout: CommitLayout = {
      commitId: "a",
      lane: 0,
      parentConnections: [],
      passThroughLanes: [0],
    };

    const { container } = render(
      <CommitLaneGraphic layout={layout} totalLanes={1} hoveredLane={null} />,
    );

    const line = container.querySelector("line");
    const circle = container.querySelector("circle");
    expect(line?.getAttribute("opacity")).toBe("1");
    expect(circle?.getAttribute("opacity")).toBe("1");
  });
});

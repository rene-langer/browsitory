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
});

import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { CommitLayout } from "../lib/commitGraphLayout";
import { CommitLaneGraphic } from "./CommitLaneGraphic";

describe("CommitLaneGraphic", () => {
  it("renders a dot at the commit's lane position", () => {
    const layout: CommitLayout = {
      commitId: "a",
      lane: 1,
      laneSegmentId: 1,
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
      laneSegmentId: 0,
      parentConnections: [
        { parentId: "b", lane: 0, segmentId: 0 },
        { parentId: "c", lane: 1, segmentId: 1 },
      ],
      passThroughLanes: [{ lane: 0, segmentId: 0 }],
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
      laneSegmentId: 0,
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
      laneSegmentId: 0,
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
      laneSegmentId: 1,
      parentConnections: [],
      passThroughLanes: [
        { lane: 0, segmentId: 0 },
        { lane: 1, segmentId: 1 },
      ],
    };

    const { container } = render(
      <CommitLaneGraphic layout={layout} totalLanes={2} hoveredSegmentId={0} />,
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

  it("dims a same-numbered lane that belongs to a different, unrelated segment", () => {
    // Lane numbers get freed and reused by unrelated branches (see commitGraphLayout.test.ts).
    // This row's own lane happens to be numbered 1, same as the hovered row's lane, but it's a
    // different segment (a different, unrelated branch occupancy) — it must still get dimmed,
    // since matching only on the numeric lane would wrongly light up the unrelated branch too.
    const layout: CommitLayout = {
      commitId: "a",
      lane: 1,
      laneSegmentId: 7,
      parentConnections: [],
      passThroughLanes: [],
    };

    const { container } = render(
      <CommitLaneGraphic layout={layout} totalLanes={2} hoveredSegmentId={1} />,
    );

    const circle = container.querySelector("circle");
    expect(circle?.getAttribute("opacity")).toBe("0.25");
  });

  it("paints a lane-shifting connector line on top of an untouched pass-through line it crosses", () => {
    // Own lane 0 connects down to lane 2, diagonally crossing lane 1's column. Lane 1 is an
    // unrelated branch just passing through (no connection of its own here) so it also draws a
    // plain vertical line through the same row. SVG paints later siblings on top of earlier
    // ones with no blending, so whichever line is later in DOM order wins visually at the
    // crossing point. The vertical pass-through must not be allowed to cut the diagonal in two.
    const layout: CommitLayout = {
      commitId: "a",
      lane: 0,
      laneSegmentId: 0,
      parentConnections: [{ parentId: "p", lane: 2, segmentId: 2 }],
      passThroughLanes: [
        { lane: 0, segmentId: 0 },
        { lane: 1, segmentId: 1 },
        { lane: 2, segmentId: 2 },
      ],
    };

    const { container } = render(<CommitLaneGraphic layout={layout} totalLanes={3} />);

    const lines = Array.from(container.querySelectorAll("line"));
    const diagonalIndex = lines.findIndex((line) => line.getAttribute("x1") !== line.getAttribute("x2"));
    const untouchedLane1Index = lines.findIndex(
      (line) =>
        line.getAttribute("x1") === line.getAttribute("x2") &&
        line.getAttribute("x1") === String(1 * 16 + 8) &&
        line.getAttribute("y1") === "0" &&
        line.getAttribute("y2") === "24",
    );

    expect(diagonalIndex).toBeGreaterThanOrEqual(0);
    expect(untouchedLane1Index).toBeGreaterThanOrEqual(0);
    expect(diagonalIndex).toBeGreaterThan(untouchedLane1Index);
  });

  it("draws an unrelated lane's pass-through as one unbroken line, not two segments meeting at the row's midpoint", () => {
    // Lane 0 is another branch waiting on a parent this row's own commit (lane 1) also happens
    // to connect to (a fork: two children of the same parent). Splitting lane 0's pass-through
    // into an upper (0..mid) and lower (mid..bottom) segment — even with matching endpoints —
    // can show a hairline rendering seam where the two independently-antialiased strokes meet,
    // since nothing here (unlike this row's own dot) sits on top of that exact point to mask it.
    // It must be a single line spanning the full row height instead.
    const layout: CommitLayout = {
      commitId: "a",
      lane: 1,
      laneSegmentId: 1,
      parentConnections: [{ parentId: "shared-parent", lane: 0, segmentId: 0 }],
      passThroughLanes: [
        { lane: 0, segmentId: 0 },
        { lane: 1, segmentId: 1 },
      ],
    };

    const { container } = render(<CommitLaneGraphic layout={layout} totalLanes={2} />);

    const fullHeightLane0 = Array.from(container.querySelectorAll("line")).find(
      (line) =>
        line.getAttribute("x1") === line.getAttribute("x2") &&
        line.getAttribute("x1") === String(0 * 16 + 8) &&
        line.getAttribute("y1") === "0" &&
        line.getAttribute("y2") === "24",
    );

    expect(fullHeightLane0).not.toBeUndefined();
  });

  it("dims nothing when no lane is hovered", () => {
    const layout: CommitLayout = {
      commitId: "a",
      lane: 0,
      laneSegmentId: 0,
      parentConnections: [],
      passThroughLanes: [{ lane: 0, segmentId: 0 }],
    };

    const { container } = render(
      <CommitLaneGraphic layout={layout} totalLanes={1} hoveredSegmentId={null} />,
    );

    const line = container.querySelector("line");
    const circle = container.querySelector("circle");
    expect(line?.getAttribute("opacity")).toBe("1");
    expect(circle?.getAttribute("opacity")).toBe("1");
  });
});

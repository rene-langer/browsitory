import type { CommitLayout } from "../lib/commitGraphLayout";
import { LANE_COLORS } from "../lib/laneColors";

const LANE_WIDTH = 16;
const ROW_HEIGHT = 24;

function laneColor(lane: number): string {
  return LANE_COLORS[lane % LANE_COLORS.length];
}

function laneCenterX(lane: number): number {
  return lane * LANE_WIDTH + LANE_WIDTH / 2;
}

// Highlighting keys off the lane *segment* id, not the raw lane number — lane numbers get freed
// and reused by later, unrelated branches, and matching on the number alone would highlight
// those unrelated branches too whenever they land on the same reused lane.
function opacityFor(segmentId: number, hoveredSegmentId: number | null | undefined): number {
  return hoveredSegmentId == null || segmentId === hoveredSegmentId ? 1 : 0.25;
}

export function CommitLaneGraphic({
  layout,
  totalLanes,
  hoveredSegmentId,
}: {
  layout: CommitLayout;
  totalLanes: number;
  hoveredSegmentId?: number | null;
}) {
  const width = totalLanes * LANE_WIDTH;
  const midY = ROW_HEIGHT / 2;

  // This row's own lane, if something above is already waiting for this commit, only needs its
  // upper half drawn here — the lower half is whatever `connectionLines` below draws for it
  // (straight, diagonal, or nothing, depending on this commit's own parents). Splitting that
  // lower half off is fine because it always meets this row's own dot exactly at the seam,
  // which masks the hairline gap two independently-antialiased strokes can leave where they abut.
  const ownUpperLine = layout.passThroughLanes.some(
    (entry) => entry.segmentId === layout.laneSegmentId,
  ) ? (
    <line
      key="up-own"
      x1={laneCenterX(layout.lane)}
      y1={0}
      x2={laneCenterX(layout.lane)}
      y2={midY}
      stroke={laneColor(layout.lane)}
      strokeWidth={2}
      opacity={opacityFor(layout.laneSegmentId, hoveredSegmentId)}
    />
  ) : null;

  // Every OTHER pass-through lane is just passing straight through this row, untouched by
  // whatever this commit's own connections do — draw it as a single line spanning the full row
  // height rather than splitting it at the midpoint. Nothing sits on top of that seam to mask
  // it (unlike the own-lane case above), so a split there is a visible rendering gap.
  const otherPassThroughLines = layout.passThroughLanes
    .filter((entry) => entry.segmentId !== layout.laneSegmentId)
    .map((entry) => (
      <line
        key={`through-${entry.lane}`}
        x1={laneCenterX(entry.lane)}
        y1={0}
        x2={laneCenterX(entry.lane)}
        y2={ROW_HEIGHT}
        stroke={laneColor(entry.lane)}
        strokeWidth={2}
        opacity={opacityFor(entry.segmentId, hoveredSegmentId)}
      />
    ));

  const connectionLines = layout.parentConnections.map((conn) => (
    <line
      key={`down-${conn.lane}`}
      x1={laneCenterX(layout.lane)}
      y1={midY}
      x2={laneCenterX(conn.lane)}
      y2={ROW_HEIGHT}
      stroke={laneColor(conn.lane)}
      strokeWidth={2}
      opacity={opacityFor(conn.segmentId, hoveredSegmentId)}
    />
  ));

  return (
    <svg
      width={width}
      height="100%"
      viewBox={`0 0 ${width} ${ROW_HEIGHT}`}
      preserveAspectRatio="none"
      style={{ flexShrink: 0 }}
    >
      {otherPassThroughLines}
      {ownUpperLine}
      {connectionLines}
      <circle
        cx={laneCenterX(layout.lane)}
        cy={midY}
        r={4}
        fill={laneColor(layout.lane)}
        opacity={opacityFor(layout.laneSegmentId, hoveredSegmentId)}
      />
    </svg>
  );
}

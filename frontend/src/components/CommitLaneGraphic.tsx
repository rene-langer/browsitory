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

export function CommitLaneGraphic({
  layout,
  totalLanes,
}: {
  layout: CommitLayout;
  totalLanes: number;
}) {
  const width = totalLanes * LANE_WIDTH;
  const midY = ROW_HEIGHT / 2;

  const upperLines = layout.passThroughLanes.map((lane) => (
    <line
      key={`up-${lane}`}
      x1={laneCenterX(lane)}
      y1={0}
      x2={laneCenterX(lane)}
      y2={midY}
      stroke={laneColor(lane)}
      strokeWidth={2}
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
    />
  ));

  const untouchedPassThroughLines = layout.passThroughLanes
    .filter(
      (lane) =>
        lane !== layout.lane && !layout.parentConnections.some((conn) => conn.lane === lane),
    )
    .map((lane) => (
      <line
        key={`through-${lane}`}
        x1={laneCenterX(lane)}
        y1={midY}
        x2={laneCenterX(lane)}
        y2={ROW_HEIGHT}
        stroke={laneColor(lane)}
        strokeWidth={2}
      />
    ));

  return (
    <svg width={width} height={ROW_HEIGHT} style={{ flexShrink: 0 }}>
      {upperLines}
      {connectionLines}
      {untouchedPassThroughLines}
      <circle cx={laneCenterX(layout.lane)} cy={midY} r={4} fill={laneColor(layout.lane)} />
    </svg>
  );
}

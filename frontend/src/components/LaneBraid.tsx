import styles from "./LaneBraid.module.css";

// Matches CommitLaneGraphic.tsx's LANE_COLORS exactly — the header braid and
// the branch graph must stay the same six hues in the same order.
const LANE_COLORS = ["#e36209", "#1a7f37", "#0969da", "#8250df", "#cf222e", "#bf8700"];

export function LaneBraid() {
  return (
    <div className={styles.braid} role="presentation">
      {LANE_COLORS.map((color) => (
        <div key={color} className={styles.segment} style={{ background: color }} />
      ))}
    </div>
  );
}

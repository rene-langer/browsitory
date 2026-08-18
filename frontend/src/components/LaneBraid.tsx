import { LANE_COLORS } from "../lib/laneColors";
import styles from "./LaneBraid.module.css";

export function LaneBraid() {
  return (
    <div className={styles.braid} role="presentation">
      {LANE_COLORS.map((color) => (
        <div key={color} className={styles.segment} style={{ background: color }} />
      ))}
    </div>
  );
}

import { compactCount, lineStatsLabel } from "../lib/lineStats";
import styles from "./LineStatsBadge.module.css";

/**
 * `+12 −3`: lines added in green, removed in red (#620). Screen readers get `lineStatsLabel`
 * instead of the symbols, which also names the commit row it sits in.
 */
export function LineStatsBadge({
  insertions,
  deletions,
  className,
}: {
  insertions: number;
  deletions: number;
  className?: string;
}) {
  const label = lineStatsLabel(insertions, deletions);
  return (
    <span className={[styles.badge, className].filter(Boolean).join(" ")} title={label} data-testid="line-stats">
      <span className={styles.srOnly}>{label}</span>
      <span aria-hidden="true" className={styles.added}>
        +{compactCount(insertions)}
      </span>
      <span aria-hidden="true" className={styles.removed}>
        −{compactCount(deletions)}
      </span>
    </span>
  );
}

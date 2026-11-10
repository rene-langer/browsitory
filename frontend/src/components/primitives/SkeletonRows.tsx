import styles from "./SkeletonRows.module.css";

// Widths that vary row to row, so the placeholder reads as a list rather than a block.
const WIDTHS = ["72%", "58%", "84%", "64%", "76%", "52%", "68%", "80%"];

/**
 * Placeholder rows while a list's first data loads (#660), instead of an empty pane that looks
 * like an empty repository. Busy (`aria-busy`) and named ("Loading history…") for screen readers;
 * the rows themselves are decoration. Reduced motion stops their shimmer (index.css).
 */
export function SkeletonRows({ label, rows = 8 }: { label: string; rows?: number }) {
  return (
    <div className={styles.skeleton} role="status" aria-busy="true" aria-label={label}>
      <span className={styles.hidden}>{label}</span>
      {Array.from({ length: rows }, (_, index) => (
        <span key={index} className={styles.row} aria-hidden="true">
          <span className={styles.bar} style={{ width: WIDTHS[index % WIDTHS.length] }} />
        </span>
      ))}
    </div>
  );
}

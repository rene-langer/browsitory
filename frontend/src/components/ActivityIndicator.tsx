import type { ReactNode } from "react";
import styles from "./ActivityIndicator.module.css";

/** What is running, as `ActivityIndicator` shows it. */
export interface Activity {
  /** Names the work: "Pushing main to origin/main". */
  label: string;
  /** Where it got to: "120 / 480 objects, 2.1 MB sent". */
  detail?: string | null;
  /** Done so far, 0 to 1; `null` (or absent) when unknown: the bar moves without a value. */
  progress?: number | null;
  /**
   * Not announced, and shown only after a short delay: for an action that usually ends at once,
   * so a quick one doesn't flash or chatter while a slow one still says it is running.
   */
  quiet?: boolean;
  /** A control next to it, such as Cancel. */
  action?: ReactNode;
}

/**
 * The repository's busy state (#660): a label, a progress bar (determinate when the work reports
 * how far it got) and an optional action. Always mounted, so its polite live region exists before
 * the first activity and announces each new label; the changing detail stays out of it, as the
 * progress bar's value. Reusable wherever something runs: a transfer in the toolbar (#659), an
 * action in flight, a section that is still loading.
 */
export function ActivityIndicator({ activity }: { activity: Activity | null }) {
  const percent =
    activity?.progress == null ? null : Math.round(Math.min(1, Math.max(0, activity.progress)) * 100);
  return (
    <div
      role={activity === null ? undefined : "group"}
      aria-label={activity?.label}
      className={styles.activity}
      data-idle={activity === null ? "true" : undefined}
      data-quiet={activity?.quiet === true ? "true" : undefined}>
      <span role="status" className={styles.announcement}>
        {activity !== null && activity.quiet !== true ? activity.label : ""}
      </span>
      {activity !== null && (
        <>
          <span className={styles.label} title={activity.label} aria-hidden="true">
            {activity.label}
          </span>
          <span
            role="progressbar"
            aria-label={activity.label}
            aria-valuemin={percent === null ? undefined : 0}
            aria-valuemax={percent === null ? undefined : 100}
            aria-valuenow={percent ?? undefined}
            aria-valuetext={activity.detail ?? undefined}
            className={styles.track}
            data-indeterminate={percent === null ? "true" : undefined}
          >
            <span className={styles.fill} style={percent === null ? undefined : { width: `${percent}%` }} />
          </span>
          {activity.detail != null && (
            <span className={styles.detail} title={activity.detail} aria-hidden="true">
              {activity.detail}
            </span>
          )}
          {activity.action}
        </>
      )}
    </div>
  );
}

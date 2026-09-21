/** Absolute local date-time for a Unix-seconds timestamp. */
export function formatAbsoluteDate(timestampSeconds: number): string {
  return new Date(timestampSeconds * 1000).toLocaleString();
}

const UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ["year", 365 * 24 * 3600],
  ["month", 30 * 24 * 3600],
  ["day", 24 * 3600],
  ["hour", 3600],
  ["minute", 60],
];

/** "3 days ago"-style label for a Unix-seconds timestamp, relative to `nowMs`. */
export function formatRelativeDate(
  timestampSeconds: number,
  nowMs: number = Date.now(),
  locale?: string,
): string {
  const diffSeconds = Math.round(timestampSeconds - nowMs / 1000);
  const formatter = new Intl.RelativeTimeFormat(locale, { numeric: "auto" });
  for (const [unit, seconds] of UNITS) {
    if (Math.abs(diffSeconds) >= seconds) {
      return formatter.format(Math.trunc(diffSeconds / seconds), unit);
    }
  }
  return formatter.format(0, "second");
}

/** Compact date for narrow columns: local `YYYY-MM-DD`. */
export function formatShortDate(timestampSeconds: number): string {
  const date = new Date(timestampSeconds * 1000);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

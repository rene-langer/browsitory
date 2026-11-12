import type { DateFormat } from "../ipc/RepoClient";

/**
 * One date formatter for the whole app (#675), following Preferences → Appearance → Date format:
 * `Iso` is `YYYY-MM-DD HH:MM` in local time, `System` the locale's short date and time
 * (`Intl.DateTimeFormat` with the operating system's or webview's locale, or `locale`). Neither
 * shows seconds.
 */
export function formatAbsoluteDate(timestampSeconds: number, format: DateFormat = "Iso", locale?: string): string {
  if (format === "System") {
    return new Intl.DateTimeFormat(locale, { dateStyle: "short", timeStyle: "short" }).format(
      new Date(timestampSeconds * 1000),
    );
  }
  const date = new Date(timestampSeconds * 1000);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${formatShortDate(timestampSeconds)} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

/** The date alone, for narrow columns: local `YYYY-MM-DD`, or the locale's short date. */
export function formatShortDate(timestampSeconds: number, format: DateFormat = "Iso", locale?: string): string {
  const date = new Date(timestampSeconds * 1000);
  if (format === "System") return new Intl.DateTimeFormat(locale, { dateStyle: "short" }).format(date);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

const UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ["year", 365 * 24 * 3600],
  ["month", 30 * 24 * 3600],
  ["day", 24 * 3600],
  ["hour", 3600],
  ["minute", 60],
];

/**
 * "3 days ago"-style label for a Unix-seconds timestamp, relative to `nowMs`. English unless a
 * `locale` is given, like the rest of the interface (#675).
 */
export function formatRelativeDate(timestampSeconds: number, nowMs: number = Date.now(), locale = "en"): string {
  const diffSeconds = Math.round(timestampSeconds - nowMs / 1000);
  const formatter = new Intl.RelativeTimeFormat(locale, { numeric: "auto" });
  for (const [unit, seconds] of UNITS) {
    if (Math.abs(diffSeconds) >= seconds) {
      return formatter.format(Math.trunc(diffSeconds / seconds), unit);
    }
  }
  return formatter.format(0, "second");
}

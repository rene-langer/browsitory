import { describe, expect, it } from "vitest";
import { formatAbsoluteDate, formatRelativeDate, formatShortDate } from "./formatDate";

describe("formatDate", () => {
  it("formats a relative date in days", () => {
    const now = Date.UTC(2026, 0, 10);
    expect(formatRelativeDate(now / 1000 - 3 * 86400, now, "en")).toBe("3 days ago");
  });

  it("formats a short local date as YYYY-MM-DD", () => {
    const ts = new Date(2026, 4, 7, 12).getTime() / 1000;
    expect(formatShortDate(ts)).toBe("2026-05-07");
  });

  it("formats an absolute local date-time as YYYY-MM-DD HH:MM, like the graph's dates (#641)", () => {
    const ts = new Date(2026, 8, 8, 21, 5, 30).getTime() / 1000;
    expect(formatAbsoluteDate(ts)).toBe("2026-09-08 21:05");
    expect(formatAbsoluteDate(new Date(2026, 0, 2, 3, 4).getTime() / 1000)).toBe("2026-01-02 03:04");
  });
});

// #675: one formatter, with the "Date format" preference: ISO (the default) or the system's.
describe("formatDate with a date format", () => {
  const ts = new Date(2026, 10, 11, 14, 5, 30).getTime() / 1000;
  const system = (options: Intl.DateTimeFormatOptions) =>
    new Intl.DateTimeFormat("en-GB", options).format(new Date(ts * 1000));

  it("is ISO without a format, and with Iso", () => {
    expect(formatAbsoluteDate(ts, "Iso")).toBe("2026-11-11 14:05");
    expect(formatShortDate(ts, "Iso")).toBe("2026-11-11");
  });

  it("uses the locale's short date and time for System, without seconds", () => {
    expect(formatAbsoluteDate(ts, "System", "en-GB")).toBe(system({ dateStyle: "short", timeStyle: "short" }));
    expect(formatAbsoluteDate(ts, "System", "en-GB")).not.toContain(":30");
    expect(formatShortDate(ts, "System", "en-GB")).toBe(system({ dateStyle: "short" }));
    expect(formatShortDate(ts, "System", "en-GB")).toBe("11/11/2026");
  });

  it("says relative dates in English, like the rest of the interface", () => {
    const now = Date.UTC(2026, 0, 10);
    expect(formatRelativeDate(now / 1000 - 3 * 86400, now)).toBe("3 days ago");
    expect(formatRelativeDate(now / 1000 - 2 * 3600, now)).toBe("2 hours ago");
  });
});

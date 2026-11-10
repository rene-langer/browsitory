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

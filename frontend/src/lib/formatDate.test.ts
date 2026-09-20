import { describe, expect, it } from "vitest";
import { formatRelativeDate, formatShortDate } from "./formatDate";

describe("formatDate", () => {
  it("formats a relative date in days", () => {
    const now = Date.UTC(2026, 0, 10);
    expect(formatRelativeDate(now / 1000 - 3 * 86400, now, "en")).toBe("3 days ago");
  });

  it("formats a short local date as YYYY-MM-DD", () => {
    const ts = new Date(2026, 4, 7, 12).getTime() / 1000;
    expect(formatShortDate(ts)).toBe("2026-05-07");
  });
});

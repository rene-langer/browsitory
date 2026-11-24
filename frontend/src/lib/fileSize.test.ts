import { describe, expect, it } from "vitest";
import { formatBytes } from "./fileSize";

// Every size the app shows uses this one format (#1060).
describe("formatBytes", () => {
  it("uses base 1024 with one decimal above bytes", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1023)).toBe("1023 B");
    expect(formatBytes(1024)).toBe("1.0 KB");
    expect(formatBytes(2048)).toBe("2.0 KB");
    expect(formatBytes(12_595)).toBe("12.3 KB");
    expect(formatBytes(25_270_000)).toBe("24.1 MB");
    expect(formatBytes(3 * 1024 ** 3)).toBe("3.0 GB");
  });

  it("stays in GB above a terabyte", () => {
    expect(formatBytes(5 * 1024 ** 4)).toBe("5120.0 GB");
  });
});

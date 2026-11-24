import { describe, expect, it } from "vitest";
import { formatBytes, formatSize } from "./fileSize";

describe("formatSize", () => {
  it("formats bytes, KB and MB", () => {
    expect(formatSize(1)).toBe("1 byte");
    expect(formatSize(512)).toBe("512 bytes");
    expect(formatSize(1536)).toBe("1.5 KB");
    expect(formatSize(2 * 1024 * 1024)).toBe("2 MB");
  });
});

// The image diff and the transfer progress share this one (#1060).
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

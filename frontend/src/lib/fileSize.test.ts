import { describe, expect, it } from "vitest";
import { formatSize } from "./fileSize";

describe("formatSize", () => {
  it("formats bytes, KB and MB", () => {
    expect(formatSize(1)).toBe("1 byte");
    expect(formatSize(512)).toBe("512 bytes");
    expect(formatSize(1536)).toBe("1.5 KB");
    expect(formatSize(2 * 1024 * 1024)).toBe("2 MB");
  });
});

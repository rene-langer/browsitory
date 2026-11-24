import { describe, expect, it } from "vitest";
import { imageDataUrl, imageMimeType, isSvgPath } from "./imageTypes";

describe("imageMimeType", () => {
  it("maps previewable image extensions case-insensitively", () => {
    expect(imageMimeType("a/B.PNG")).toBe("image/png");
    expect(imageMimeType("x.jpeg")).toBe("image/jpeg");
    expect(imageMimeType("x.jpg")).toBe("image/jpeg");
    expect(imageMimeType("x.gif")).toBe("image/gif");
    expect(imageMimeType("x.webp")).toBe("image/webp");
    expect(imageMimeType("icon.svg")).toBe("image/svg+xml");
  });

  it("ignores other files, folders and bare names", () => {
    expect(imageMimeType("x.bmp")).toBeNull();
    expect(imageMimeType("png")).toBeNull();
    expect(imageMimeType(".png")).toBeNull();
    expect(imageMimeType("dir.png/readme")).toBeNull();
    expect(imageMimeType("x.png.txt")).toBeNull();
  });

  it("tells SVGs apart", () => {
    expect(isSvgPath("a/Logo.SVG")).toBe(true);
    expect(isSvgPath("a.png")).toBe(false);
  });
});

describe("imageDataUrl", () => {
  it("builds a data URL only for an allowed sniffed type and well-formed base64", () => {
    expect(imageDataUrl("image/png", "Zm9v")).toBe("data:image/png;base64,Zm9v");
    expect(imageDataUrl("image/svg+xml", "PHN2Zy8+")).toBe("data:image/svg+xml;base64,PHN2Zy8+");
    expect(imageDataUrl(null, "Zm9v")).toBeNull();
    expect(imageDataUrl("text/html", "Zm9v")).toBeNull();
    expect(imageDataUrl("image/png", "Zm9v\"><script>")).toBeNull();
    expect(imageDataUrl("image/png", "Zm9")).toBeNull();
    expect(imageDataUrl("image/png", "Zm=v")).toBeNull();
  });

  it("checks a preview at the 20 MB cap without overflowing the stack", () => {
    const big = "QUJD".repeat(7_000_000); // 28 MB of base64, about 21 MB of bytes
    expect(imageDataUrl("image/png", big)).toBe(`data:image/png;base64,${big}`);
  });
});

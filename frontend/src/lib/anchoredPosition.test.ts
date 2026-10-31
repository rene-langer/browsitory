import { describe, expect, it } from "vitest";
import { placeAnchored } from "./anchoredPosition";

const viewport = { width: 1000, height: 800 };
const size = { width: 300, height: 200 };

describe("placeAnchored", () => {
  it("places below when it fits", () => {
    expect(placeAnchored({ top: 100, bottom: 120, left: 50 }, size, viewport)).toEqual({
      side: "below",
      top: 124,
      left: 50,
      maxHeight: 800 - 124 - 4,
    });
  });

  it("flips above when it doesn't fit below but fits above", () => {
    const placement = placeAnchored({ top: 700, bottom: 720, left: 50 }, size, viewport);
    expect(placement.side).toBe("above");
    expect(placement.top).toBe(700 - 4 - 200);
    expect(placement.maxHeight).toBe(700 - 4 - 4);
  });

  it("takes the larger side and caps the height when neither fits", () => {
    const placement = placeAnchored({ top: 500, bottom: 520, left: 50 }, { width: 300, height: 600 }, viewport);
    expect(placement).toEqual({ side: "above", maxHeight: 492, top: 4, left: 50 });
  });

  it("takes the room below when neither fits and below is larger", () => {
    const placement = placeAnchored({ top: 200, bottom: 220, left: 50 }, { width: 300, height: 700 }, viewport);
    expect(placement).toEqual({ side: "below", top: 224, left: 50, maxHeight: 800 - 224 - 4 });
  });

  it("shifts left at the right edge", () => {
    expect(placeAnchored({ top: 100, bottom: 120, left: 900 }, size, viewport).left).toBe(1000 - 300 - 4);
  });

  it("clamps to the margin at the left edge", () => {
    expect(placeAnchored({ top: 100, bottom: 120, left: -20 }, size, viewport).left).toBe(4);
  });

  it("pins to the margin when wider than the viewport", () => {
    expect(placeAnchored({ top: 100, bottom: 120, left: 500 }, { width: 1200, height: 200 }, viewport).left).toBe(4);
  });

  it("honours a custom gap and margin", () => {
    expect(placeAnchored({ top: 100, bottom: 120, left: 0 }, size, viewport, { gap: 8, margin: 10 })).toEqual({
      side: "below",
      top: 128,
      left: 10,
      maxHeight: 800 - 128 - 10,
    });
  });
});

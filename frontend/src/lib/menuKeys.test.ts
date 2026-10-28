import { describe, expect, it } from "vitest";
import { isMenuKey, menuAnchor, rectAnchor } from "./menuKeys";

function elementAt(left: number, top: number, width: number, height: number): Element {
  const element = document.createElement("div");
  element.getBoundingClientRect = () =>
    ({
      left,
      top,
      right: left + width,
      bottom: top + height,
      width,
      height,
      x: left,
      y: top,
      toJSON: () => ({}),
    }) as DOMRect;
  return element;
}

describe("isMenuKey", () => {
  it("accepts the Menu key and Shift+F10 only", () => {
    expect(isMenuKey({ key: "ContextMenu", shiftKey: false })).toBe(true);
    expect(isMenuKey({ key: "F10", shiftKey: true })).toBe(true);
    expect(isMenuKey({ key: "F10", shiftKey: false })).toBe(false);
    expect(isMenuKey({ key: "Enter", shiftKey: true })).toBe(false);
  });
});

describe("menuAnchor", () => {
  const row = elementAt(100, 200, 300, 22);

  it("uses the pointer when it is inside the element (a right-click)", () => {
    expect(menuAnchor({ clientX: 150, clientY: 210 }, row)).toEqual({ x: 150, y: 210 });
  });

  it("uses the element's bottom-left corner for a keyboard-generated event at (0,0)", () => {
    expect(menuAnchor({ clientX: 0, clientY: 0 }, row)).toEqual({ x: 100, y: 222 });
  });

  it("uses the element's bottom-left corner when the point is outside it", () => {
    expect(menuAnchor({ clientX: 5, clientY: 900 }, row)).toEqual({ x: 100, y: 222 });
  });
});

describe("rectAnchor", () => {
  it("is the bottom-left corner", () => {
    expect(rectAnchor(elementAt(10, 20, 30, 40))).toEqual({ x: 10, y: 60 });
  });
});

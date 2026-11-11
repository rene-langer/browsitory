import { fireEvent } from "@testing-library/react";

/** The height of every `li` in `layoutInPane`. */
export const ROW = 26;

/**
 * A scrolling pane for a windowed list's tests (`useVirtualRows`, #685/#775): jsdom has no
 * layout, so every `li` is `ROW` tall, the pane is `height` px high at the top of the page, and
 * a `ul` inside it starts at its top and moves up by the pane's `scrollTop`. Render into
 * `pane` (`render(..., { container: pane })`) and call `restore` when done.
 */
export function layoutInPane(height = 260) {
  const pane = document.createElement("div");
  pane.style.overflowY = "auto";
  Object.defineProperty(pane, "clientHeight", { configurable: true, value: height });
  document.body.appendChild(pane);

  const original = Element.prototype.getBoundingClientRect;
  Element.prototype.getBoundingClientRect = function (this: Element) {
    const rowHeight = this.tagName === "LI" ? ROW : 0;
    const top = this.tagName === "UL" ? -pane.scrollTop : 0;
    return {
      top,
      left: 0,
      right: 0,
      bottom: top + rowHeight,
      width: 0,
      height: rowHeight,
      x: 0,
      y: top,
      toJSON: () => ({}),
    };
  };

  const scrollTo = (top: number) => {
    pane.scrollTop = top;
    fireEvent.scroll(pane);
  };
  const restore = () => {
    Element.prototype.getBoundingClientRect = original;
    pane.remove();
  };
  return { pane, scrollTo, restore };
}

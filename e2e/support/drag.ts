import { browser } from "@wdio/globals";

/**
 * Drags `source` onto `target` with pointer events dispatched in the page (#744), at the two
 * elements' real positions: the app's own hit test (`elementFromPoint`) then finds the target
 * against the real layout. Not WebdriverIO's pointer actions, which upset the WebKitGTK driver's
 * button state (see rebase.spec.ts on right-clicks). `where` picks the point on the target: its
 * middle, or a quarter from its top or bottom (before or after a row).
 */
export async function dragInPage(
  source: WebdriverIO.Element,
  target: WebdriverIO.Element,
  where: "middle" | "upper" | "lower" = "middle",
): Promise<void> {
  await browser.execute(
    (from: HTMLElement, to: HTMLElement, point: string) => {
      from.scrollIntoView({ block: "center" });
      to.scrollIntoView({ block: "nearest" });
      const start = from.getBoundingClientRect();
      const end = to.getBoundingClientRect();
      const fire = (element: EventTarget, type: string, x: number, y: number) =>
        element.dispatchEvent(
          new PointerEvent(type, {
            bubbles: true,
            cancelable: true,
            clientX: x,
            clientY: y,
            button: 0,
            pointerId: 1,
            isPrimary: true,
          }),
        );
      const startX = start.left + start.width / 2;
      const startY = start.top + start.height / 2;
      const endX = end.left + end.width / 2;
      const endY =
        point === "upper"
          ? end.top + end.height / 4
          : point === "lower"
            ? end.bottom - end.height / 4
            : end.top + end.height / 2;
      // Pressed on the source's own text, not on a button inside it.
      const pressed = document.elementFromPoint(startX, startY) ?? from;
      fire(pressed, "pointerdown", startX, startY);
      fire(document, "pointermove", startX, startY + 10);
      fire(document, "pointermove", endX, endY);
      fire(document, "pointerup", endX, endY);
    },
    source,
    target,
    where,
  );
}

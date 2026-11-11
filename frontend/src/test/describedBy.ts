/**
 * The text of the elements `element`'s `aria-describedby` names (#748): its description from
 * visible text. Unlike `toHaveAccessibleDescription`, a `title` never counts.
 */
export function describedByText(element: Element): string {
  const ids = element.getAttribute("aria-describedby")?.split(/\s+/).filter(Boolean) ?? [];
  return ids
    .map((id) => element.ownerDocument.getElementById(id)?.textContent ?? "")
    .join(" ")
    .trim();
}

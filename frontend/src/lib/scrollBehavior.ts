/** Smooth scrolling unless the user asked the OS for reduced motion. */
export function scrollBehavior(): ScrollBehavior {
  return typeof window.matchMedia === "function" && window.matchMedia("(prefers-reduced-motion: reduce)").matches
    ? "auto"
    : "smooth";
}

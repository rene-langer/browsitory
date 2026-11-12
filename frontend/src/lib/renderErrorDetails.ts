/**
 * The text of a render error caught by `RenderErrorBoundary` (#846): what the log records and
 * what the fallback's "Copy details" copies.
 */

/** An exception thrown while rendering, and where in the component tree it happened. */
export interface CaughtRenderError {
  error: unknown;
  /** React's component stack, one `    at Name` line per component; empty when unknown. */
  componentStack: string;
}

/** The error, its stack and React's component stack, as the log records them. */
export function renderErrorReport(caught: CaughtRenderError): string {
  const { error } = caught;
  const lines = [error instanceof Error ? `${error.name}: ${error.message}` : String(error)];
  if (error instanceof Error && error.stack !== undefined && error.stack !== "") lines.push("", error.stack);
  const componentStack = caught.componentStack.replace(/^\n+/, "").trimEnd();
  if (componentStack !== "") lines.push("", "Component stack:", componentStack);
  return lines.join("\n");
}

/** What "Copy details" copies: the app version (when known), then {@link renderErrorReport}. */
export function renderErrorDetails(caught: CaughtRenderError, version: string | null): string {
  return `${version === null ? "Browsitory (version unknown)" : `Browsitory ${version}`}\n${renderErrorReport(caught)}`;
}

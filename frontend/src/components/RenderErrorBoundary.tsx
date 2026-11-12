import type { ReactNode } from "react";

/** Stub for the test-first commit (#846); the implementation follows. */
export interface CaughtRenderError {
  error: unknown;
  componentStack: string;
}

export function RenderErrorBoundary(props: {
  onError: (caught: CaughtRenderError) => void;
  fallback: (caught: CaughtRenderError, reset: () => void) => ReactNode;
  children: ReactNode;
}) {
  return <>{props.children}</>;
}

export function RenderErrorFallback(_props: {
  title: string;
  caught: CaughtRenderError;
  hint: string;
  reloadLabel: string;
  onReload: () => void;
  onClose?: () => void;
  getAppVersion: () => Promise<string>;
  focusOnShow: boolean;
}) {
  return null;
}

export function renderErrorDetails(_caught: CaughtRenderError, _version: string | null): string {
  return "";
}

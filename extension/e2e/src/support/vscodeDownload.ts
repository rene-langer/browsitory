export interface RetryOptions {
  attempts: number;
  initialDelayMs: number;
  sleep?: (ms: number) => Promise<void>;
  onRetry?: (attempt: number, error: unknown, delayMs: number) => void;
}

export async function withRetry<T>(_run: (attempt: number) => Promise<T>, _options: RetryOptions): Promise<T> {
  throw new Error("not implemented");
}

export function pinnedVSCodeVersion(_env: NodeJS.ProcessEnv): string | undefined {
  throw new Error("not implemented");
}

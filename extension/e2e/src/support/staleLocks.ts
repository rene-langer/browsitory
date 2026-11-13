export interface ClearStaleLocksOptions {
  graceMs: number;
  pollMs: number;
  sleep?: (ms: number) => Promise<void>;
}

export async function clearStaleLocks(_gitDir: string, _options: ClearStaleLocksOptions): Promise<string[]> {
  return [];
}

export interface TransportStatus {
  state: "reconnecting" | "failed";
  message: string;
}

type TransportStatusListener = (status: TransportStatus) => void;

const listeners = new Set<TransportStatusListener>();

export function publishTransportStatus(status: TransportStatus): void {
  for (const listener of listeners) listener(status);
}

export function subscribeTransportStatus(listener: TransportStatusListener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** Test-only lifecycle cleanup for Vitest's shared module graph. */
export function __resetTransportStatusForTests(): void {
  listeners.clear();
}

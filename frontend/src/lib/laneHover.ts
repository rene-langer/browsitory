/**
 * The commit graph's hovered lane segment (#837), outside React state: a hover renders only the
 * lane graphics whose look changes (`CommitLaneGraphic` subscribes), not the whole graph.
 */
export interface LaneHover {
  get(): number | null;
  set(segmentId: number | null): void;
  subscribe(listener: () => void): () => void;
}

export function createLaneHover(): LaneHover {
  let hovered: number | null = null;
  const listeners = new Set<() => void>();
  return {
    get: () => hovered,
    set(segmentId) {
      if (segmentId === hovered) return;
      hovered = segmentId;
      for (const listener of listeners) listener();
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}

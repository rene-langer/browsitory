export function loadPersistedOpen(storageKey: string, defaultOpen: boolean): boolean {
  const stored = localStorage.getItem(storageKey);
  if (stored === "open") return true;
  if (stored === "closed") return false;
  return defaultOpen;
}

export function persistOpen(storageKey: string, open: boolean): void {
  localStorage.setItem(storageKey, open ? "open" : "closed");
}

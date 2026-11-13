// Stub for the test-first commit (#900); the implementation commit replaces it.
export function parseKeybindingsJson(_text: string): unknown[] {
  return [];
}
export function normalizeVscodeKey(key: string): string {
  return key;
}
export function effectiveShortcutKeys(
  _contributed: readonly unknown[],
  _user: readonly unknown[],
  _isMac: boolean,
): Record<string, string[]> {
  return {};
}
export function keybindingsFileCandidates(_globalStorage: string): string[] {
  return [];
}

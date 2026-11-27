// Not implemented yet (#1144); the tests in fixtureLock.test.ts describe it.

export function acquireFixtureLock(lockDir: string, hint = ""): () => void {
  void lockDir;
  void hint;
  return () => {};
}

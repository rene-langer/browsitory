// Stub so the tests of #1032 run red; the implementation follows.
export interface FailedOperation {
  failed: string;
  next: string;
}
export class FailureWithNextStep extends Error {}
const none = (): FailedOperation => ({ failed: "", next: "" });
export const failedOperation = new Proxy({} as Record<string, (...args: unknown[]) => FailedOperation>, {
  get: () => none,
});

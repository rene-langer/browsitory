// Stub for the tests-first commit (#843); the implementation follows.
import type { WorkerLike } from "./highlightWorkerProtocol";

export interface BlobWorkerDeps {
  fetch(url: string): Promise<Response>;
  createObjectURL(blob: Blob): string;
  createWorker(url: string): WorkerLike;
}

export function highlightWorkerScriptUrl(_script: { src: string } | null, _baseUri: string): string {
  return "";
}

export function blobWorker(_url: string, _deps?: BlobWorkerDeps): WorkerLike {
  return { postMessage() {}, addEventListener() {}, terminate() {} };
}

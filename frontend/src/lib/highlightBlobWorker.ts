import type { HighlightRequest, HighlightResponse, WorkerLike } from "./highlightWorkerProtocol";

/**
 * The VS Code webview's highlight worker (#843). A webview can start a worker only from a blob:
 * (or data:) URL, not from the extension's files, and the worker can't import further code. The
 * VS Code build therefore ships the worker as one self-contained file next to its bundle
 * (`vite.vscode.config.ts`); this fetches it (the CSP's `connect-src` allows the webview's own
 * files), wraps it in a blob and starts the worker from that (`worker-src blob:`).
 */

export const HIGHLIGHT_WORKER_FILE = "highlight-worker.js";

/** The worker file sits next to the bundle's classic script, in `<build root>/assets/`. */
export function highlightWorkerScriptUrl(script: { src: string } | null, baseUri: string): string {
  if (script !== null && script.src !== "") return new URL(HIGHLIGHT_WORKER_FILE, script.src).href;
  return new URL(`assets/${HIGHLIGHT_WORKER_FILE}`, baseUri).href;
}

export interface BlobWorkerDeps {
  fetch(url: string): Promise<Response>;
  createObjectURL(blob: Blob): string;
  createWorker(url: string): WorkerLike;
}

const browserDeps: BlobWorkerDeps = {
  fetch: (url) => fetch(url),
  createObjectURL: (blob) => URL.createObjectURL(blob),
  createWorker: (url) => new Worker(url) as unknown as WorkerLike,
};

/**
 * A worker that starts once its script has loaded. Messages posted meanwhile are passed on when
 * it starts; a script that can't be fetched, or a worker that can't start, is reported as an
 * `error` event, as a worker that fails to load would.
 */
export function blobWorker(url: string, deps: BlobWorkerDeps = browserDeps): WorkerLike {
  const queued: HighlightRequest[] = [];
  const messageListeners: ((event: { data: HighlightResponse }) => void)[] = [];
  const errorListeners: ((event: unknown) => void)[] = [];
  let started: WorkerLike | null = null;
  let terminated = false;

  const reportError = (error: unknown) => {
    for (const listener of errorListeners) listener(error);
  };

  deps
    .fetch(url)
    .then(async (response) => {
      if (!response.ok) throw new Error(`Could not load the syntax highlighting worker (HTTP ${response.status})`);
      const blob = new Blob([await response.text()], { type: "text/javascript" });
      if (terminated) return;
      const worker = deps.createWorker(deps.createObjectURL(blob));
      worker.addEventListener("message", (event) => {
        for (const listener of messageListeners) listener(event);
      });
      worker.addEventListener("error", reportError);
      started = worker;
      for (const message of queued.splice(0)) worker.postMessage(message);
    })
    .catch((error: unknown) => {
      if (!terminated) reportError(error);
    });

  return {
    postMessage(message) {
      if (started !== null) started.postMessage(message);
      else queued.push(message);
    },
    addEventListener(type: "message" | "error", listener: (event: never) => void) {
      if (type === "message") messageListeners.push(listener as (event: { data: HighlightResponse }) => void);
      else errorListeners.push(listener as (event: unknown) => void);
    },
    terminate() {
      terminated = true;
      started?.terminate();
    },
  };
}

import { describe, expect, it, vi } from "vitest";
import { blobWorker, highlightWorkerScriptUrl } from "./highlightBlobWorker";
import type { HighlightRequest, HighlightResponse } from "./highlightWorkerProtocol";

// #843: a VS Code webview can start a worker only from a blob: (or data:) URL, not from the
// extension's files. The VS Code build ships the worker as one file next to its bundle; the
// page fetches it, wraps it in a blob and starts the worker from that.

describe("highlightWorkerScriptUrl", () => {
  it("finds the worker file next to the bundle's script", () => {
    expect(
      highlightWorkerScriptUrl(
        { src: "https://file+.vscode-resource.vscode-cdn.net/ext/frontend/dist-vscode/assets/vscode-main.js" },
        "vscode-webview://panel/index.html",
      ),
    ).toBe("https://file+.vscode-resource.vscode-cdn.net/ext/frontend/dist-vscode/assets/highlight-worker.js");
  });
});

/** A started worker, as the page sees it. */
class StartedWorker {
  readonly posted: HighlightRequest[] = [];
  readonly listeners = new Map<string, ((event: unknown) => void)[]>();
  terminated = false;
  postMessage(message: HighlightRequest) {
    this.posted.push(message);
  }
  addEventListener(type: string, listener: (event: unknown) => void) {
    this.listeners.set(type, [...(this.listeners.get(type) ?? []), listener]);
  }
  terminate() {
    this.terminated = true;
  }
  emit(type: string, event: unknown) {
    for (const listener of this.listeners.get(type) ?? []) listener(event);
  }
}

function deps(response: Promise<Response>) {
  const started: StartedWorker[] = [];
  return {
    started,
    fetch: vi.fn(() => response),
    createObjectURL: vi.fn(() => "blob:vscode-webview://panel/1234"),
    createWorker: vi.fn((url: string) => {
      const worker = new StartedWorker();
      started.push(worker);
      void url;
      return worker;
    }),
  };
}

describe("blobWorker", () => {
  it("starts the worker from a blob of the fetched script and passes on what was posted meanwhile", async () => {
    const d = deps(Promise.resolve(new Response("self.onmessage = () => {};")));
    const worker = blobWorker("https://res/assets/highlight-worker.js", d);

    worker.postMessage({ type: "languages", id: 1 });
    worker.postMessage({ type: "cancel", id: 1 });

    await vi.waitFor(() => expect(d.started).toHaveLength(1));
    expect(d.fetch).toHaveBeenCalledWith("https://res/assets/highlight-worker.js");
    expect(d.createObjectURL).toHaveBeenCalledTimes(1);
    expect(d.createWorker).toHaveBeenCalledWith("blob:vscode-webview://panel/1234");
    expect(d.started[0].posted).toEqual([
      { type: "languages", id: 1 },
      { type: "cancel", id: 1 },
    ]);

    worker.postMessage({ type: "languages", id: 2 });
    expect(d.started[0].posted).toHaveLength(3);
  });

  it("passes the worker's messages and errors on", async () => {
    const d = deps(Promise.resolve(new Response("")));
    const worker = blobWorker("https://res/w.js", d);
    const messages: HighlightResponse[] = [];
    const errors: unknown[] = [];
    worker.addEventListener("message", (event) => messages.push(event.data));
    worker.addEventListener("error", (event) => errors.push(event));

    await vi.waitFor(() => expect(d.started).toHaveLength(1));
    d.started[0].emit("message", { data: { id: 1, languages: [] } });
    d.started[0].emit("error", new Event("error"));

    expect(messages).toEqual([{ id: 1, languages: [] }]);
    expect(errors).toHaveLength(1);
  });

  it("reports an error when the script can't be fetched", async () => {
    const d = deps(Promise.resolve(new Response("", { status: 404 })));
    const worker = blobWorker("https://res/w.js", d);
    const errors: unknown[] = [];
    worker.addEventListener("error", (event) => errors.push(event));

    await vi.waitFor(() => expect(errors).toHaveLength(1));
    expect(d.createWorker).not.toHaveBeenCalled();
  });

  it("reports an error when the worker can't be started", async () => {
    const d = deps(Promise.resolve(new Response("")));
    d.createWorker.mockImplementation(() => {
      throw new Error("blocked by CSP");
    });
    const worker = blobWorker("https://res/w.js", d);
    const errors: unknown[] = [];
    worker.addEventListener("error", (event) => errors.push(event));

    await vi.waitFor(() => expect(errors).toHaveLength(1));
  });

  it("never starts a worker terminated while its script loads", async () => {
    const d = deps(Promise.resolve(new Response("")));
    const worker = blobWorker("https://res/w.js", d);
    worker.terminate();

    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(d.createWorker).not.toHaveBeenCalled();
  });
});

/// <reference lib="webworker" />
import { createTokenizer } from "./highlightTokenizer";
import { createHighlightWorkerHandler, type HighlightRequest } from "./highlightWorkerProtocol";

// The highlight worker (#843): Shiki, its grammars and the tokenizing run here, off the page's
// main thread. The desktop app starts it as a module worker (grammars load as separate chunks);
// the VS Code build bundles it into one file and starts it from a blob: (highlightBlobWorker.ts).
const scope = self as unknown as DedicatedWorkerGlobalScope;
const handle = createHighlightWorkerHandler(createTokenizer(), (message) => scope.postMessage(message));
scope.addEventListener("message", (event: MessageEvent<HighlightRequest>) => handle(event.data));

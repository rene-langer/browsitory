import { fileURLToPath } from "node:url";
import { build, type Plugin } from "vite";
import { excludeGrammarsPlugin } from "./vite.grammars.ts";

const ROOT = fileURLToPath(new URL(".", import.meta.url));
// Must match HIGHLIGHT_WORKER_FILE in src/lib/highlightBlobWorker.ts, next to the bundle.
const OUTPUT = "assets/highlight-worker.js";

/**
 * The VS Code build's syntax highlighting worker (#843). A webview can start a worker only from
 * a blob: URL and the worker can't import further code, so it is built here as one classic
 * script with every grammar in it, and written next to the bundle; the page fetches it and
 * starts it from a blob (src/lib/highlightBlobWorker.ts). Vite's own `?worker` imports don't
 * fit: they reference `import.meta.url`, which the classic bundle can't contain.
 */
export function vscodeHighlightWorkerPlugin(): Plugin {
  return {
    name: "browsitory-vscode-highlight-worker",
    apply: "build",
    async generateBundle() {
      const result = await build({
        configFile: false,
        root: ROOT,
        publicDir: false,
        logLevel: "warn",
        define: { "import.meta.url": JSON.stringify(""), "import.meta.resolve": "undefined" },
        plugins: [excludeGrammarsPlugin()],
        build: {
          write: false,
          emptyOutDir: false,
          rollupOptions: {
            input: "src/lib/highlight.worker.ts",
            output: { format: "iife", inlineDynamicImports: true, entryFileNames: "highlight-worker.js" },
          },
        },
      });
      const outputs = Array.isArray(result) ? result : [result];
      const chunk = outputs
        .flatMap((o) => ("output" in o ? o.output : []))
        .find((file) => file.type === "chunk" && file.isEntry);
      if (chunk === undefined || chunk.type !== "chunk") this.error("the highlight worker build produced no script");
      this.emitFile({ type: "asset", fileName: OUTPUT, source: chunk.code });
    },
  };
}

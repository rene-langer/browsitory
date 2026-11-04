import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import type { Plugin } from "vite";

// Keep in sync with THIRD_PARTY_NOTICES_FILE in src/lib/thirdPartyNotices.ts.
const FILE_NAME = "third-party-notices.md";
const SOURCE = fileURLToPath(new URL("../THIRD_PARTY_NOTICES.md", import.meta.url));

/**
 * Puts THIRD_PARTY_NOTICES.md (generated, at the repository root) at the root of the build as a
 * separate static file, and serves it in `vite dev`. The app's "Third-party licences" dialog
 * fetches it on demand, so the text stays out of the script bundle.
 */
export function thirdPartyNoticesPlugin(): Plugin {
  return {
    name: "browsitory-third-party-notices",
    configureServer(server) {
      server.middlewares.use(`/${FILE_NAME}`, (_req, res) => {
        res.setHeader("Content-Type", "text/markdown; charset=utf-8");
        res.end(readFileSync(SOURCE));
      });
    },
    generateBundle() {
      this.emitFile({ type: "asset", fileName: FILE_NAME, source: readFileSync(SOURCE) });
    },
  };
}

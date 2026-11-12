import type { Plugin } from "vite";
import { EXCLUDED_GRAMMARS } from "./src/lib/excludedGrammars.ts";

const VIRTUAL_PREFIX = "\0browsitory-excluded-grammar:";
const LANGS_DIST = /[\\/]@shikijs[\\/]langs[\\/]dist[\\/]/;

/**
 * Keeps the grammars in `EXCLUDED_GRAMMARS` out of the build. Shiki's language modules import
 * the grammars they embed (`cpp.mjs` imports `./glsl.mjs`); this plugin resolves such an
 * import to an empty list instead, and `highlightTokenizer.ts` puts Browsitory's own grammar in
 * its place (`OWN_GRAMMARS` in excludedGrammars.ts, #326). The build
 * then fails if an excluded grammar's definition still made it into any output file, so a
 * Shiki update that pulls one in another way can't ship it unnoticed.
 *
 * Only `vite build` is covered: `vite dev` pre-bundles dependencies with its own resolver, so
 * a dev server may still load the excluded modules. `highlight.ts` never registers them, and
 * nothing from a dev server ships.
 */
export function excludeGrammarsPlugin(): Plugin {
  const names = new Set<string>(EXCLUDED_GRAMMARS.map((g) => g.name));
  return {
    name: "browsitory-exclude-grammars",
    enforce: "pre",
    resolveId(source, importer) {
      if (importer === undefined || !LANGS_DIST.test(importer)) return null;
      const match = /^\.\/([\w.+-]+)\.mjs$/.exec(source);
      return match !== null && names.has(match[1]) ? VIRTUAL_PREFIX + match[1] : null;
    },
    load(id) {
      return id.startsWith(VIRTUAL_PREFIX) ? "export default [];" : null;
    },
    generateBundle(_options, bundle) {
      for (const [fileName, output] of Object.entries(bundle)) {
        if (output.type !== "chunk") continue;
        for (const grammar of EXCLUDED_GRAMMARS) {
          // A grammar module defines its scope as `"scopeName":"<scope>"` inside a JSON string.
          const definition = new RegExp(`scopeName\\\\?":\\\\?"${grammar.scopeName.replaceAll(".", "\\.")}\\\\?"`);
          if (definition.test(output.code)) {
            this.error(`${fileName} contains the excluded "${grammar.name}" grammar (see src/lib/excludedGrammars.ts)`);
          }
        }
      }
    },
  };
}

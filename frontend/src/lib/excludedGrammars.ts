import type { LanguageRegistration } from "shiki/core";

/**
 * Shiki grammars Browsitory must never ship, with the scope name each one defines.
 *
 * - `glsl` has no licence at all (polym0rph/GLSL.tmbundle), so it can't be redistributed.
 *   Shiki's `cpp` grammar embeds it, and `cpp-macro` comes with `cpp`, so all three are out.
 *   C++ files use the C grammar instead (`languageForPath.ts`).
 *
 * Three places read this list: `highlightTokenizer.ts` drops these grammars (and references to them)
 * before loading a language, `vite.grammars.ts` keeps them out of both builds and fails a build
 * that still contains one, and `scripts/generate-third-party-notices.py` leaves them out of the
 * shipped grammar set. Keep it a plain array literal: the Python script parses it.
 */
export const EXCLUDED_GRAMMARS = [
  { name: "cpp", scopeName: "source.cpp" },
  { name: "cpp-macro", scopeName: "source.cpp.embedded.macro" },
  { name: "glsl", scopeName: "source.glsl" },
] as const;

export const EXCLUDED_GRAMMAR_NAMES: readonly string[] = EXCLUDED_GRAMMARS.map((g) => g.name);

/**
 * Drops the grammars Browsitory doesn't ship from a language module's grammar list, and from
 * the embedded languages the rest name, so Shiki doesn't ask for them. Code embedded in such a
 * language (C++ in a Ruby heredoc) then stays plain.
 */
export function withoutExcludedGrammars(langs: readonly LanguageRegistration[]): LanguageRegistration[] {
  const excluded = (name: string) => EXCLUDED_GRAMMAR_NAMES.includes(name);
  return langs
    .filter((lang) => !excluded(lang.name))
    .map((lang) =>
      lang.embeddedLangs?.some(excluded) ? { ...lang, embeddedLangs: lang.embeddedLangs.filter((n) => !excluded(n)) } : lang,
    );
}

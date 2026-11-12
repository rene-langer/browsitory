import type { LanguageRegistration } from "shiki/core";
import { GLSL_GRAMMAR } from "./grammars/glsl.ts";

/**
 * Shiki grammars Browsitory must never ship, with the scope name each one defines.
 *
 * - `glsl` has no licence at all (polym0rph/GLSL.tmbundle), so it can't be redistributed.
 *   Shiki's `cpp` grammar embeds it (GLSL in `R"glsl(...)glsl"` raw strings); Browsitory's own
 *   grammar (`grammars/glsl.ts`, `OWN_GRAMMARS` below) takes its place (#326).
 *
 * Three places read this list: `highlightTokenizer.ts` drops these grammars before loading a
 * language (and puts the own replacements in), `vite.grammars.ts` keeps them out of both builds
 * and fails a build that still contains one, and `scripts/generate-third-party-notices.py` leaves
 * them out of the shipped grammar set. Keep it a plain array literal: the Python script parses it.
 */
export const EXCLUDED_GRAMMARS = [{ name: "glsl", scopeName: "source.glsl" }] as const;

export const EXCLUDED_GRAMMAR_NAMES: readonly string[] = EXCLUDED_GRAMMARS.map((g) => g.name);

/**
 * Browsitory's own grammars, each registered under the name of an excluded one it replaces.
 * `scripts/generate-third-party-notices.py` lists them (its `OWN_GRAMMARS`) as Browsitory's own.
 */
export const OWN_GRAMMARS: Readonly<Record<string, LanguageRegistration>> = { glsl: GLSL_GRAMMAR };

/**
 * Drops the grammars Browsitory doesn't ship from a language module's grammar list. Where
 * another grammar embeds one that has an own replacement, the replacement goes first in the
 * list and the embed stays; an embed of an excluded grammar without one is dropped, so Shiki
 * doesn't ask for it (the embedded code then stays plain).
 */
export function withoutExcludedGrammars(langs: readonly LanguageRegistration[]): LanguageRegistration[] {
  const excluded = (name: string) => EXCLUDED_GRAMMAR_NAMES.includes(name);
  const replaced = (name: string) => excluded(name) && name in OWN_GRAMMARS;
  const kept = langs
    .filter((lang) => !excluded(lang.name))
    .map((lang) =>
      lang.embeddedLangs?.some((n) => excluded(n) && !replaced(n))
        ? { ...lang, embeddedLangs: lang.embeddedLangs.filter((n) => !excluded(n) || replaced(n)) }
        : lang,
    );
  const replacements = Object.keys(OWN_GRAMMARS)
    .filter((name) => kept.some((lang) => lang.embeddedLangs?.includes(name)))
    .map((name) => OWN_GRAMMARS[name]);
  return [...replacements, ...kept];
}

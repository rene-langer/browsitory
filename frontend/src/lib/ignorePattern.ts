/**
 * `text` (a path or part of one) escaped for a `.gitignore` line, so the line matches it and
 * nothing else (#1168): `\`, `[`, `*` and `?` are glob syntax there, and git drops trailing
 * spaces that no backslash escapes. A leading `#` or `!` needs the caller's leading `/`.
 */
export function escapeIgnorePattern(text: string): string {
  return text.replace(/[\\[*?]/g, (char) => `\\${char}`).replace(/ +$/, (spaces) => "\\ ".repeat(spaces.length));
}

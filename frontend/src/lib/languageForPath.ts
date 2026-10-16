export type HighlightLanguage =
  | "typescript" | "tsx" | "javascript" | "jsx" | "json" | "rust" | "python" | "go" | "java"
  | "c" | "cpp" | "csharp" | "css" | "scss" | "html" | "markdown" | "yaml" | "toml"
  | "shellscript" | "sql" | "ruby" | "php" | "kotlin" | "swift";

const BY_EXTENSION: Record<string, HighlightLanguage> = {
  ts: "typescript", mts: "typescript", cts: "typescript", tsx: "tsx",
  js: "javascript", mjs: "javascript", cjs: "javascript", jsx: "jsx",
  json: "json", rs: "rust", py: "python", go: "go", java: "java",
  c: "c", h: "c", cc: "cpp", cpp: "cpp", hpp: "cpp", cs: "csharp",
  css: "css", scss: "scss", html: "html", htm: "html", md: "markdown",
  yml: "yaml", yaml: "yaml", toml: "toml", sh: "shellscript", bash: "shellscript", zsh: "shellscript",
  sql: "sql", rb: "ruby", php: "php", kt: "kotlin", kts: "kotlin", swift: "swift",
};

export function languageForPath(path: string): HighlightLanguage | null {
  const name = path.split("/").pop() ?? "";
  const dot = name.lastIndexOf(".");
  if (dot <= 0) return null;
  return BY_EXTENSION[name.slice(dot + 1).toLowerCase()] ?? null;
}

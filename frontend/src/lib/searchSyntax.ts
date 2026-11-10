/**
 * Rows of the commit search syntax help. docs/guide/search.md has the same table, and the keys are
 * the backend's `SEARCH_KEYS`; `searchSyntax.test.ts` checks both.
 */
export const SEARCH_SYNTAX: { syntax: string; meaning: string }[] = [
  { syntax: "word", meaning: "Message contains it, or SHA starts with it" },
  { syntax: '"two words"', meaning: "Exact phrase" },
  { syntax: "author:NAME", meaning: "Author name or email" },
  { syntax: "committer:NAME", meaning: "Committer name or email" },
  { syntax: "message:TEXT", meaning: "Anywhere in the full message" },
  { syntax: "commit:SHA", meaning: "SHA starts with it" },
  { syntax: "path:PATTERN", meaning: "Changes a matching path (src/, *.md)" },
  { syntax: "contents:TEXT", meaning: "An added or removed line contains it" },
  { syntax: "line:PATH:FROM-TO", meaning: "Changed those lines of the file at HEAD, followed back" },
  { syntax: "after:YYYY-MM-DD", meaning: "Committed on or after that day" },
  { syntax: "before:YYYY-MM-DD", meaning: "Committed on or before that day" },
  { syntax: "branch:NAME", meaning: "Only commits reachable from NAME" },
  { syntax: "merges:only", meaning: "Only merge commits" },
  { syntax: "merges:none", meaning: "No merge commits" },
  { syntax: "a or b", meaning: "Either one matches" },
  { syntax: "-word", meaning: "Leaves out what matches it; also not word" },
  { syntax: "(a or b) c", meaning: "Parentheses group terms" },
];

export const SEARCH_SYNTAX_FOOTER = "Terms side by side combine with AND. Case is ignored except in path:.";

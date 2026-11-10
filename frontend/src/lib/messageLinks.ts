import type { ForgeRepository } from "../ipc/RepoClient";

/** A run of a message: plain text, or text that links to `url`. Joined, the texts are the message. */
export type MessageSegment = { kind: "text"; text: string } | { kind: "link"; text: string; url: string };

// An `http(s)://` URL up to the next whitespace or a character that can't be in one unescaped,
// or an issue reference: `#123` or `owner/repo#123` (an owner has no `.`, a repository may).
const CANDIDATE =
  /\bhttps?:\/\/[^\s<>"'`]+|(?:([A-Za-z0-9_-]+)\/([A-Za-z0-9_.-]+))?#([1-9][0-9]*)(?![A-Za-z0-9_])/gi;
// Characters a URL found in prose usually ends on without meaning them.
const TRAILING_PUNCTUATION = /[.,;:!?'"*_]+$/;
// A URL keeps a closing bracket it opened (`…/Foo_(bar)`), not one that closes the prose around it.
const BRACKETS = [
  ["(", ")"],
  ["[", "]"],
] as const;

/**
 * The forge repository a message's `#123` refers to (#612): the one of the remote the current
 * branch tracks, else `origin`'s, else the first one; `null` without a forge remote.
 */
export function issueForgeRepository(
  forgeRepositories: readonly ForgeRepository[],
  upstreamRemote: string | null,
): ForgeRepository | null {
  return (
    forgeRepositories.find((repository) => repository.remoteName === upstreamRemote) ??
    forgeRepositories.find((repository) => repository.remoteName === "origin") ??
    forgeRepositories[0] ??
    null
  );
}

/** The web page of issue `number` in `owner/name` on `forge`'s host. */
export function issueUrl(forge: ForgeRepository, owner: string, name: string, number: string): string {
  switch (forge.provider) {
    case "GitHub":
    case "Bitbucket":
      return `https://${forge.host}/${owner}/${name}/issues/${number}`;
  }
}

/** `url` without the punctuation it ends on in prose: `.`, `,`, … and a `)`/`]` it doesn't open. */
function trimUrl(url: string): string {
  let trimmed = url;
  for (;;) {
    let next = trimmed.replace(TRAILING_PUNCTUATION, "");
    for (const [open, close] of BRACKETS) {
      if (next.endsWith(close) && count(next, close) > count(next, open)) next = next.slice(0, -1);
    }
    if (next === trimmed) return trimmed;
    trimmed = next;
  }
}

function count(text: string, char: string): number {
  return text.split(char).length - 1;
}

/**
 * Splits a commit message into text and links (#612): `http://`/`https://` URLs (no other
 * scheme), and, when `forge` is known, `#123` and `owner/repo#123` issue references. A reference
 * right after a letter, digit, `_`, `/`, `.` or `#` (`abc#1`, `a/b/c#1`) is text, and so is one
 * without a forge.
 */
export function parseMessageLinks(message: string, forge: ForgeRepository | null): MessageSegment[] {
  const segments: MessageSegment[] = [];
  let textStart = 0;
  const pushText = (end: number) => {
    if (end > textStart) segments.push({ kind: "text", text: message.slice(textStart, end) });
  };
  for (const match of message.matchAll(CANDIDATE)) {
    const start = match.index;
    let text = match[0];
    let url: string | null;
    if (match[3] === undefined) {
      text = trimUrl(text);
      url = text.length > text.indexOf("//") + 2 ? text : null;
    } else {
      const before = start === 0 ? "" : message[start - 1];
      const attached = /[A-Za-z0-9_/.#]/.test(before);
      url =
        forge === null || attached
          ? null
          : issueUrl(forge, match[1] ?? forge.owner, match[2] ?? forge.name, match[3]);
    }
    if (url === null) continue;
    pushText(start);
    segments.push({ kind: "link", text, url });
    textStart = start + text.length;
  }
  pushText(message.length);
  return segments;
}

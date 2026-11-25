// The frontend's copy of `repo_service::oplog::redact` (#1056): what the app writes to its log
// or copies for a bug report loses the same credentials the operation log does.

const REDACTED = "***";
const AUTHORIZATION = "authorization:";

/**
 * Characters that end a URL in running text, as in `oplog::ends_url`. `'`, `(`, `)`, `,`, `[`
 * and `]` don't: a password may hold them unencoded (#1125).
 */
const URL_END = /["<>`\s]/;
const HOST_END = /[/?#]/;

/** `text.search(pattern)`, or `text.length` when nothing matches. */
function indexOrLength(text: string, pattern: RegExp): number {
  const index = text.search(pattern);
  return index === -1 ? text.length : index;
}

/** As `oplog::userinfo_end`: where the user information of `url` (the text after `://`) ends. */
function userinfoEnd(url: string): number | undefined {
  const hostEnd = indexOrLength(url, HOST_END);
  const before = url.slice(0, hostEnd);
  const at = before.lastIndexOf("@");
  if (at !== -1) return at;
  const colon = before.lastIndexOf(":");
  if (colon === -1) return undefined;
  const port = before.slice(colon + 1);
  if (before.startsWith("[") || /^\d+$/.test(port)) return undefined;
  const nextAt = url.indexOf("@", hostEnd);
  if (nextAt === -1) return undefined;
  const end = nextAt + indexOrLength(url.slice(nextAt), HOST_END);
  return url.lastIndexOf("@", end - 1);
}

function redactUserinfo(text: string): string {
  let out = "";
  let rest = text;
  for (let at = rest.indexOf("://"); at !== -1; at = rest.indexOf("://")) {
    const urlStart = at + 3;
    out += rest.slice(0, urlStart);
    const after = rest.slice(urlStart);
    const end = userinfoEnd(after.slice(0, indexOrLength(after, URL_END)));
    if (end === undefined) {
      rest = after;
    } else {
      out += REDACTED;
      rest = after.slice(end);
    }
  }
  return out + rest;
}

/** `line` with the value after `Authorization:` replaced, its line ending kept. */
function redactAuthorization(line: string): string {
  const at = line.toLowerCase().indexOf(AUTHORIZATION);
  if (at === -1) return line;
  const end = at + AUTHORIZATION.length;
  const ending = line.slice(line.replace(/[\r\n]+$/, "").length);
  return `${line.slice(0, end)} ${REDACTED}${ending}`;
}

/**
 * `text` without credentials: the user information of every URL in it (`scheme://user:pass@`
 * → `scheme://***@`) and the value of every `Authorization` header line.
 */
export function redactCredentials(text: string): string {
  const withoutUserinfo = redactUserinfo(text);
  if (!withoutUserinfo.toLowerCase().includes(AUTHORIZATION)) return withoutUserinfo;
  return (withoutUserinfo.match(/[^\n]*\n|[^\n]+$/g) ?? []).map(redactAuthorization).join("");
}

/**
 * `error` as text for the log, without credentials. A plain object (a command's `{ kind,
 * message }` payload) becomes JSON with each string redacted, so a redacted header value can't
 * swallow the JSON after it.
 */
export function redactedLogText(error: unknown): string {
  if (typeof error === "object" && error !== null && !(error instanceof Error)) {
    return JSON.stringify(error, (_key, value: unknown) => (typeof value === "string" ? redactCredentials(value) : value));
  }
  return redactCredentials(String(error));
}

// The frontend's copy of `repo_service::oplog::redact` (#1056): what the app writes to its log
// or copies for a bug report loses the same credentials the operation log does.

const REDACTED = "***";
const AUTHORIZATION = "authorization:";

/** Characters that end a URL's authority (`user:pass@host:port`), as in `oplog::ends_authority`. */
const AUTHORITY_END = /[/?#"'<>()[\]`,\s]/;

function redactUserinfo(text: string): string {
  let out = "";
  let rest = text;
  for (let at = rest.indexOf("://"); at !== -1; at = rest.indexOf("://")) {
    const authorityStart = at + 3;
    out += rest.slice(0, authorityStart);
    const after = rest.slice(authorityStart);
    const end = after.search(AUTHORITY_END);
    const authorityLength = end === -1 ? after.length : end;
    const authority = after.slice(0, authorityLength);
    const userEnd = authority.lastIndexOf("@");
    out += userEnd === -1 ? authority : `${REDACTED}${authority.slice(userEnd)}`;
    rest = after.slice(authorityLength);
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

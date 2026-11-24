/** Last path segment of a clone URL without `.git` — what `git clone` names the folder. */
export function deriveRepoName(url: string): string {
  const trimmed = url.trim().replace(/[\\/]+$/, "");
  const segment = trimmed.split(/[\\/:]/).pop() ?? "";
  return segment.replace(/\.git$/, "");
}

export function joinPath(parent: string, name: string): string {
  const separator = parent.includes("\\") && !parent.includes("/") ? "\\" : "/";
  const base = parent.replace(/[\\/]+$/, "");
  return `${base}${separator}${name}`;
}

/**
 * An https URL with a user name and no password (Bitbucket's copy URL, or a token passed as
 * the user name), split into the URL without it and the decoded name; null for any other URL.
 * The backend refuses a user name in the URL (#1052): git would keep it in `.git/config`.
 */
export function splitHttpsUserName(url: string): { url: string; username: string } | null {
  // The last `@` before the path ends the user information, as in a URL parser.
  const match = /^(https:\/\/)([^/?#]*)@([^/?#@]+.*)$/i.exec(url.trim());
  if (match === null) return null;
  const [, scheme, userinfo, rest] = match;
  if (userinfo === "" || userinfo.includes(":")) return null;
  let username = userinfo;
  try {
    username = decodeURIComponent(userinfo);
  } catch {
    // A malformed escape is kept as typed.
  }
  return { url: `${scheme}${rest}`, username };
}

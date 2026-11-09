export function validateRemoteUrls(fetchUrl: string, pushUrl: string | null) {
  for (const url of [fetchUrl, pushUrl]) {
    if (url === null) continue;
    const parsed = parseUrl(url);
    if (parsed === null) continue;
    if ((parsed.protocol === "http:" || parsed.protocol === "https:") && (parsed.username !== "" || parsed.password !== "")) {
      throw new Error("Remote URLs must not contain embedded credentials");
    }
  }
}

// `null` for what the browser can't parse as a URL (an scp-style `git@host:path`, a local
// path): the backend checks those.
function parseUrl(url: string): URL | null {
  try {
    return new URL(url);
  } catch {
    return null;
  }
}

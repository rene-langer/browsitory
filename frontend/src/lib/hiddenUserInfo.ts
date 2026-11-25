/**
 * Whether `url` is an http(s) URL whose user information the backend hid as `***`: a remote
 * saved outside the app with a user name or token in its URL is listed that way (#1081). Sent
 * back unchanged by Edit remote, it keeps the saved URL.
 */
export function hidesUserInfo(url: string): boolean {
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    return false;
  }
  return (parsed.protocol === "http:" || parsed.protocol === "https:") && parsed.username === "***" && parsed.password === "";
}

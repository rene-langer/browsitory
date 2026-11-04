/**
 * The third-party notices file (THIRD_PARTY_NOTICES.md at the repository root) ships as a
 * separate static file at the root of both frontend builds, not inside the script bundle: it
 * is several hundred KB. `thirdPartyNoticesPlugin` in `vite.notices.ts` puts it there.
 */
export const THIRD_PARTY_NOTICES_FILE = "third-party-notices.md";

/**
 * Where the notices file is served. The Tauri build loads its entry as a module script, so
 * `script` is null and the file sits next to the page. The VSCode webview loads the bundle as a
 * classic script from `<build root>/assets/`, while the page itself comes from elsewhere, so the
 * file is found relative to that script.
 */
export function thirdPartyNoticesUrl(script: { src: string } | null, baseUri: string): string {
  if (script !== null && script.src !== "") return new URL(`../${THIRD_PARTY_NOTICES_FILE}`, script.src).href;
  return new URL(THIRD_PARTY_NOTICES_FILE, baseUri).href;
}

/** Fetches the notices on first use and caches them; a failed fetch is retried next time. */
export function thirdPartyNoticesLoader(
  url: string,
  fetchImpl: (url: string) => Promise<Response> = (u) => fetch(u),
): () => Promise<string> {
  let cached: Promise<string> | null = null;
  return () => {
    cached ??= fetchImpl(url).then((response) => {
      if (!response.ok) throw new Error(`Could not load the licences (HTTP ${response.status})`);
      return response.text();
    });
    cached.catch(() => {
      cached = null;
    });
    return cached;
  };
}

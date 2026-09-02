export function validateRemoteUrls(fetchUrl: string, pushUrl: string | null) {
  for (const url of [fetchUrl, pushUrl]) {
    if (url === null) continue;
    try {
      const parsed = new URL(url);
      if ((parsed.protocol === "http:" || parsed.protocol === "https:") && (parsed.username !== "" || parsed.password !== "")) {
        throw new Error("Remote URLs must not contain embedded credentials");
      }
    } catch (error) {
      if (error instanceof Error && error.message === "Remote URLs must not contain embedded credentials") throw error;
    }
  }
}

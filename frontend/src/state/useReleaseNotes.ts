import { useEffect, useState } from "react";
import type { ReleaseNotesEntry } from "../components/ReleaseNotesModal";
import type { RepoClient } from "../ipc/RepoClient";
import releaseNotesData from "../generated/releaseNotes.json";

export const allReleaseNotes = releaseNotesData as ReleaseNotesEntry[];

/**
 * The release notes dialog: opens by itself after an update with notes the user hasn't seen,
 * and records the version as seen when that automatic view is closed.
 */
export function useReleaseNotes(client: RepoClient) {
  const [releaseNotesView, setReleaseNotesView] = useState<
    { mode: "auto" | "all"; entries: ReleaseNotesEntry[] } | null
  >(null);

  useEffect(() => {
    // Skip in E2E: a fresh run always has no last-seen version, so this would auto-open the
    // modal (a native <dialog>) on every launch and block interaction with the rest of the UI
    // that the E2E specs are actually testing.
    if (typeof import.meta.env.VITE_E2E_REPO_PATH === "string") return;
    let cancelled = false;
    async function checkVersion() {
      try {
        const [appVersion, lastSeenVersion] = await Promise.all([
          client.getAppVersion(),
          client.getLastSeenVersion(),
        ]);
        if (cancelled || appVersion === lastSeenVersion) return;
        const seenIndex =
          lastSeenVersion === null
            ? -1
            : allReleaseNotes.findIndex((entry) => entry.version === lastSeenVersion);
        // Releases with only developer-only changes have no groups and aren't shown.
        const withNotes = (list: ReleaseNotesEntry[]) => list.filter((entry) => entry.groups.length > 0);
        // A fresh install (or an unknown last-seen version) gets the newest release with notes.
        const entries =
          seenIndex === -1 ? withNotes(allReleaseNotes).slice(0, 1) : withNotes(allReleaseNotes.slice(0, seenIndex));
        if (entries.length === 0) {
          // Only developer-only releases since the last one seen: nothing to show users.
          await client.setLastSeenVersion(appVersion);
          return;
        }
        setReleaseNotesView({ mode: "auto", entries });
      } catch (error) {
        console.error("Release notes version check failed", error);
      }
    }
    void checkVersion();
    return () => {
      cancelled = true;
    };
  }, [client]);

  function closeReleaseNotes() {
    const wasAuto = releaseNotesView?.mode === "auto";
    setReleaseNotesView(null);
    if (!wasAuto) return;
    void client
      .getAppVersion()
      .then((version) => client.setLastSeenVersion(version))
      .catch((error) => console.error("Failed to record last-seen version", error));
  }

  return { releaseNotesView, setReleaseNotesView, closeReleaseNotes };
}

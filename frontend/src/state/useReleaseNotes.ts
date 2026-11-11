import { useCallback, useEffect, useRef, useState } from "react";
import type { ReleaseNotesEntry } from "../components/ReleaseNotesModal";
import type { RepoClient } from "../ipc/RepoClient";
import releaseNotesData from "../generated/releaseNotes.json";

export const allReleaseNotes = releaseNotesData as ReleaseNotesEntry[];

export interface ReleaseNotesOptions {
  /**
   * Set where the notes must not open by themselves (the VSCode extension, #680): called once
   * instead, with a function that opens them, and the version counts as seen right away.
   */
  announce?: (version: string, showNotes: () => void) => void;
  /** Whether a repository tab is open: opening the first one ends the welcome (#688). */
  repoOpen: boolean;
}

/**
 * The release notes dialog and the first launch's welcome (#688). With no last-seen version the
 * picker shows the welcome instead of release notes; dismissing it, or opening a repository,
 * records the version as seen. After an update the notes not seen yet open by themselves (or are
 * announced), and the automatic view records the version when it is closed.
 */
export function useReleaseNotes(client: RepoClient, { announce, repoOpen }: ReleaseNotesOptions) {
  const [releaseNotesView, setReleaseNotesView] = useState<
    { mode: "auto" | "all"; entries: ReleaseNotesEntry[] } | null
  >(null);
  const [welcome, setWelcome] = useState(false);
  // Read once, at the check: a new function from a later render changes nothing.
  const announceRef = useRef(announce);

  const recordSeen = useCallback(
    () =>
      client
        .getAppVersion()
        .then((version) => client.setLastSeenVersion(version))
        .catch((error: unknown) => console.error("Failed to record last-seen version", error)),
    [client],
  );

  useEffect(() => {
    // Skip in E2E: a fresh run always has no last-seen version, so this would show the welcome
    // or auto-open the modal (a native <dialog>) on every launch and get in the way of the UI
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
        if (lastSeenVersion === null) {
          // The first launch: a welcome, not the notes of a release the user never had.
          setWelcome(true);
          return;
        }
        const seenIndex = allReleaseNotes.findIndex((entry) => entry.version === lastSeenVersion);
        // Releases with only developer-only changes have no groups and aren't shown.
        const withNotes = (list: ReleaseNotesEntry[]) => list.filter((entry) => entry.groups.length > 0);
        // An unknown last-seen version gets the newest release with notes.
        const entries =
          seenIndex === -1 ? withNotes(allReleaseNotes).slice(0, 1) : withNotes(allReleaseNotes.slice(0, seenIndex));
        if (entries.length === 0) {
          // Only developer-only releases since the last one seen: nothing to show users.
          await client.setLastSeenVersion(appVersion);
          return;
        }
        const announceUpdate = announceRef.current;
        if (announceUpdate !== undefined) {
          announceUpdate(appVersion, () => setReleaseNotesView({ mode: "all", entries }));
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

  // Opening a repository ends the welcome: the user has found their way in.
  useEffect(() => {
    if (!welcome || !repoOpen) return;
    void recordSeen().finally(() => setWelcome(false));
  }, [welcome, repoOpen, recordSeen]);

  function dismissWelcome() {
    setWelcome(false);
    void recordSeen();
  }

  function closeReleaseNotes() {
    const wasAuto = releaseNotesView?.mode === "auto";
    setReleaseNotesView(null);
    if (!wasAuto) return;
    void recordSeen();
  }

  return { releaseNotesView, setReleaseNotesView, closeReleaseNotes, welcome, dismissWelcome };
}

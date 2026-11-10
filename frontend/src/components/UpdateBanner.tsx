import { useCallback, useContext, useEffect, useRef, useState } from "react";
import { checkForUpdate, type UpdateInfo } from "../ipc/updater";
import { UpdateCheckContext, type UpdateCheckOutcome } from "../state/updateCheck";
import { useUpdateSettingsContext } from "../state/useUpdateSettings";
import styles from "./UpdateBanner.module.css";

/** Time between checks while they succeed (no update found yet). */
const RECHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;
/** First retry after a failed check or download (#477). */
const FIRST_RETRY_DELAY_MS = 10 * 60 * 1000;
/** Longest retry delay; stays below RECHECK_INTERVAL_MS. */
const MAX_RETRY_DELAY_MS = 3 * 60 * 60 * 1000;

/** Delay before the next try after `failures` failed tries in a row (1 or more). */
function retryDelayMs(failures: number): number {
  return Math.min(FIRST_RETRY_DELAY_MS * 2 ** (failures - 1), MAX_RETRY_DELAY_MS);
}

const errorMessage = (error: unknown) => (error instanceof Error ? error.message : String(error));

/**
 * The desktop app's updater: checks for a new version, downloads one it finds and offers the
 * restart. It checks on its own at start and every 6 hours only while `[updates]
 * check_automatically` is on (#630); "Check for updates" (registered with `UpdateCheckContext`)
 * checks whenever asked.
 */
export function UpdateBanner() {
  const [readyUpdate, setReadyUpdate] = useState<UpdateInfo | null>(null);
  const { settings, status } = useUpdateSettingsContext();
  const registry = useContext(UpdateCheckContext);
  const ready = useRef<UpdateInfo | null>(null);
  const inFlight = useRef<Promise<UpdateCheckOutcome> | null>(null);

  // Checks and downloads a found update. A check already running is joined, not repeated, and
  // once an update is ready nothing is asked again.
  const checkAndDownload = useCallback((): Promise<UpdateCheckOutcome> => {
    if (ready.current !== null) return Promise.resolve({ kind: "ready", version: ready.current.version });
    if (inFlight.current !== null) return inFlight.current;
    const run = (async (): Promise<UpdateCheckOutcome> => {
      let update: UpdateInfo | null;
      try {
        update = await checkForUpdate();
      } catch (error) {
        console.error("Update check failed", error);
        return { kind: "failed", message: errorMessage(error) };
      }
      if (update === null) return { kind: "none" };
      try {
        await update.download();
      } catch (error) {
        console.error("Update download failed", error);
        return { kind: "failed", message: errorMessage(error) };
      }
      ready.current = update;
      setReadyUpdate(update);
      return { kind: "ready", version: update.version };
    })().finally(() => {
      inFlight.current = null;
    });
    inFlight.current = run;
    return run;
  }, []);

  useEffect(() => registry?.register(checkAndDownload), [registry, checkAndDownload]);

  // Skip in E2E builds (VITE_E2E_REPO_PATH is set at build time): the E2E suite and the guide
  // screenshots must not contact the update endpoints, download a release bundle, or show the
  // banner over the window (#576). Otherwise wait for the saved setting: the default (on) must
  // not check before a saved "off" is read, nor after the read failed.
  const automatic =
    typeof import.meta.env.VITE_E2E_REPO_PATH !== "string" && status === "loaded" && settings.checkAutomatically;

  useEffect(() => {
    if (!automatic) return;
    let cancelled = false;
    let timer: number | undefined;
    // Failed tries in a row; a successful check resets it.
    let failures = 0;

    // Checks and schedules the next check: RECHECK_INTERVAL_MS after a successful check, sooner
    // with backoff after a failed check or download (the update host can be down for a while,
    // #477). Stops once an update is ready to install, or when automatic checks are turned off.
    async function runCheck() {
      const outcome = await checkAndDownload();
      if (cancelled || outcome.kind === "ready") return;
      failures = outcome.kind === "failed" ? failures + 1 : 0;
      const delay = failures === 0 ? RECHECK_INTERVAL_MS : retryDelayMs(failures);
      timer = window.setTimeout(() => void runCheck(), delay);
    }

    void runCheck();
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [automatic, checkAndDownload]);

  if (readyUpdate === null) return null;

  return (
    <div className={styles.banner} role="status">
      <span>Update v{readyUpdate.version} ready</span>
      <button
        className={styles.restartButton}
        onClick={() =>
          void readyUpdate
            .installAndRelaunch()
            .catch((e) => console.error("Restart failed", e))
        }
      >
        Restart to update
      </button>
    </div>
  );
}

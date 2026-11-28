import { useCallback, useContext, useEffect, useRef, useState } from "react";
import { checkForUpdate, relaunchApp, type UpdateInfo } from "../ipc/updater";
import type { RepoClient } from "../ipc/RepoClient";
import { UpdateCheckContext, type UpdateCheckOutcome } from "../state/updateCheck";
import { useUpdateSettingsContext } from "../state/useUpdateSettings";
import { logFailure } from "../lib/logger";
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

/** Why the last install failed, and whether only a restart of the app helps now (#1194). */
interface InstallFailure {
  message: string;
  restartNeeded: boolean;
}

/**
 * The desktop app's updater: checks for a new version, downloads one it finds and offers the
 * restart. It checks on its own at start and every 6 hours only while `[updates]
 * check_automatically` is on (#630); "Check for updates" (registered with `UpdateCheckContext`)
 * checks whenever asked.
 */
export function UpdateBanner({ client }: { client: Pick<RepoClient, "recoverFromFailedUpdate"> }) {
  const [readyUpdate, setReadyUpdate] = useState<UpdateInfo | null>(null);
  const [installError, setInstallError] = useState<InstallFailure | null>(null);
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
        logFailure("Checking for updates failed", error);
        return { kind: "failed", message: errorMessage(error) };
      }
      if (update === null) return { kind: "none" };
      try {
        await update.download();
      } catch (error) {
        logFailure(`Downloading update v${update.version} failed`, error);
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

  // A failed install (rpm refusing the package, a cancelled password prompt, #1077) is shown and
  // logged; the button stays so the user can try again. Unless the updater got the app ready to
  // exit first (Windows, #1194): the backend shows the hidden window again, and only a restart
  // brings the repositories and the update back.
  const install = () => {
    setInstallError(null);
    readyUpdate.installAndRelaunch().catch(async (error: unknown) => {
      logFailure(`Installing update v${readyUpdate.version} failed`, error);
      let restartNeeded = false;
      try {
        restartNeeded = await client.recoverFromFailedUpdate();
      } catch (recoveryError) {
        logFailure("Recovering from the failed update install failed", recoveryError);
      }
      setInstallError({ message: errorMessage(error), restartNeeded });
    });
  };

  const restart = () => {
    relaunchApp().catch((error: unknown) => logFailure("Restarting Browsitory failed", error));
  };

  let message = `Update v${readyUpdate.version} ready`;
  if (installError?.restartNeeded) {
    message =
      `Couldn't start the installer for update v${readyUpdate.version}: ${installError.message.replace(/\.$/, "")}. ` +
      "Restart Browsitory to keep working, then try the update again.";
  } else if (installError !== null) {
    message = `Couldn't install update v${readyUpdate.version}: ${installError.message}`;
  }

  return (
    // No role of its own: it sits in the toast region, which is the live region (#1000).
    <div className={installError === null ? styles.banner : `${styles.banner} ${styles.failed}`}>
      <span>{message}</span>
      {installError?.restartNeeded ? (
        <button className={styles.restartButton} onClick={restart}>
          Restart Browsitory
        </button>
      ) : (
        <button className={styles.restartButton} onClick={install}>
          Restart to update
        </button>
      )}
    </div>
  );
}

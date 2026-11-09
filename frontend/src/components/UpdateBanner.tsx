import { useEffect, useState } from "react";
import { checkForUpdate, type UpdateInfo } from "../ipc/updater";
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

export function UpdateBanner() {
  const [readyUpdate, setReadyUpdate] = useState<UpdateInfo | null>(null);

  useEffect(() => {
    let cancelled = false;
    let timer: number | undefined;
    // Failed tries in a row; a successful check resets it.
    let failures = 0;

    // Checks, downloads a found update, and schedules the next check: RECHECK_INTERVAL_MS after
    // a successful check, sooner with backoff after a failed check or download (the update
    // host can be down for a while, #477). Stops once an update is ready to install.
    async function runCheck() {
      const outcome = await checkAndDownload();
      if (cancelled || outcome === "ready") return;
      failures = outcome === "failed" ? failures + 1 : 0;
      const delay = failures === 0 ? RECHECK_INTERVAL_MS : retryDelayMs(failures);
      timer = window.setTimeout(() => void runCheck(), delay);
    }

    async function checkAndDownload(): Promise<"ready" | "none" | "failed"> {
      let update: UpdateInfo | null;
      try {
        update = await checkForUpdate();
      } catch (error) {
        console.error("Update check failed", error);
        return "failed";
      }
      if (update === null) return "none";
      try {
        await update.download();
      } catch (error) {
        console.error("Update download failed", error);
        return "failed";
      }
      if (!cancelled) setReadyUpdate(update);
      return "ready";
    }

    void runCheck();
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, []);

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

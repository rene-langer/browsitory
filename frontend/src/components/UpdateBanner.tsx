import { useEffect, useRef, useState } from "react";
import { checkForUpdate, relaunchApp, type UpdateInfo } from "../ipc/updater";
import styles from "./UpdateBanner.module.css";

const RECHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;

export function UpdateBanner() {
  const [readyUpdate, setReadyUpdate] = useState<UpdateInfo | null>(null);
  const checking = useRef(false);

  useEffect(() => {
    async function runCheck() {
      if (checking.current) return;
      checking.current = true;
      try {
        const update = await checkForUpdate();
        if (update === null) return;
        await update.install();
        setReadyUpdate(update);
      } catch (error) {
        console.error("Update download failed", error);
      } finally {
        checking.current = false;
      }
    }

    void runCheck();
    const timer = window.setInterval(() => void runCheck(), RECHECK_INTERVAL_MS);
    return () => window.clearInterval(timer);
  }, []);

  if (readyUpdate === null) return null;

  return (
    <div className={styles.banner} role="status">
      <span>Update v{readyUpdate.version} ready</span>
      <button className={styles.restartButton} onClick={() => void relaunchApp()}>
        Restart to update
      </button>
    </div>
  );
}

import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export interface UpdateInfo {
  version: string;
  download: () => Promise<void>;
  installAndRelaunch: () => Promise<void>;
}

/**
 * Asks the updater for a newer version: the update, or `null` when there is none.
 * Rejects when the check fails (for example no endpoint answered), so the caller can tell a
 * failed check from "no update" and retry sooner (#477).
 */
export async function checkForUpdate(): Promise<UpdateInfo | null> {
  const update = await check();
  if (update === null) return null;
  return {
    version: update.version,
    download: () => update.download(),
    installAndRelaunch: async () => {
      await update.install();
      // On Windows, `install()` exits the process itself, so this line
      // never runs. On macOS/Linux it typically doesn't, so relaunch
      // explicitly as a fallback to complete the restart flow there too.
      await relaunch();
    },
  };
}

export async function relaunchApp(): Promise<void> {
  await relaunch();
}

import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export interface UpdateInfo {
  version: string;
  download: () => Promise<void>;
  installAndRelaunch: () => Promise<void>;
}

export async function checkForUpdate(): Promise<UpdateInfo | null> {
  try {
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
  } catch (error) {
    console.error("Update check failed", error);
    return null;
  }
}

export async function relaunchApp(): Promise<void> {
  await relaunch();
}

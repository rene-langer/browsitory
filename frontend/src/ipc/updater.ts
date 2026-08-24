import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export interface UpdateInfo {
  version: string;
  install: () => Promise<void>;
}

export async function checkForUpdate(): Promise<UpdateInfo | null> {
  try {
    const update = await check();
    if (update === null) return null;
    return {
      version: update.version,
      install: () => update.downloadAndInstall(),
    };
  } catch (error) {
    console.error("Update check failed", error);
    return null;
  }
}

export async function relaunchApp(): Promise<void> {
  await relaunch();
}

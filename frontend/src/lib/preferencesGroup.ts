/**
 * The Preferences dialog's groups (#784) and the one it showed last: a per-machine UI preference,
 * stored like the commit search view (`searchViewPreference.ts`), so the dialog reopens on it.
 */
export const PREFERENCES_GROUP_IDS = [
  "appearance",
  "diff",
  "commit-message",
  "fetch",
  "external-tools",
  "updates",
] as const;

export type PreferencesGroupId = (typeof PREFERENCES_GROUP_IDS)[number];

const STORAGE_KEY = "preferences-group";

// What a blocked storage can't keep.
let fallback: PreferencesGroupId | null = null;

function isGroupId(value: string | null): value is PreferencesGroupId {
  return value !== null && (PREFERENCES_GROUP_IDS as readonly string[]).includes(value);
}

/** The group shown last, `null` when none was (or the stored one is unknown). */
export function readPreferencesGroup(): PreferencesGroupId | null {
  if (fallback !== null) return fallback;
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    return isGroupId(stored) ? stored : null;
  } catch {
    return null;
  }
}

export function writePreferencesGroup(group: PreferencesGroupId): void {
  try {
    localStorage.setItem(STORAGE_KEY, group);
  } catch {
    // Blocked storage: the choice still holds until the app restarts.
    fallback = group;
  }
}

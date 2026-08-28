import { useState } from "react";

export type SidebarPanelId = "stash" | "worktree" | "submodule" | "reflog" | "tags" | "pullRequests";

export const SIDEBAR_PANEL_IDS: SidebarPanelId[] = [
  "stash",
  "worktree",
  "submodule",
  "reflog",
  "tags",
  "pullRequests",
];

const STORAGE_KEY = "sidebar.panels";

function defaults(): Record<SidebarPanelId, boolean> {
  return Object.fromEntries(SIDEBAR_PANEL_IDS.map((id) => [id, true])) as Record<SidebarPanelId, boolean>;
}

function loadVisibility(): Record<SidebarPanelId, boolean> {
  const result = defaults();
  const stored = localStorage.getItem(STORAGE_KEY);
  if (stored === null) return result;
  try {
    const parsed: unknown = JSON.parse(stored);
    if (typeof parsed !== "object" || parsed === null) return result;
    for (const id of SIDEBAR_PANEL_IDS) {
      const value = (parsed as Record<string, unknown>)[id];
      if (typeof value === "boolean") result[id] = value;
    }
    return result;
  } catch {
    return result;
  }
}

export function useSidebarPanelVisibility(): {
  visibility: Record<SidebarPanelId, boolean>;
  setPanelVisible: (id: SidebarPanelId, visible: boolean) => void;
} {
  const [visibility, setVisibility] = useState<Record<SidebarPanelId, boolean>>(loadVisibility);

  const setPanelVisible = (id: SidebarPanelId, visible: boolean) => {
    setVisibility((prev) => {
      const next = { ...prev, [id]: visible };
      localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
      return next;
    });
  };

  return { visibility, setPanelVisible };
}

import { useEffect, useState } from "react";
import type { AppState } from "./useAppState";

/**
 * `RepoWorkspace`'s window-level keyboard shortcuts: the command palette (Ctrl/Cmd+K), the
 * shortcut sheet (?), Undo/Redo and closing the tab. Owns the palette's and the sheet's open
 * state.
 */
export function useWorkspaceShortcuts({
  active,
  repositoryOperationDisabled,
  onCloseRepoTab,
  onReopenClosedTab,
  repoPath,
  handleUndo,
  handleRedo,
  pending,
  transfer,
}: {
  active: boolean;
  repositoryOperationDisabled: boolean;
  onCloseRepoTab: (path: string) => void;
  /** Ctrl/Cmd+Shift+T; absent when no tab was closed. */
  onReopenClosedTab?: () => void;
  repoPath: string;
  handleUndo: () => void;
  handleRedo: () => void;
  pending: boolean;
  transfer: AppState["transfer"];
}) {
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);

  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (!active) return;
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setPaletteOpen((prev) => !prev);
        return;
      }
      if (event.key === "?" && !event.metaKey && !event.ctrlKey && !event.altKey) {
        const target = event.target as HTMLElement | null;
        const tag = target?.tagName;
        if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || target?.isContentEditable) return;
        event.preventDefault();
        setShortcutsOpen(true);
        return;
      }
      const mod = event.metaKey || event.ctrlKey;
      const key = event.key.toLowerCase();
      const isUndo = mod && !event.altKey && key === "z" && !event.shiftKey;
      const isRedo = mod && !event.altKey && ((key === "z" && event.shiftKey) || (event.ctrlKey && !event.metaKey && key === "y"));
      if (isUndo || isRedo) {
        const target = event.target as HTMLElement | null;
        const tag = target?.tagName;
        // Text fields keep their native undo (commit message, branch-name inputs, …).
        if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || target?.isContentEditable) return;
        if (pending || transfer !== null) return;
        event.preventDefault();
        if (isUndo) handleUndo();
        else handleRedo();
        return;
      }
      // A11Y-003: keyboard equivalent for the per-tab close button, which is mouse-only
      // (`aria-hidden`/`tabIndex={-1}` in `RepoTabs.tsx`) so it doesn't sit in the tablist's
      // accessible children. This `RepoWorkspace` only ever handles its own tab's `repoPath`, and
      // only while `active` (checked above), so "the active repo" is just this one.
      // Best-effort only: Tauri's default macOS app menu binds Cmd+W to "Close Window" (the whole
      // app) before the webview sees the key, and a VSCode webview may forward Ctrl+W to the
      // workbench. The host-independent paths are Delete on a focused tab (`RepoTabs.tsx`) and
      // the palette's "Close tab" command.
      if (event.key.toLowerCase() === "w" && (event.metaKey || event.ctrlKey) && !event.altKey) {
        if (repositoryOperationDisabled) return;
        event.preventDefault();
        onCloseRepoTab(repoPath);
      }
      // Reopen closed tab (#613), best-effort like Ctrl/Cmd+W: a host may claim the key first;
      // the palette and the tab menu offer the same command.
      if (event.key.toLowerCase() === "t" && (event.metaKey || event.ctrlKey) && event.shiftKey && !event.altKey) {
        if (onReopenClosedTab === undefined) return;
        event.preventDefault();
        onReopenClosedTab();
      }
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [active, repositoryOperationDisabled, onCloseRepoTab, onReopenClosedTab, repoPath, handleUndo, handleRedo, pending, transfer]);

  return { paletteOpen, setPaletteOpen, shortcutsOpen, setShortcutsOpen };
}

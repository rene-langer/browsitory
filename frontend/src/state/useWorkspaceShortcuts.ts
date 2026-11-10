import { useEffect, useState } from "react";
import { navigationKeyDirection } from "../lib/navigationKeys";
import { onWorkspaceRequest } from "../lib/workspaceRequests";
import type { AppState } from "./useAppState";

/**
 * `RepoWorkspace`'s window-level keyboard shortcuts: the command palette (Ctrl/Cmd+K), the
 * shortcut sheet (?), Undo/Redo, closing and reopening the tab, and Back/Forward through the
 * visited commits (keys and the mouse's back/forward buttons), and the sidebar toggle. Owns the
 * palette's and the sheet's open state, which the header's Commands button and Help menu open
 * too (#647).
 */
export function useWorkspaceShortcuts({
  active,
  onToggleSidebar,
  repositoryOperationDisabled,
  onCloseRepoTab,
  onReopenClosedTab,
  onNavigateBack,
  onNavigateForward,
  repoPath,
  handleUndo,
  handleRedo,
  pending,
  transfer,
}: {
  active: boolean;
  /** Ctrl/Cmd+Alt+B (#635): shows or hides the sidebar, a drawer in a narrow layout. */
  onToggleSidebar?: () => void;
  repositoryOperationDisabled: boolean;
  onCloseRepoTab: (path: string) => void;
  /** Ctrl/Cmd+Shift+T; absent when no tab was closed. */
  onReopenClosedTab?: () => void;
  /** Back/Forward through the visited commits (#618); no-ops at the ends. */
  onNavigateBack?: () => void;
  onNavigateForward?: () => void;
  repoPath: string;
  handleUndo: () => void;
  handleRedo: () => void;
  pending: boolean;
  transfer: AppState["transfer"];
}) {
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);

  useEffect(() => {
    if (!active) return;
    return onWorkspaceRequest((request) => {
      if (request === "palette") setPaletteOpen(true);
      else setShortcutsOpen(true);
    });
  }, [active]);

  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (!active) return;
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setPaletteOpen((prev) => !prev);
        return;
      }
      // Best-effort like Ctrl/Cmd+W: a host may claim the key first; the toolbar button and the
      // palette's "Show sidebar" / "Hide sidebar" do the same. `code`, because Alt changes `key`
      // on macOS.
      if ((event.metaKey || event.ctrlKey) && event.altKey && !event.shiftKey && (event.code === "KeyB" || event.key.toLowerCase() === "b")) {
        if (onToggleSidebar === undefined) return;
        event.preventDefault();
        onToggleSidebar();
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
      // Back/Forward (#618): Alt+Left/Right, Cmd+[ / Cmd+] on macOS. Best-effort too: a host may
      // claim the keys first (a webview may hand Alt+Left to the workbench's own Back); the
      // toolbar buttons and the palette offer the same. Not in text fields, and not where a
      // control already used the arrow (a tab strip, a divider).
      const direction = navigationKeyDirection(event);
      if (direction !== null) {
        const target = event.target as HTMLElement | null;
        const tag = target?.tagName;
        if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || target?.isContentEditable) return;
        if (event.defaultPrevented || (target instanceof Element && target.getAttribute("role") === "separator")) return;
        event.preventDefault();
        if (direction === -1) onNavigateBack?.();
        else onNavigateForward?.();
      }
    }
    // The mouse's back (3) and forward (4) buttons; preventing the default keeps a webview from
    // navigating the page itself.
    function handleMouseUp(event: MouseEvent) {
      if (!active || (event.button !== 3 && event.button !== 4)) return;
      event.preventDefault();
      if (event.button === 3) onNavigateBack?.();
      else onNavigateForward?.();
    }
    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("mouseup", handleMouseUp);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("mouseup", handleMouseUp);
    };
  }, [active, onToggleSidebar, repositoryOperationDisabled, onCloseRepoTab, onReopenClosedTab, onNavigateBack, onNavigateForward, repoPath, handleUndo, handleRedo, pending, transfer]);

  return { paletteOpen, setPaletteOpen, shortcutsOpen, setShortcutsOpen };
}

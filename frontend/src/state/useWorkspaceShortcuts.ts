import { useEffect, useRef, useState } from "react";
import { matchShortcut, type ShortcutId } from "../lib/keybindings";
import { navigationKeyDirection } from "../lib/navigationKeys";
import { focusAdjacentPane, paneOf } from "../lib/paneFocus";
import { onWorkspaceRequest } from "../lib/workspaceRequests";
import type { AppState } from "./useAppState";

/** The shortcuts that run a palette command (`Command.shortcut`), with the same availability. */
const COMMAND_SHORTCUTS: ReadonlySet<ShortcutId> = new Set<ShortcutId>([
  "refresh",
  "fetch",
  "pull",
  "push",
  "stage-all",
  "focus-commit-message",
]);

function isTextField(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || target.isContentEditable;
}

/** In a dialog, a menu or the palette, which own their keys. */
function inOverlay(target: EventTarget | null): boolean {
  return target instanceof Element && target.closest('dialog, [role="dialog"], [role="alertdialog"], [role="menu"]') !== null;
}

/**
 * `RepoWorkspace`'s window-level keyboard shortcuts (`lib/keybindings`): the command palette, the
 * shortcut sheet (?), Undo/Redo, closing and reopening the tab, Back/Forward through the visited
 * commits (keys and the mouse's back/forward buttons), the sidebar toggle, and (#646, #670)
 * Refresh, Fetch, Pull, Push, Stage all and the commit message (through the palette's commands,
 * `runCommand`), switching tabs and F6 between the panes. Owns the palette's and the sheet's open
 * state, which the header's Commands button and Help menu open too (#647).
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
  runCommand,
  tabs,
  workspaceElement = null,
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
  /** Runs the palette command marked with this shortcut, or says why it can't run (#646). */
  runCommand?: (shortcut: ShortcutId) => void;
  /** The open tabs in strip order, for Ctrl+Tab and Ctrl/Cmd+1…9 (#646). */
  tabs?: { paths: readonly string[]; onSwitch: (path: string) => void };
  /** The workspace's root, whose `data-pane` elements F6 cycles through (#670). */
  workspaceElement?: HTMLElement | null;
}) {
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  // Read when a key is pressed, so a new render doesn't re-add the listeners.
  const latest = useRef({ runCommand, tabs });
  useEffect(() => {
    latest.current = { runCommand, tabs };
  });
  // The element that last had the focus in each pane, which F6 returns to (#670).
  const lastFocused = useRef(new WeakMap<Element, Element>());

  useEffect(() => {
    if (!active) return;
    return onWorkspaceRequest((request) => {
      if (request === "palette") setPaletteOpen(true);
      else if (request === "shortcuts") setShortcutsOpen(true);
    });
  }, [active]);

  useEffect(() => {
    if (workspaceElement === null) return;
    const remember = (event: FocusEvent) => {
      const target = event.target;
      if (!(target instanceof Element)) return;
      const pane = paneOf(target);
      if (pane !== null) lastFocused.current.set(pane, target);
    };
    workspaceElement.addEventListener("focusin", remember);
    return () => workspaceElement.removeEventListener("focusin", remember);
  }, [workspaceElement]);

  useEffect(() => {
    /** Ctrl+Tab, Ctrl+Shift+Tab and Ctrl/Cmd+1…9; whether the key was used. */
    function switchTab(event: KeyboardEvent, id: ShortcutId): boolean {
      const tabs = latest.current.tabs;
      if (tabs === undefined || tabs.paths.length < 2) return false;
      const index = tabs.paths.indexOf(repoPath);
      let next: string | undefined;
      if (id === "next-tab" || id === "previous-tab") {
        const step = id === "next-tab" ? 1 : -1;
        next = tabs.paths[(index + step + tabs.paths.length) % tabs.paths.length];
      } else {
        // Ctrl/Cmd+9 is the last tab, as in browsers.
        const digit = Number(event.code.startsWith("Digit") ? event.code.slice(5) : event.key);
        next = digit === 9 ? tabs.paths[tabs.paths.length - 1] : tabs.paths[digit - 1];
      }
      if (next === undefined) return false;
      if (next !== repoPath) tabs.onSwitch(next);
      return true;
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (!active) return;
      const shortcut = matchShortcut(event);
      const target = event.target;
      // Text fields keep their own keys (native undo in the commit message, "?" typed, …).
      if (shortcut === null || (!shortcut.inTextFields && isTextField(target))) return;
      switch (shortcut.id) {
        case "palette":
          event.preventDefault();
          setPaletteOpen((prev) => !prev);
          return;
        case "toggle-sidebar":
          // Best-effort like Ctrl/Cmd+W: a host may claim the key first; the toolbar button and
          // the palette's "Show sidebar" / "Hide sidebar" do the same.
          if (onToggleSidebar === undefined) return;
          event.preventDefault();
          onToggleSidebar();
          return;
        case "show-shortcuts":
          event.preventDefault();
          setShortcutsOpen(true);
          return;
        case "undo":
        case "redo":
          if (pending || transfer !== null) return;
          event.preventDefault();
          if (shortcut.id === "undo") handleUndo();
          else handleRedo();
          return;
        case "close-tab":
          // A11Y-003: keyboard equivalent for the per-tab close button, which is mouse-only
          // (`aria-hidden`/`tabIndex={-1}` in `RepoTabs.tsx`) so it doesn't sit in the tablist's
          // accessible children. This `RepoWorkspace` only ever handles its own tab's `repoPath`,
          // and only while `active` (checked above), so "the active repo" is just this one.
          // Best-effort only: Tauri's default macOS app menu binds Cmd+W to "Close Window" (the
          // whole app) before the webview sees the key, and a VSCode webview may forward Ctrl+W
          // to the workbench. The host-independent paths are Delete on a focused tab
          // (`RepoTabs.tsx`) and the palette's "Close tab" command.
          if (repositoryOperationDisabled) return;
          event.preventDefault();
          onCloseRepoTab(repoPath);
          return;
        case "reopen-closed-tab":
          // Reopen closed tab (#613), best-effort like Ctrl/Cmd+W; the palette and the tab menu
          // offer the same command.
          if (onReopenClosedTab === undefined) return;
          event.preventDefault();
          onReopenClosedTab();
          return;
        case "go-back":
        case "go-forward": {
          // Back/Forward (#618): Alt+Left/Right, Cmd+[ / Cmd+] on macOS. Best-effort too: a host
          // may claim the keys first (a webview may hand Alt+Left to the workbench's own Back);
          // the toolbar buttons and the palette offer the same. Not where a control already used
          // the arrow (a tab strip, a divider).
          const direction = navigationKeyDirection(event);
          if (direction === null) return;
          if (event.defaultPrevented || (target instanceof Element && target.getAttribute("role") === "separator")) return;
          event.preventDefault();
          if (direction === -1) onNavigateBack?.();
          else onNavigateForward?.();
          return;
        }
        case "next-tab":
        case "previous-tab":
        case "tab-by-number":
          if (inOverlay(target)) return;
          if (switchTab(event, shortcut.id)) event.preventDefault();
          return;
        case "next-pane":
        case "previous-pane":
          if (inOverlay(target) || workspaceElement === null) return;
          event.preventDefault();
          focusAdjacentPane(workspaceElement, shortcut.id === "next-pane" ? 1 : -1, lastFocused.current);
          return;
        default: {
          // Refresh, Fetch, Pull, Push, Stage all and the commit message (#646): the palette's
          // commands, so they follow its rules (`operationBlockers`).
          if (!COMMAND_SHORTCUTS.has(shortcut.id)) return;
          if (inOverlay(target) || event.defaultPrevented) return;
          const runCommand = latest.current.runCommand;
          if (runCommand === undefined) return;
          event.preventDefault();
          if (!event.repeat) runCommand(shortcut.id);
        }
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
  }, [
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
    workspaceElement,
  ]);

  return { paletteOpen, setPaletteOpen, shortcutsOpen, setShortcutsOpen };
}

import { GLOBAL_SHORTCUTS, formatChords, shortcutLabel, type GlobalShortcut, type KeyChord } from "./keybindings";
import { isApplePlatform, isVscodeHost } from "./platform";

/** One row of the shortcut sheet: its keys and what they do, written for the platform (#683). */
export interface ShortcutEntry {
  keys: string;
  description: string;
}

export interface ShortcutGroup {
  title: string;
  shortcuts: ShortcutEntry[];
}

const k = (key: string, modifiers: Omit<KeyChord, "key"> = {}): KeyChord => ({ key, ...modifiers });
const UP = k("ArrowUp");
const DOWN = k("ArrowDown");
const LEFT = k("ArrowLeft");
const RIGHT = k("ArrowRight");
const SHIFT_UP = k("ArrowUp", { shift: true });
const SHIFT_DOWN = k("ArrowDown", { shift: true });
const MENU_KEYS = [k("ContextMenu"), k("F10", { shift: true })];

function globalEntry(shortcut: GlobalShortcut, apple: boolean, vscode: boolean): ShortcutEntry {
  // In VSCode the extension contributes these keys for the Browsitory panel (#679), so they work.
  const claimed = shortcut.hostMayClaim === true && !(vscode && shortcut.vscodeKeybinding === true);
  const description = claimed ? `${shortcut.description} (where the host doesn't claim it)` : shortcut.description;
  return { keys: shortcutLabel(shortcut.id, apple), description };
}

/**
 * Every keyboard shortcut the app implements, for the help sheet, written for the platform. The
 * global ones come from `GLOBAL_SHORTCUTS`; keep the others in sync with their handlers.
 */
export function shortcutGroups(apple: boolean = isApplePlatform(), vscode: boolean = isVscodeHost()): ShortcutGroup[] {
  // Chords, or words such as "A letter".
  const row = (keys: readonly KeyChord[] | string, description: string): ShortcutEntry => ({
    keys: typeof keys === "string" ? keys : formatChords(keys, apple),
    description,
  });
  return [
    {
      title: "Global",
      shortcuts: [
        ...GLOBAL_SHORTCUTS.map((shortcut) => globalEntry(shortcut, apple, vscode)),
        row("Mouse back / forward buttons", "Back / forward through the selected commits"),
      ],
    },
    {
      title: "Repository tabs",
      shortcuts: [
        row([LEFT, RIGHT], "Switch to the previous / next tab"),
        row([k("Home"), k("End")], "Switch to the first / last tab"),
        row([k("Delete")], apple ? "Close the focused tab (Fn+⌫ on a laptop)" : "Close the focused tab"),
        row([k("F2")], "Rename the focused tab"),
      ],
    },
    {
      title: "Commit history",
      shortcuts: [
        row([DOWN, k("j")], "Select next commit"),
        row([UP, k("k")], "Select previous commit"),
        row([SHIFT_DOWN, SHIFT_UP], "Extend the selection"),
        row([k("Home"), k("End")], "Jump to first / last commit"),
        row([k("PageUp"), k("PageDown")], "Move by a page"),
        row([k("["), k("]")], "Go to the parent / child commit"),
        row([k("[", { shift: true })], "Choose one of a merge commit's parents"),
        row([RIGHT, LEFT], "Expand / collapse a collapsed merge (← on its commits: go to the merge)"),
        row([k("Enter"), ...MENU_KEYS], "Open the commit context menu"),
        row([k("f", { mod: true })], "Search commits"),
      ],
    },
    {
      title: "Command palette",
      shortcuts: [
        row([UP, DOWN], "Move the highlight"),
        row([k("PageUp"), k("PageDown")], "Move the highlight by a page"),
        row([k("Home", { mod: true }), k("End", { mod: true })], "First / last command (Home / End before you type)"),
        row([k("Enter")], "Run the highlighted command"),
        row([k("Backspace")], "Back from a list such as Go to branch… (with nothing typed)"),
      ],
    },
    {
      title: "Context menus",
      shortcuts: [
        row(MENU_KEYS, "Open the focused item's menu"),
        row([UP, DOWN], "Move between menu items"),
        row([k("Home"), k("End")], "First / last menu item"),
        row("A letter", "Jump to the next item starting with it"),
        row([k("Enter"), k(" ")], "Run the focused menu item"),
        row([k("Escape")], "Close the menu and return to where it opened"),
        row([k("Tab")], "Close the menu and move on"),
      ],
    },
    {
      title: "Changed files",
      shortcuts: [
        row([DOWN, k("j")], "Select next file (on into the other group at the end)"),
        row([UP, k("k")], "Select previous file (on into the other group at the start)"),
        row([SHIFT_DOWN, SHIFT_UP], "Extend the selection"),
        row([k("a", { mod: true })], "Select every file in the list"),
        row([k("Escape")], "Keep only the current file selected"),
        row([k("s")], "Stage or unstage the selected files"),
        row([k("Enter")], "Move into the selected file's diff"),
        row(MENU_KEYS, "Open the selected file's menu"),
      ],
    },
    {
      title: "Diff",
      shortcuts: [
        row([UP, DOWN], "Move line cursor"),
        row([SHIFT_UP, SHIFT_DOWN], "Extend line selection"),
        row([k("s")], "Stage or unstage the selected lines or hunk"),
        row([k("d")], "Discard the selected lines or hunk (Undo brings them back)"),
        row([k("["), k("]")], "Previous / next hunk"),
        row([k("j"), k("k")], "Next / previous changed file (working tree)"),
      ],
    },
    {
      title: "Commit message",
      shortcuts: [row([k("Enter", { mod: true })], "Commit")],
    },
    {
      title: "Layout",
      shortcuts: [
        row([LEFT, RIGHT], "Resize a pane (on a divider)"),
        row("Double-click a divider", "Collapse or expand the sidebar"),
        row([k("Escape")], "Close a dialog or menu"),
      ],
    },
  ];
}

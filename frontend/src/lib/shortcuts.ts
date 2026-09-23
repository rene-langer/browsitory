export interface ShortcutGroup {
  title: string;
  shortcuts: { keys: string; description: string }[];
}

/** Every keyboard shortcut the app implements, for the help sheet. Keep in sync with the handlers. */
export const SHORTCUT_GROUPS: ShortcutGroup[] = [
  {
    title: "Global",
    shortcuts: [
      { keys: "Ctrl/Cmd+K", description: "Open the command palette" },
      { keys: "Ctrl/Cmd+W", description: "Close the active tab" },
      { keys: "?", description: "Show keyboard shortcuts" },
    ],
  },
  {
    title: "Commit history",
    shortcuts: [
      { keys: "Down / j", description: "Select next commit" },
      { keys: "Up / k", description: "Select previous commit" },
      { keys: "Shift+Down / Shift+Up", description: "Extend the selection" },
      { keys: "Home / End", description: "Jump to first / last commit" },
      { keys: "PageUp / PageDown", description: "Move by a page" },
      { keys: "Enter / Menu / Shift+F10", description: "Open the commit context menu" },
    ],
  },
  {
    title: "Changed files",
    shortcuts: [
      { keys: "Down / j", description: "Select next file" },
      { keys: "Up / k", description: "Select previous file" },
      { keys: "s", description: "Stage or unstage the selected file" },
    ],
  },
  {
    title: "Commit message",
    shortcuts: [{ keys: "Ctrl/Cmd+Enter", description: "Commit" }],
  },
  {
    title: "Layout",
    shortcuts: [
      { keys: "Left / Right (on a divider)", description: "Resize a pane" },
      { keys: "Double-click a divider", description: "Collapse or expand the sidebar" },
      { keys: "Esc", description: "Close a dialog or menu" },
    ],
  },
];

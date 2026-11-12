# Content guidelines

- Sentence case for buttons, menu items, headings and palette entries ("Continue rebase", "Open folder").
- One noun per concept: "Stash", "Uncommitted changes" (everything not committed yet, in two
  groups: "Changes", not staged, and "Staged"), "Workspace" (a saved group of repositories).
- One verb per action: "Switch to" a branch, or "Switch to (detached)" for a commit or tag; labels
  never say "Checkout" or "check out". The release notes are "What's new" everywhere.
- Disabled actions say why in visible text near the control, tied with `aria-describedby`; `title` alone is not enough.
- Errors name what failed and the next step; keep raw backend text as the message and add a plain-language `hint`.
- Positive results go to the toast region (`useToasts`), not to layout-shifting banners.
- Keyboard shortcuts render the platform modifier (⌘ on Apple, Ctrl elsewhere).

The last known Title Case labels ("Pull Requests", "Uncommitted Changes", "Release Notes",
"New Workspace" / "Edit Workspace", "Choose Root Folder") were changed to sentence case in #320.
New labels follow the rules above.

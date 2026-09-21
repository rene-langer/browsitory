# Content guidelines

- Sentence case for buttons, menu items, headings and palette entries ("Continue rebase", "Open folder").
- One noun per concept: "Stash", "Working tree changes", "Workspace" (a saved group of repositories).
- Disabled actions say why in visible text near the control, tied with `aria-describedby`; `title` alone is not enough.
- Errors name what failed and the next step; keep raw backend text as the message and add a plain-language `hint`.
- Positive results go to the toast region (`useToasts`), not to layout-shifting banners.
- Keyboard shortcuts render the platform modifier (⌘ on Apple, Ctrl elsewhere).

Known leftovers (owned by other areas): Title Case labels in the hunk actions, branch context menu and commit graph.

# Content guidelines

- Sentence case for buttons, menu items, headings and palette entries ("Continue rebase", "Open folder").
- One noun per concept: "Stash", "Working tree changes", "Workspace" (a saved group of repositories).
- Disabled actions say why in visible text near the control, tied with `aria-describedby`; `title` alone is not enough.
- Errors name what failed and the next step; keep raw backend text as the message and add a plain-language `hint`.
- Positive results go to the toast region (`useToasts`), not to layout-shifting banners.
- Keyboard shortcuts render the platform modifier (⌘ on Apple, Ctrl elsewhere).

Known leftovers (Title Case labels still in the UI, found by a source grep on 2026-09-23 — not
verified in every running screen): "Pull Requests" (sidebar section, its panel toggle and the
palette's "Go to Pull Requests"), "Uncommitted Changes" (commit graph row), "Release Notes"
(release notes dialog heading), "New Workspace" / "Edit Workspace" and "Choose Root Folder"
(workspace editor).

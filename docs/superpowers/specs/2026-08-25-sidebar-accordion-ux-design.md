# Sidebar Accordion UX — Design

GitHub issue: [rene-langer/browsitory#17](https://github.com/rene-langer/browsitory/issues/17)

## Background

The left-hand sidebar (`Sidebar.tsx`) hosts 7 `AccordionSection`s
(Branches/Stashes, Worktrees, Submodules, Reflog, Remotes, Tags, Pull
Requests). A UX review (issue #17) found 10 problems: every section
defaults closed with no first-launch content, headers carry no
counts, the component is named/commented as a mutually-exclusive
accordion but isn't, there's no expand/collapse-all, toggling has no
motion, one long section can push the rest of the sidebar out of view,
`PullRequestPanel` nests a second, inconsistent disclosure primitive
(`Panel`) inside its accordion body, the cited WAI-ARIA APG keyboard
pattern isn't actually implemented, and headers have no icons.

This spec covers all 9 improvement ideas from the issue in one pass.

## Decisions

Resolved via user Q&A during brainstorming:

- **Independent open/close stays** (no true mutual-exclusion
  accordion). The persisted per-key `localStorage` design depends on
  it. Only the misleading naming/comment is fixed.
- **Only "Branches" gets `defaultOpen`.** The rest stay closed by
  default; each section's open state is still overridden by whatever
  is in `localStorage` once the user has toggled it.
- **Keyboard nav: implement the full APG pattern** (roving tabindex,
  Up/Down/Home/End across sibling headers), not just soften the
  comment.
- **Expand-all/collapse-all: a small toolbar atop `Sidebar`**, not a
  modifier-click or palette-only command.
- **Section height: flexible space-sharing.** Open sections share the
  sidebar's remaining height via flex-grow, each scrolling internally,
  rather than a fixed `max-height` per section.
- **`PullRequestPanel`'s nested `Panel`s become nested
  `AccordionSection`s**, one per forge repository, inside their own
  scoped `AccordionGroup` — not a Panel restyle.

## Component & architecture changes

### `AccordionSection.tsx`
- New optional props: `icon?: LucideIcon`, `count?: number`. Rendered
  in the header next to the title (count dimmed, right-aligned).
- Chevron changes from swapping `ChevronDown`/`ChevronRight`
  components to a single `ChevronRight` rotated via CSS `transform`
  (enables a rotation transition; the swap approach can't animate).
- Comment above the heading is rewritten: keyboard behavior follows
  the WAI-ARIA APG accordion pattern, but *open state is independent
  per section by design*, not mutual exclusion — removes the
  misleading implication without changing behavior.

### `AccordionGroup` (new)
- A context component wrapping a set of sibling `AccordionSection`s.
  Used once around `Sidebar`'s 7 top-level sections, and once more
  inside `PullRequestPanel`'s per-repository list (a nested, separately
  scoped group — arrow-key nav among repo cards doesn't leak into the
  outer 7-section group, and vice versa).
- Each `AccordionSection` registers its header button with the nearest
  enclosing group on mount, unregisters on unmount.
- Implements roving tabindex: only one header in the group is a tab
  stop at a time; `ArrowUp`/`ArrowDown` move focus between sibling
  headers without toggling them; `Home`/`End` jump to the first/last
  header. This is the full APG accordion keyboard pattern.
- Exposes `expandAll()`/`collapseAll()`, driven by `Sidebar`'s new
  toolbar buttons; no-ops safely if the group has no registered
  sections.

### `Sidebar.tsx`
- Becomes a flex column. Gains a small toolbar row above the sections:
  "Expand all" / "Collapse all" icon buttons wired to its
  `AccordionGroup`.
- Layout for flexible space-sharing: `.section` is `flex: 0 0 auto`
  when closed; an *open* section's `.section` switches to
  `flex: 1 1 0%; min-height: 0`, and its `.body` is
  `flex: 1 1 auto; min-height: 0; overflow-y: auto`. Multiple open
  sections share the sidebar's remaining height equally, each
  scrolling independently; closed headers stay pinned and visible
  regardless of how much content is open above them.

### Motion
- Chevron rotation: CSS `transition: transform 120ms` on toggle.
- Body content still mounts only while open (unchanged from today —
  preserves existing `AccordionSection.test.tsx` assertions that
  closed content is absent from the DOM). On mount, the body gets a
  fade + slight slide-in keyframe (~120ms). Collapse stays instant
  (unmount, no exit animation — accepted trade-off to avoid an
  animation library and avoid breaking the "closed content isn't in
  the DOM" test/a11y guarantee).
- All transitions/keyframes are wrapped in
  `@media (prefers-reduced-motion: reduce)` and disabled there,
  falling back to today's instant toggle.

## Per-consumer changes

Icon and count wiring for each of the 7 `AccordionSection` call sites:

| Section | Icon | Count source |
|---|---|---|
| Branches | `GitBranch` | `branches.length` |
| Worktrees | `GitFork` | `worktrees.length` |
| Submodules | `Package` | `submodules.length` |
| Reflog | `History` | `entries.length` (0 until a reference is selected — matches today's empty state, not a new gap) |
| Remotes | `Cloud` | `remotes.length` |
| Tags | `Tag` | `tags.length` |
| Pull Requests | `GitPullRequest` | sum of listed PRs across forge repos (0 until "List pull requests" has been run at least once per repo — expected, not a bug) |

All icons are already dependencies (`lucide-react`) and several are
already used elsewhere in the codebase (e.g. `GitPullRequest` in
`PullRequestPanel`'s row list).

### `PullRequestPanel.tsx`
`ForgeRepositorySection`'s outer `<Panel title={sectionLabel} ...
headingLevel={3}>` becomes `<AccordionSection title={sectionLabel}
storageKey={`sidebar-pr-${repository.remoteName}`}>`, and the list of
`ForgeRepositorySection`s is wrapped in its own `<AccordionGroup>`
(nested inside the outer Pull Requests section, scoped only to the
per-repo cards). The `forgeRepositories.length === 0` empty state is
unchanged — it's already a plain `AccordionSection` with a message, no
nesting involved. `Panel` primitive itself is untouched; still used by
other, unrelated call sites.

## Edge cases

- Reduced-motion users get behavior identical to today's — pure
  regression-safety, not a new path needing its own extensive test
  matrix.
- `expandAll()`/`collapseAll()` on an empty/unregistered group is a
  safe no-op, not an error.
- `goToSidebarSection` (`commands.ts`) — the "Go to {section}" command
  palette lookup — is untouched by any of this; it queries
  `section[aria-label] button[aria-expanded]`, which still holds.

## Testing

- New unit tests for `AccordionGroup`: Arrow/Home/End move focus
  without toggling open state; `expandAll`/`collapseAll` open/close
  every registered section.
- Updated `AccordionSection.test.tsx` for icon/count rendering.
- `commands.test.ts` stays green unchanged (see Edge cases above).
- No E2E changes required — the existing E2E flow doesn't assert
  sidebar visuals.

## Out of scope

- The separately-filed Remotes panel review (issue #18) — different
  issue, different plan.
- Any change to which sections exist or what they contain.

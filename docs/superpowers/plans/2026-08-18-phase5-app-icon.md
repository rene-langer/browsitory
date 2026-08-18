# Phase 5 App Icon and Favicon Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the placeholder flat-blue desktop app icon set with a real
mark derived from `frontend/public/favicon.svg`'s existing angular logo, and
bring the favicon and in-app icon sprite onto the same accent palette as the
new design tokens.

**Architecture:** A new master SVG (`frontend/src-icon/app-icon.svg`) reuses
`favicon.svg`'s exact outer path — a solid, single-color, small-size-legible
version rather than its full blurred/gradient composition — on a rounded dark
background. `@tauri-apps/cli` (already a `frontend` devDependency) generates
the five files `crates/tauri-app/tauri.conf.json` already references from that
source. `favicon.svg` and `frontend/public/icons.svg` get their accent color
aligned to the new `--color-accent` token.

**Tech Stack:** `@tauri-apps/cli`'s `tauri icon` command; no new dependency.

**Spec:** `docs/superpowers/specs/2026-08-18-browsitory-phase5-design.md`
(see "App icon and favicon")

**Depends on:** None — independent of the Foundation and Rollout plans, but
uses the same `--color-accent` value the Foundation plan's
`frontend/src/styles/tokens.css` defines (`#7c3aed` light / `#a370ff` dark).
May be executed before, after, or in parallel with either.

## Global Constraints

- No `RepoClient` method, DTO, Tauri command, worker message, or `git-core`
  function is added, removed, or changed in shape by this plan.
- `crates/tauri-app/tauri.conf.json`'s `bundle.icon` array (lines 21-26) is
  not reordered or renamed — the same five paths must exist and be valid
  images afterward.
- `cargo build --workspace` must succeed after regenerating the icon set.

---

### Task 1: Master icon source and palette alignment

**Files:**
- Create: `frontend/src-icon/app-icon.svg`
- Modify: `frontend/public/favicon.svg` (accent color only)
- Modify: `frontend/public/icons.svg` (accent color only)

**Interfaces:**
- Produces: `frontend/src-icon/app-icon.svg`, the source Task 2 feeds to
  `tauri icon`.

- [ ] **Step 1: Write the master icon source**

Create `frontend/src-icon/` (a new directory, sibling to `frontend/public/`
— not itself served by Vite, since it's a generation input, not a runtime
asset) and write `frontend/src-icon/app-icon.svg`:

```svg
<svg xmlns="http://www.w3.org/2000/svg" width="48" height="46" viewBox="0 0 48 46">
  <rect width="48" height="46" rx="8" fill="#14131a"/>
  <path fill="#7c3aed" d="M25.946 44.938c-.664.845-2.021.375-2.021-.698V33.937a2.26 2.26 0 0 0-2.262-2.262H10.287c-.92 0-1.456-1.04-.92-1.788l7.48-10.471c1.07-1.497 0-3.578-1.842-3.578H1.237c-.92 0-1.456-1.04-.92-1.788L10.013.474c.214-.297.556-.474.92-.474h28.894c.92 0 1.456 1.04.92 1.788l-7.48 10.471c-1.07 1.498 0 3.579 1.842 3.579h11.377c.943 0 1.473 1.088.89 1.83L25.947 44.94z"/>
</svg>
```

This reuses `favicon.svg`'s exact outer silhouette path unmodified (a
"B"/lightning-bolt-like angular mark), flattened to a single solid fill —
the browser favicon's full blurred multi-ellipse gradient composition
reads as a soft blob at 16-32px, so the desktop icon needs the bold
single-shape version instead. Background is `#14131a` (the Foundation
plan's dark-theme `--color-bg`); mark fill is `#7c3aed` (the light-theme
`--color-accent`) — this combination stays legible against both a light
and a dark OS taskbar/dock.

- [ ] **Step 2: Align `favicon.svg`'s visible accent fill to the token**

In `frontend/public/favicon.svg`, replace both occurrences of `#863bff`
(the outer path's `fill` attribute and its matching inline `style`
`fill:#863bff;fill:color(display-p3 .5252 .23 1);fill-opacity:1`) with
`#7c3aed` and the equivalent `color(display-p3 ...)` value for `#7c3aed`
— compute it with:

Run: `python3 -c "
h = '7c3aed'
r, g, b = (int(h[i:i+2], 16) / 255 for i in (0, 2, 4))
print(f'color(display-p3 {r:.4f} {g:.4f} {b:.4f} / 1)')"`

Leave every other color in the file (the `#ede6ff`/`#7e14ff`/`#47bfff`
gradient ellipses) unchanged — they're decorative fill, not the brand
accent.

- [ ] **Step 3: Align `icons.svg`'s functional-icon stroke color to the
  token**

In `frontend/public/icons.svg`, replace every occurrence of `#aa3bff` (the
purple stroke used on the documentation/social icons) with `#7c3aed`.
Leave `#08060d` (the literal brand-mark icons — bluesky, discord, github,
x) unchanged; those colors are fixed by the third-party brands they
represent, not Browsitory's palette.

- [ ] **Step 4: Verify the SVGs still parse and render**

Run: `cd frontend && pnpm build`
Expected: succeeds — Vite's build copies/processes `public/` assets and
would fail on malformed SVG.

- [ ] **Step 5: Commit**

```bash
git add frontend/src-icon/app-icon.svg frontend/public/favicon.svg frontend/public/icons.svg
git commit -m "feat(frontend): add master app-icon source, align favicon/sprite accent color"
```

---

### Task 2: Regenerate the desktop icon set

**Files:**
- Modify: `crates/tauri-app/icons/32x32.png`
- Modify: `crates/tauri-app/icons/128x128.png`
- Modify: `crates/tauri-app/icons/128x128@2x.png`
- Modify: `crates/tauri-app/icons/icon.icns`
- Modify: `crates/tauri-app/icons/icon.ico`

**Interfaces:**
- Consumes: `frontend/src-icon/app-icon.svg` (Task 1).

- [ ] **Step 1: Confirm the icon generator is available**

Run: `cd frontend && pnpm exec tauri icon --help`
Expected: prints the `tauri icon` command's usage. If this fails because
`@tauri-apps/cli` isn't installed locally, run `cd frontend && pnpm install`
first (it's already a `devDependencies` entry in `frontend/package.json`).

- [ ] **Step 2: Generate the icon set**

Run:
```bash
cd frontend && pnpm exec tauri icon src-icon/app-icon.svg --output ../crates/tauri-app/icons
```
Expected: overwrites all five files listed in `crates/tauri-app/tauri.conf.json:21-26`
(`32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.icns`, `icon.ico`) —
confirm none is left with today's date only on some of the five (a partial
generation is a red flag; re-run if so).

- [ ] **Step 3: Verify every `tauri.conf.json`-referenced file exists and
  is non-empty**

Run:
```bash
for f in 32x32.png 128x128.png 128x128@2x.png icon.icns icon.ico; do
  ls -la "crates/tauri-app/icons/$f"
done
```
Expected: all five files exist and are larger than the placeholder sizes
recorded before this task (`32x32.png` was 104 bytes, `128x128.png` was
299 bytes, `128x128@2x.png` was 665 bytes — a real icon with the mark
should be visibly larger than each of those).

- [ ] **Step 4: Verify the workspace still builds**

Run: `cargo build --workspace`
Expected: succeeds — confirms `tauri-build`'s icon-embedding step accepts
the regenerated files.

- [ ] **Step 5: Visual check**

Run: `cargo tauri dev` (from `crates/tauri-app`, per `CLAUDE.md`'s
Commands section) and confirm the new mark appears in the OS
taskbar/dock/window-title-bar icon, not the old flat-blue square.

- [ ] **Step 6: Commit**

```bash
git add crates/tauri-app/icons/32x32.png crates/tauri-app/icons/128x128.png crates/tauri-app/icons/128x128@2x.png crates/tauri-app/icons/icon.icns crates/tauri-app/icons/icon.ico
git commit -m "feat(tauri-app): regenerate desktop app icon set from the new brand mark"
```

# Documentation screenshots

This folder makes the screenshots in `docs/guide/assets/`. It builds a fixed demo repository and
drives the desktop app with the E2E harness (tauri-driver and WebdriverIO), so the pictures can
be made again after the UI changes.

## Run it

Linux only, from the repository root. You need what the Tauri E2E suite needs (see "E2E: Tauri
app" in `AGENTS.md`): `tauri-driver`, `WebKitWebDriver`, `xvfb-run` and `dbus-run-session`.
`optipng` is optional; it makes the files smaller without changing a pixel.

```bash
e2e/screenshots/capture.sh              # build the E2E app, then capture
e2e/screenshots/capture.sh --no-build   # reuse an E2E build you already have
```

The build is the E2E build from `AGENTS.md`: the frontend with `VITE_E2E_REPO_PATH`, then
`cargo build` with the `custom-protocol` and `forge-fixture-override` features. The capture runs
`pnpm screenshots` in `e2e/` under `dbus-run-session -- xvfb-run`, on a 24-bit virtual screen,
so the window never opens on your desktop. When it exits, the script removes
`/tmp/browsitory-screenshots` (the demo repository and the temp HOME); run it with `KEEP=1` to
keep that folder.

Then look at every changed image before you commit it.

## What it does

- `demoRepo.ts` builds the demo repository `trailmap` in `/tmp/browsitory-screenshots/`. Its
  content, authors and dates are fixed: branches, a merge, tags, a remote (never contacted),
  two stashes, staged, unstaged and untracked files, and a branch that conflicts with `main`.
- `../wdio.screenshots.conf.ts` reuses the E2E config (`wdio.conf.ts`) and points `HOME`, the
  XDG folders, git's global config and the GnuPG home at a temp folder. The app's config and
  webview data already live in temp folders in the E2E harness. Nothing reads or writes your
  own settings, keys or Browsitory data.
- `capture.docs.ts` opens the demo from the repository picker, sets a 1280×800 window and the
  light theme, and saves each screen to `docs/guide/assets/<name>.png`. The last shot uses the dark
  theme.

## Shots

| File | Shows |
|---|---|
| `overview.png` | The main window: sidebar, commit graph, working tree changes |
| `staging.png` | A selected file's diff with a commit message typed |
| `commit-diff.png` | A commit from history with its diff |
| `branches.png` | The Branches section with the `origin` folder open |
| `stashes.png` | The Stashes section |
| `tags.png` | The Tags section |
| `command-palette.png` | The command palette, filtered to "branch" |
| `conflict.png` | A merge stopped on a conflict, with the resolve controls |
| `dark-theme.png` | The same conflict in the dark theme |

To add a shot, add a test to `capture.docs.ts` that ends with `shoot("<name>")`, run the
script, and add the file to this table.

The VSCode extension shows the same webview, so the user docs use these desktop shots for it
too.

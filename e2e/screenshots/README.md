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
so the window never opens on your desktop. Only the capture runs with `TMPDIR=/tmp`, because
the E2E build bakes `/tmp` paths into the app and `demoRepo.ts` uses `os.tmpdir()`; the build
keeps your own `TMPDIR`, so a small tmpfs `/tmp` doesn't fill up with compiler temp files. The
E2E build never checks for updates, so no "Update ready" banner lands in the shots and the
capture needs no network. When it exits, the script removes
`/tmp/browsitory-screenshots` (the demo repository); run it with `KEEP=1` to keep that folder.

Then look at every changed image before you commit it.

## What it does

- `demoRepo.ts` builds the demo repository `trailmap` in `/tmp/browsitory-screenshots/`. Its
  content, authors and dates are fixed: branches, a merge, tags, a remote (never contacted),
  two stashes, staged, unstaged and untracked files, and a branch that conflicts with `main`.
- `../wdio.screenshots.conf.ts` reuses the E2E config (`wdio.conf.ts`), which points `HOME`, the
  XDG folders, git's global config and the GnuPG home at a temp folder
  (`/tmp/browsitory-e2e-home`) and keeps the app's config and webview data in temp folders too.
  Nothing reads or writes your own settings, keys or Browsitory data. It adds English dates
  and the UTC time zone.
- `capture.docs.ts` opens the demo from the repository picker, sets a 1280×800 window and the
  light theme, and saves each screen to `docs/guide/assets/<name>.png`. The `dark-theme` shot uses
  the dark theme, and `merge-editor` a wider and taller window, with room for the three panes.

## Shots

| File | Shows |
|---|---|
| `overview.png` | The main window: sidebar, commit graph, working tree changes |
| `staging.png` | A selected file's diff with a commit message typed |
| `staging-stacked.png` | The same changes in the stacked view: every file's diff under the file list |
| `commit-diff.png` | A commit from history with its diff |
| `branches.png` | The Branches section with the `origin` folder open |
| `stashes.png` | The Stashes section |
| `tags.png` | The Tags section |
| `preferences.png` | Preferences, open on the Appearance group |
| `command-palette.png` | The command palette, filtered to "branch" |
| `preferences-keyboard.png` | Preferences, open on the Keyboard group |
| `preferences-commands.png` | Preferences, open on the Commands group: three commands added (two for every repository, one for the demo), the first open in its editor |
| `operation-log.png` | The operation log on a push its pre-push hook refused, with the hook's output (a local `backup` remote, removed again afterwards) |
| `conflict.png` | A merge stopped on a conflict, with the resolve controls |
| `dark-theme.png` | The same conflict in the dark theme |
| `merge-editor.png` | The merge editor on that conflict in its three panes, in a 1600×1000 window |
| `edit-commit.png` | Edit commit… stopped at the latest commit: the strip, the staged change and **Commit part** |

To add a shot, add a test to `capture.docs.ts` that ends with `shoot("<name>")`, run the
script, and add the file to this table.

The VSCode extension shows the same webview, so the user docs use these desktop shots for it
too.

## Screen inventory for design reviews

`capture.inventory.ts` (with `../wdio.inventory.conf.ts` and `inventoryHelpers.ts`) drives the
same E2E build through every screen, context menu, dialog and Preferences group it can reach,
in light and dark and at several window sizes, for UX reviews (#978). It writes to
`e2e/inventory-out/` (ignored by git) or to `INVENTORY_SHOTS`, never to `docs/guide/assets/`.
Build as for `capture.sh`, then from `e2e/`:

```bash
TMPDIR=/tmp dbus-run-session -- xvfb-run -a -s "-screen 0 1920x1200x24" pnpm screenshots:inventory
```

`INVENTORY_PHASES=A,B,…` limits the run to some phases. Each shot is its own test, and a failing
shot doesn't stop the run. The run resets the shared `/tmp/browsitory-e2e-*` fixture paths, so
it takes the E2E suite's lock: next to an E2E run it stops at once, naming that run (#1144).

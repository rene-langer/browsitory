#!/usr/bin/env bash
# Rebuilds the documentation screenshots in docs/guide/assets/. Linux only (tauri-driver needs
# WebKitWebDriver). See e2e/screenshots/README.md.
#
#   e2e/screenshots/capture.sh              # build, then capture
#   e2e/screenshots/capture.sh --no-build   # capture with the existing E2E build
#
# The demo repository and the temp HOME live in /tmp/browsitory-screenshots, which is removed
# when the script exits. Set KEEP=1 to keep it, for example to look at the demo repository.
set -euo pipefail

cleanup() {
  if [[ "${KEEP:-}" != "1" ]]; then
    rm -rf -- /tmp/browsitory-screenshots
  fi
}
trap cleanup EXIT

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

if [[ "${1:-}" != "--no-build" ]]; then
  # The E2E build steps from AGENTS.md, in the same order.
  (cd frontend && pnpm install --frozen-lockfile && VITE_E2E_REPO_PATH=/tmp/browsitory-e2e-repo pnpm build)
  cargo build --workspace --features tauri-app/custom-protocol,tauri-app/forge-fixture-override
fi

(cd e2e && pnpm install --frozen-lockfile)

# A 24-bit screen: xvfb-run's default is 8-bit, which bands the colours.
# Always under Xvfb, also on a desktop, so the window never opens on the real screen.
screen=(-s "-screen 0 1600x1000x24")
# The E2E build bakes /tmp paths into the app, and demoRepo.ts uses os.tmpdir(): keep them equal.
# Only for the capture: the build keeps your TMPDIR, so a small tmpfs /tmp can't fill up (#574).
(cd e2e && TMPDIR=/tmp dbus-run-session -- xvfb-run -a "${screen[@]}" pnpm screenshots)

# Lossless size reduction, if optipng is installed.
if command -v optipng >/dev/null; then
  optipng -quiet -o2 -strip all docs/guide/assets/*.png
else
  echo "optipng not found: the PNGs are not optimised" >&2
fi
ls -l docs/guide/assets/*.png

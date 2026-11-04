#!/usr/bin/env bash
# Rebuilds the documentation screenshots in docs/assets/. Linux only (tauri-driver needs
# WebKitWebDriver). See e2e/screenshots/README.md.
#
#   e2e/screenshots/capture.sh              # build, then capture
#   e2e/screenshots/capture.sh --no-build   # capture with the existing E2E build
set -euo pipefail

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
(cd e2e && dbus-run-session -- xvfb-run -a "${screen[@]}" pnpm screenshots)

# Lossless size reduction, if optipng is installed.
if command -v optipng >/dev/null; then
  optipng -quiet -o2 -strip all docs/assets/*.png
else
  echo "optipng not found: the PNGs are not optimised" >&2
fi
ls -l docs/assets/*.png

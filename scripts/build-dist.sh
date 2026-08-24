#!/usr/bin/env bash
# Build a standalone Browsitory distributable for the current OS (Linux/macOS).
#
# Produces the native bundle formats for the host platform via `cargo tauri build`:
#   Linux:   .deb, .rpm (if rpmbuild present), .AppImage
#   macOS:   .app, .dmg
#
# Usage: scripts/build-dist.sh [extra cargo-tauri build args]
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if ! command -v pnpm >/dev/null 2>&1; then
  echo "error: pnpm not found (https://pnpm.io)" >&2
  exit 1
fi

if ! cargo tauri --version >/dev/null 2>&1; then
  echo "error: tauri-cli not found; install with: cargo install tauri-cli" >&2
  exit 1
fi

echo "==> Installing frontend dependencies"
pnpm --dir "$repo_root/frontend" install --frozen-lockfile

echo "==> Building distributable"
(cd "$repo_root/crates/tauri-app" && cargo tauri build "$@")

echo "==> Done. Bundles in target/release/bundle/"

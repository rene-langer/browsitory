#!/usr/bin/env bash
# Sets up a development host for browsitory: Rust toolchain + the native GUI
# build dependencies eframe/winit and rfd need on Linux (X11/Wayland input,
# libgit2's build toolchain), plus Xvfb so GUI apps can be launched and
# screenshotted under an isolated virtual display instead of the real desktop
# (GNOME Wayland's screenshot D-Bus API refuses unsandboxed callers, so this
# is the automatable path for verifying UI changes). Safe to re-run.
set -euo pipefail

echo "==> Checking Rust toolchain"
if ! command -v rustc >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable --profile default
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
else
  echo "rustc already installed: $(rustc --version)"
fi
rustup component add rustfmt clippy >/dev/null 2>&1 || true

echo "==> Installing system packages"
os="$(uname -s)"
if [ "$os" = "Linux" ]; then
  if command -v apt-get >/dev/null 2>&1; then
    sudo apt-get update
    sudo apt-get install -y build-essential cmake pkg-config \
      libxkbcommon-dev libwayland-dev libx11-dev libxcb1-dev xvfb
  elif command -v dnf >/dev/null 2>&1; then
    sudo dnf install -y gcc gcc-c++ cmake pkgconfig \
      libxkbcommon-devel wayland-devel libX11-devel libxcb-devel xorg-x11-server-Xvfb
  elif command -v pacman >/dev/null 2>&1; then
    sudo pacman -S --needed --noconfirm base-devel cmake pkgconf \
      libxkbcommon wayland libx11 libxcb xorg-server-xvfb
  else
    echo "Unrecognized Linux package manager — install cmake, pkg-config, a C" >&2
    echo "toolchain, libxkbcommon/libwayland/libx11/libxcb dev headers, and Xvfb" >&2
    echo "manually." >&2
  fi
elif [ "$os" = "Darwin" ]; then
  if command -v brew >/dev/null 2>&1; then
    brew install cmake pkg-config
  else
    echo "Homebrew not found — install cmake and pkg-config manually, and ensure" >&2
    echo "Xcode Command Line Tools are installed (xcode-select --install)." >&2
  fi
else
  echo "Unrecognized OS '$os' — see docs/PROJECT_SETUP.md for manual setup steps." >&2
fi

echo "==> Verifying workspace builds"
cd "$(dirname "$0")/.."
cargo build --workspace

echo "==> Done. Run 'cargo run -p app' to launch Browsitory."

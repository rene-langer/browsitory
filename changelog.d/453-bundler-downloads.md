### Internal: build and test robustness

- Changed: CI's `build-dist` and the release's `build-release` cache the tools the Tauri bundler
  downloads (WiX and NSIS on Windows, the AppImage tools on Linux), keyed on the locked CLI
  version, and download what is missing before bundling with retries and hash checks
  (`scripts/prefetch-bundler-tools.py`), so a GitHub 504 no longer fails the bundling (#454).
- Fixed: the desktop E2E test that continues a paused rebase after an app restart could click
  **Continue rebase** while the restarted app was still settling. It now waits until the button
  has stayed in place for a second, allows 20 s for the rebase to finish, and names the status
  strip's last text and any error when it doesn't (#453).

### Settings on Windows

- Fixed: on Windows, saving a setting could fail with "Access is denied" while another window
  (a VS Code window, or the desktop app) was reading the settings file at that moment (#1219).
- Internal: the Tauri E2E "Push to…" flow waits for the app to report the push before it opens
  the remote's folder, checks that the folder opened, and records the folder's state if its
  branch row doesn't show (#1188).
- Internal: Renovate proposes patch and minor updates of the crate tools the workflows pin
  (cargo-audit, cargo-deny, cargo-about, cargo-mutants, tauri-driver); it used to propose only
  updates outside the pin's Cargo range (#1097).

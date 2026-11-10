### Internal

- Internal: `scripts/check-privacy-hosts.py` no longer misses hosts. It scans the whole text
  instead of skipping comments (a `//` inside a string or a leading `<!--` hid the rest of the
  line), matches the scheme in any case, finds protocol-relative `//host` URLs, flags URLs whose
  host isn't written after the scheme (`format!`, `concat!`, template literals) unless they are
  listed, skips the files of `#[cfg(test)] mod name;` modules, and also scans
  `frontend/index.html`, `frontend/public/`, `frontend/vite*.ts`, `.json` sources,
  `extension/package.json` and the Tauri capabilities (#706).
- Internal: CI's licence-doc and privacy-host checks run even when `cargo audit` or
  `pnpm audit` fails (#707).

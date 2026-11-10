### Internal: Screenshot capture and E2E builds

- Fixed: `e2e/screenshots/capture.sh` sets `TMPDIR=/tmp` only for the capture, so the build no
  longer fails when `/tmp` is a small tmpfs (#574).
- Fixed: E2E builds (`VITE_E2E_REPO_PATH`) skip the update check, so the E2E suite and the guide
  screenshots no longer contact the update endpoints, download a release bundle or show the
  "Update ready" banner (#576).

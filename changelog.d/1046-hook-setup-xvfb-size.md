### Internal

- Changed: CI and the documented headless E2E commands give `xvfb-run` an explicit
  1280x1024x24 screen, since its default differs by distribution (640x480 on Fedora, too small
  for the VS Code merge editor spec) (#1046).
- Fixed: the documented `pre-push` hook setup always links the main checkout's script, so the
  hook no longer dangles once the linked worktree that ran the setup is removed (#1049).

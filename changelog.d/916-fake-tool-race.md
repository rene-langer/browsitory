### Internal

- Internal: the desktop app's external tools end-to-end test no longer fails now and then on a
  diff tool launch it read while the fake tool was still writing it; a unit test of the fake
  tool runs in CI's `e2e` job (`pnpm test:support` in `e2e/`) (#916).

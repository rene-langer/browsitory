### E2E reliability

- Internal: the Tauri E2E specs set a field that starts with text (the reword and combined
  messages) inside the page with the new `setValueInPage` helper, so a render between
  WebKitWebDriver's clear and its typing no longer leaves the old text in front of the new
  (#783, #791).
- Internal: the cherry-pick sequence and stash specs click Continue and Pop with `clickInPage`,
  which waits while the button is blocked (`aria-disabled`), instead of a click that a blocked
  button ignores (#790).
- Internal: the history rewrite spec reports the history when its wait gives up, not when it
  started (#783).
- Internal: WebDriver's own clicks reach the whole 1200×800 E2E window, its bottom edge
  included; the bottom-edge failures seen on #749 no longer reproduce on main (#751).

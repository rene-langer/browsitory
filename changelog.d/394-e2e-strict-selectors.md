### Internal

- The desktop E2E suite and the documentation screenshot script run with WebdriverIO's strict
  selectors again: every `$` selector matches exactly one element, so a test can't act on the
  wrong one by accident (#394).

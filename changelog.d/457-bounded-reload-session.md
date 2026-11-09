### Internal: build and test robustness

- Fixed: a desktop E2E test that restarts the app could hang until mocha's timeout when the new
  WebDriver session never answered. The restart now gets 30 s per attempt and one retry with a
  freshly started tauri-driver, and the session hooks log the driver's pid, ports and the app's
  pid, so a hang shows where it stopped (#457).

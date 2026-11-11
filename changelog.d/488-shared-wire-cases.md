### Code clean-ups

- Internal: the wire snapshot tests of the desktop app and the VSCode sidecar take their shared
  DTO cases from one list (`crates/test-support/wire_cases.rs`), so a new DTO is snapshotted by
  both transports at once (#488).

### Internal: faster pull request checks

- Internal: CI cancels a pull request's older run when a new commit is pushed, skips the Rust
  jobs when a pull request changes no file they read, and skips every build and test job for
  documentation-only pull requests. The skipped jobs still report success, so the required
  checks keep their names; pushes to main, the schedule and dispatched runs run everything.

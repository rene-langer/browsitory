### Hook cancel on macOS

- Changed: when a hook is cancelled or runs out of time, Browsitory now kills its process group a
  second time once the hook itself has exited, to close a suspected race where a hook's background
  job could survive the cancel on macOS (#349).
- Internal: the hook timeout and cancel tests on Unix and Windows no longer depend on how fast the
  runner starts the hook's background job, which made them flaky on macOS CI (#349).

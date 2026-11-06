### Hook cancel on macOS

- Fixed: cancelling a hook, or a hook running out of time, could on macOS rarely leave a
  background job running that the hook was starting at that moment. Browsitory now kills the
  hook's process group a second time once the hook itself has exited (#349).
- Internal: the hook timeout and cancel tests no longer depend on how fast the runner starts
  the hook's background job, which made them flaky on macOS CI (#349).

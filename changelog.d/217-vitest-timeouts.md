### Internal: frontend tests under load

- Internal: frontend tests no longer time out on a busy machine (#217). A failed
  `findBy*`/`waitFor` retry no longer pretty-prints the whole document (5% of App.test.tsx's CPU
  time when idle, 15% under load). Testing Library waits up to 4 s instead of 1 s, the default
  test timeout is 10 s instead of 5 s, and App.test.tsx (whole-App renders) and the VSCode
  bootstrap test get 20 s.

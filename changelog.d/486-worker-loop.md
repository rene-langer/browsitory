### Internal: worker loop split up

- Changed: `Worker::spawn_paused` no longer holds the whole worker thread. Its state lives in a
  `WorkerLoop` struct (`worker/worker_loop.rs`) whose `step` runs each command through `acquire`
  (the state lock), `before` (undo settle/begin, held reply, operation gate), `dispatch` (the
  `match` over `Command`, `worker_loop/dispatch.rs`) and `after` (undo finish, lock release).
  The `probe!`/`rebase_slot!` macros and the `marker` closure became methods. No behaviour
  change (#486).

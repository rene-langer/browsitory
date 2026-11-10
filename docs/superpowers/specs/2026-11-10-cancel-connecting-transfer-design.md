# Cancel a fetch, pull or push that is still connecting (#547): design

## Problem

A cancel only reaches a transfer through libgit2's progress callbacks (`CancelRegistry`, polled
from `transfer_progress`/`sideband_progress`/`update_tips`, and `push_negotiation` for a push).
A manual Fetch, Fetch all, Pull or Push that is still *connecting* (a host that drops packets, DNS
that hangs, a server that accepts and never answers, the TLS/SSH handshake, the ref
advertisement) calls none of them, so Cancel did nothing until the OS gave up (60–120 s, or never
for a silent server), and the worker, with every command queued behind it, waited too. #462
solved this for the background fetch only (`docs/superpowers/specs/2026-11-09-background-fetch-unreachable-design.md`).

## Options considered

1. **Shorter libgit2 timeouts**: process-wide, so they would cut short a slow but working manual
   transfer, and race between threads. Rejected for the same reasons as in #462.
2. **A second mechanism for manual transfers** (e.g. a cancel that closes the socket). libgit2
   gives no handle on the socket of a connecting transfer. Rejected.
3. **Generalise the background-fetch helper to every network transfer** (chosen): one mechanism,
   already reasoned through for "an abandoned helper never writes".

## Design

### git-core: connect, ask, then transfer

`TransferReporter` gets `connected(&mut self) -> bool` (default `true`), replacing #462's
separate `ConnectWatch` trait and `fetch_remote_watched`: the reporter is already passed to every
transfer, so no public signature changes, and the background fetch uses `fetch_remote_as`
(`fetch_remote` reporting as another `TransferOperation`).

- Fetch (`fetch_with`, unchanged from #462): `Remote::connect_auth(Fetch)`, then
  `reporter.connected()`, then `fetch` over the same connection.
- Push (`push_refs`, used by Push, Push to…, force push, Push tags, delete remote branch/tag): the
  same split, `connect_auth(Push)`, `reporter.connected()` and `cancel()`, then `push` over the
  same connection (libgit2's `git_remote_upload` reuses a connected remote:
  `connect_or_reset_options`). Before `connected` answers, nothing has been sent, `pre-push` has
  not run, and nothing local (tracking refs, the upstream config of Push with "set upstream", the
  tracking ref a remote-branch delete removes) has been written. A network-level connect failure
  is `RemoteError::Unreachable` (still reported as `TransferFailed`), as for a fetch.

### repo-service: the transfer runs on a helper thread the worker may abandon

`worker/transfer_thread.rs` is #462's helper, generalised: `run(repo, credentials, remote_name,
Wait { op, events, connect_limit, prompt }, transfer)` opens a `Repository` and the remote's
credential provider on a helper thread, runs `transfer` there with a reporter and the cancel
check, and makes the worker poll it every 25 ms. The `Phase` handshake (`Connecting(since)` →
`Transferring`, or `Abandoned`, under one mutex) is unchanged: the worker can only abandon a
`Connecting` transfer, and `connected` only moves to `Transferring` while not abandoned, so an
abandoned helper never writes, sends or runs a hook. While connecting:

- a user cancel abandons the helper: the transfer ends `RemoteError::Cancelled`, the worker is
  free within one poll, and `Completed { error: Cancelled }` is sent (after `TransferOp::finish`,
  #567);
- with a `connect_limit` (background fetch only, 30 s) a connect that outlasts it is
  `RemoteError::Unreachable`.

Manual transfers pass `connect_limit: None` (libgit2's timeouts stay as they are) and
`prompt: true` (the OS credential store may prompt, as before); the background fetch passes its
limit and `prompt: false`. Progress is passed on stamped with the operation's ID (push progress
carries none of its own). The helper never holds the caller's event `Sender`.

Users of the helper:

- **Fetch**: the whole fetch on the helper.
- **Fetch all** and the **background fetch** share one loop (`worker/fetch_all.rs`, `Mode::Manual`
  or `Mode::Background`), each remote on its own helper. A cancel while one remote connects ends
  the whole Fetch all `Cancelled`. Backoff (`background_fetch::Memory`) is as in #462; a remote
  that answered ends its backoff for both modes, and only the background fetch starts one.
- **Pull**: the fetch of the upstream's remote on the helper; the fast-forward check, checkout
  and `post-merge` then run on the worker as before, after the helper finished.
- **Push**, force push, Push tags, delete remote branch/tag: the whole git-core call on the
  helper (`pre-push` included, after `connected`); `push.default` is still resolved on the worker
  first.

The worker still waits for the helper in every case except an abandon while connecting, so a
helper's writes never overlap another command. The commands, their policy rows
(`worker/policy.rs`) and the wire are unchanged. The frontend already shows the transfer panel,
with Cancel, from the operation's `Started` event, i.e. while connecting.

## Limits

- An abandoned helper stays blocked until the OS gives up on its socket (for a server that
  accepts and never answers, possibly the end of the session), holding a socket and an open
  `Repository`. Retrying the transfer starts a new helper; the old one never writes.
- A transfer that stalls after connecting is not abandoned (libgit2 may be writing then); a push
  is never cancelled after `push_negotiation` (#185's rule).

## Tests

- `git-core` (`tests/remote.rs`): a fetch and a push tell the reporter once connected; a reporter
  answering `false` stops a fetch and a push with `Cancelled` and nothing written (no remote
  branch, no tracking ref, no upstream config); a push to a refused port is `Unreachable`.
- `repo-service` worker (`tests/remote.rs`), with a local TCP listener that accepts and never
  answers: Fetch, Fetch all, Pull and Push to it are cancelled while connecting, the worker
  answers a status read at once, `Completed` is `Cancelled` (and Pull's reply), nothing is
  written. The existing transfer tests cover the normal paths.
- `transfer_thread::tests`: the phase handshake and when the worker gives up.

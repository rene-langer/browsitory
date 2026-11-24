# Cancel a fetch, pull or push that stalls after connecting (#1064): design

## Problem

#547 lets the worker give up on a transfer that is still *connecting*
(`docs/superpowers/specs/2026-11-10-cancel-connecting-transfer-design.md`). Once
`TransferReporter::connected` answered `true`, a cancel reaches the transfer only through
libgit2's progress callbacks, and they don't run while no data arrives. When the network goes
away mid-transfer without a reset (Wi-Fi change, a VPN that drops, a laptop that sleeps), the
socket read blocks: libgit2's server timeout is 0 (none). Cancel does nothing, and the worker
waits in `transfer_thread::run` until the app restarts: reads still answer, every write waits,
and a pull holds the state lock, so other windows wait too.

## Options considered

1. **libgit2's server timeout** (`GIT_OPT_SET_SERVER_TIMEOUT`, `git2::opts::
   set_server_timeout_in_milliseconds`): a socket read or write that gets nothing for that long
   fails. It ends every stall, with or without Cancel, but only after the timeout, and not over
   HTTP(S) on Windows: libgit2's WinHTTP transport sets WinHTTP's receive and send timeouts to
   infinite and ignores the option. SSH (libssh2's session timeout), `git://` and HTTP(S) on
   Linux and macOS (libgit2's socket stream under OpenSSL/SecureTransport) honour it.
   #462/#547 rejected *shorter connect* timeouts because they would cut short a slow but working
   connect; a read that gets no byte at all for minutes is not a working transfer (git servers
   send keep-alives every 5 s by default while they prepare a pack or run hooks). Set once at
   startup, before any thread uses libgit2, so nothing races on it.
2. **Let Cancel give up on a stalled transfer too**: the worker abandons the helper as it does
   while connecting. Prompt, and works on every transport, but an abandoned helper must still
   never write anything that overlaps another command.
3. **Close the socket from the worker**: libgit2 gives no handle on it. Rejected.

Chosen: 1 **and** 2. The timeout covers the stall nobody cancels (and lets an abandoned helper
end); Cancel covers Windows HTTPS and the user who doesn't want to wait.

## Design

### git-core

- `repo::configure_libgit2` also sets the server timeout to `SERVER_TIMEOUT` (120 s, the top of
  the issue's 60–120 s range: the cost of a false timeout, a failed transfer, is higher than a
  longer wait now that Cancel works). The connect timeout stays the OS's (#547's decision).
- `TransferReporter` gets `writing(&mut self) -> bool` (default `true`): asked once a transfer
  is about to write what other commands see. `false` stops it as `Cancelled` before it writes.
  - Fetch (`fetch_with`): `Remote::fetch` becomes the same steps libgit2's `git_remote_fetch`
    runs, split so the question fits between them: `download` (negotiate, receive, index; a new
    pack is renamed into place, which other commands never notice: no ref names its objects),
    `disconnect`, then `writing()` and `cancel()`, then `update_tips` (remote-tracking refs, tags,
    `FETCH_HEAD`; reflog message `fetch <remote>`, as before) and, when pruning applies
    (explicit, else `remote.<name>.prune`, else `fetch.prune`), `prune`. A cancel noticed before
    `update_tips` now writes no ref at all (before, the `update_tips` callback stopped it after
    the first one).
  - Push (`push_refs`): `writing()` right after `connected()`, before `Remote::push`. The push
    sends, runs `pre-push` and writes tracking refs in that one call, and it may land on the
    remote however it ends, so it is never given up on once it got there (#185's rule; the
    callbacks still see a cancel up to `push_negotiation`).

### repo-service: `transfer_thread`

`Phase` gets `Writing`: `Connecting(since)` → `Transferring` (`connected`) → `Writing`
(`writing`), each step under the one lock and refused once `Abandoned`. The worker's `run` loop
remembers when it first saw the user's cancel (`cancelled_at`), and `give_up(phase,
cancelled_at, connect_limit)` decides:

- `Connecting`: a cancel gives up at once; a `connect_limit` that ran out gives up as
  `Unreachable` (unchanged);
- `Transferring`: a cancel that the transfer hasn't answered within `STALLED_CANCEL_GRACE`
  (2 s) gives up, as `Cancelled`. A transfer that still receives answers a cancel at its next
  progress callback, far sooner, and ends by itself, so only a stalled one is abandoned. A
  short grace keeps a background fetch the user's action cancels from leaving a helper behind
  whenever data was flowing;
- `Writing` and `Abandoned`: never.

The abandoned helper goes on until its socket read fails (the server timeout, or the OS), and
its cancel check is `true`, so a callback stops it if data comes back; either way its
`writing()` answers `false`, so it writes no ref, no `FETCH_HEAD`, no configuration. It may
have renamed a pack into place, which is harmless (above; the read lane already relies on it,
#734). The background fetch's `Memory::connecting` guard lives as long as the helper, so a
lingering one still makes later ticks skip that remote.

Users: Fetch, Fetch all, the background fetch and Pull's fetch can now be given up on while
transferring; pushes (all variants) only while connecting, as before. Commands, policy rows,
wire and frontend are unchanged; the toolbar already shows Cancel for the whole transfer.

## Limits

- A push that stalls after it connected still holds the worker until the server timeout, and
  over HTTPS on Windows (WinHTTP) until the OS gives up on the connection, which may be never.
- A fetch over HTTPS on Windows that stalls and that nobody cancels likewise waits until the OS
  gives up.
- An abandoned helper keeps its socket and `Repository` until it ends: at most the server
  timeout, except over WinHTTP.

## Tests

- git-core `tests/remote.rs`: a fetch and a push ask `writing()` once, after `connected()`; a
  reporter answering `false` stops a fetch with `Cancelled`, no tracking ref and no
  `FETCH_HEAD` written, and a push with nothing sent and nothing written. Characterization tests
  that pin what the split must keep: a fetch follows tags and writes `FETCH_HEAD`, and
  `remote.<name>.prune` wins over `fetch.prune`.
- git-core `tests/server_timeout.rs` (own binary, the option is process-wide):
  `configure_libgit2` sets a 120 s server timeout; and, with a short timeout, a fetch from a
  local `git://` server that answers the connect and then stalls fails instead of hanging
  (characterizes libgit2's socket stream honouring the option).
- repo-service worker `tests/remote.rs`, with that stalling `git://` server: Fetch, Fetch all and
  Pull stalled after connecting are cancelled within a few seconds, the worker answers a status
  read, `Completed` (and Pull's reply) is `Cancelled`, and nothing is written. Bounded waits, so
  the red run fails instead of hanging.
- `transfer_thread::tests`: the phase handshake with `Writing`, and when the worker gives up.

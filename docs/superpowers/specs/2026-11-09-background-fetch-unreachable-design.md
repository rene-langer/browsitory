# Background fetch to an unreachable host (#462): design

## Problem

"Fetch automatically" (#440) runs a quiet Fetch all on the repository's worker. Starting a user
action cancels it, but a cancel only reaches a transfer through libgit2's progress callbacks
(`CancelRegistry`, polled from `transfer_progress`/`sideband_progress`/`update_tips`). A fetch
that is still *connecting* (TCP connect to a host that drops packets, DNS that hangs, a captive
portal or server that accepts and never answers, the TLS/SSH handshake, the ref advertisement)
calls none of them. The worker stays blocked until the OS gives up (60–120 s for a connect, never
for a silent server that accepted), and every command queued behind it waits: the user's action,
and the reads too.

## Options considered

1. **Shorter libgit2 timeouts** (`git2::opts::set_server_connect_timeout_in_milliseconds`,
   `set_server_timeout_in_milliseconds`). Process-wide: they would shorten manual fetch, pull
   and push as well, and the setting races between threads. Rejected.
2. **Reachability probe before the fetch** (resolve the host, `TcpStream::connect_timeout`).
   Doesn't follow libgit2's own resolution (proxies, SSH host aliases), the resolve itself has
   no timeout, and a host that accepts and then stays silent passes the probe and hangs the
   fetch anyway. Rejected as the fix (it narrows the window, doesn't close it).
3. **Run the whole background fetch off the worker.** The worker is never blocked, but the fetch
   then writes references concurrently with the user's mutations (push updating a tracking ref,
   pull, remote rename), which #440 deliberately ruled out; lock contention would surface as
   spurious failures of the user's action. Rejected.
4. **Fetch on a helper thread, worker waits but may abandon it while it connects** (chosen).

## Design

`git-core::remote::fetch_remote_watched` splits each remote's fetch at the only point that
matters: it connects first (`Remote::connect_auth`), then asks a `ConnectWatch` whether to go
on, then fetches over the same connection (`git_remote_fetch` reuses a connected remote:
libgit2's `connect_or_reset_options`), so the fetch itself is unchanged. Nothing is downloaded
or written before the watch answers. A connect that fails at the network level (libgit2's socket
stream: "failed to connect to", "failed to resolve address for") is `RemoteError::Unreachable`,
reported as `TransferFailed` (no wire change). Manual fetches go through the same split, without
a watch.

`repo-service/src/worker/background_fetch.rs` runs the background tick on the worker as before
(same command, same policy row, same `allocate_if_idle` gate, so it still never overlaps another
operation), but fetches each remote on a helper thread with its own `Repository` and the shared
`Arc<CredentialService>` (the no-prompt provider). The worker polls the helper (25 ms):

- A `Phase` (`Connecting(since)` → `Transferring`, or `Abandoned`) under one mutex is the
  handshake. The worker can only abandon a `Connecting` fetch; the watch only moves it to
  `Transferring` if it isn't abandoned. So an abandoned helper never writes: when the OS finally
  lets its connect return, the watch answers `false` (or a callback sees the abandoned phase).
- While connecting, a user cancel abandons the helper and the whole tick ends `Cancelled`; the
  worker is free within one poll. A connect longer than `BACKGROUND_CONNECT_LIMIT` (30 s)
  abandons that remote only, as a quiet `RemoteFailed`, and the tick goes on to the next remote.
- Once transferring, the worker waits as for any fetch; the existing callback cancel applies
  (data is flowing, so callbacks run).
- Helpers report progress through a channel to the worker, which forwards it; they never hold
  the caller's event `Sender`, so an abandoned helper doesn't keep the Tauri/sidecar event relay
  (or a test's `event_rx.iter()`) open.

### Backoff

Per worker, `Memory` remembers per remote: failures to reach it in a row and when to try again.
After the n-th failure (unreachable or connect limit) the remote is skipped until
`interval × 2^n` (capped at an hour, or at the interval when that is longer) has about passed:
with ticks every interval, it is tried every 2, 4, 8 … intervals, at most hourly. The frontend
passes the interval with `backgroundFetch(repoPath, prune, intervalMinutes)` (the only wire
change). Any fetch of that remote that connects, background or manual (Fetch, Fetch all), ends
its backoff. A remote whose abandoned helper is still connecting is skipped as well, so at most
one helper per remote lingers; a skipped remote is a quiet `RemoteFailed` (`TransferFailed`),
and a tick where every remote was skipped doesn't count as "Fetched just now".

Backoff lives in the backend rather than in `useAutoFetch`: it is per remote (one dead remote
must not slow fetching the others), and only the backend knows a failure was a connect failure.

## Limits

- An abandoned helper thread stays blocked until the OS gives up on its socket; for a server
  that accepts and never answers that may be the end of the session. It holds a socket and an
  open `Repository`, and blocks only later background fetches of that one remote.
- A transfer that stalls after connecting (the network drops mid-download) is not abandoned:
  libgit2 may be writing the pack then. That case is no worse than before and also affects
  manual fetches.
- Manual fetch, pull and push keep libgit2's defaults; a cancel while they connect still waits
  for the connect.

## Tests

- `git-core` (`tests/remote.rs`): a watched fetch tells the watch once and fetches; a watch
  answering `false` leaves the tracking ref untouched and reports nothing; a refused connection
  (a port bound and closed again) is `Unreachable`; a missing local path stays `Git`.
- `repo-service` worker (`tests/remote.rs`), with a local TCP listener that accepts and never
  answers: a cancel while the background fetch is connecting frees the worker at once (a status
  read answers in well under the limit), the tick ends `Cancelled`, nothing is written; with a
  short connect limit the silent remote fails quietly while the other remote is fetched, and the
  next tick skips it without connecting again.
- `background_fetch::tests`: the backoff schedule (5-minute ticks → tries at 0, 10, 30, 70, 130
  minutes), the hour cap, reset on success, skip while a helper lingers.
- Frontend: `useAutoFetch` passes the interval; both clients send `intervalMinutes`.

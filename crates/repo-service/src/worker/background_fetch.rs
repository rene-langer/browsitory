//! What the "Fetch automatically" tick (#440) needs besides a Fetch all: its options, and the
//! backoff of remotes it could not reach (#462).
//!
//! The tick is a Fetch all (`fetch_all`), reported as `BackgroundFetch`, whose remotes each
//! connect on a helper thread the worker gives up on after `BACKGROUND_CONNECT_LIMIT` (see
//! `transfer_thread`). A remote that could not be reached is skipped by later background fetches
//! for a while: the interval doubles with each failure in a row, up to an hour (`Memory`). A
//! remote whose abandoned helper is still connecting is skipped too, so at most one helper per
//! remote lingers.
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// How long a background fetch may take to connect to one remote (reach the host and receive
/// its references) before the worker gives up on that remote for this tick. Manual fetches have
/// no such limit: libgit2's own timeouts are process-wide, so they are left alone.
pub(super) const BACKGROUND_CONNECT_LIMIT: Duration = Duration::from_secs(30);

/// The longest a remote that could not be reached waits for its next background fetch, unless
/// the interval itself is longer.
const MAX_BACKOFF: Duration = Duration::from_secs(60 * 60);

/// What one background fetch is asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BackgroundFetchOptions {
    pub(crate) prune: Option<bool>,
    /// The "Fetch automatically" interval, which a backoff doubles.
    pub(crate) interval: Duration,
    /// `BACKGROUND_CONNECT_LIMIT`, shorter in tests.
    pub(crate) connect_limit: Duration,
}

impl BackgroundFetchOptions {
    pub(crate) fn new(prune: Option<bool>, interval_minutes: u32) -> Self {
        Self {
            prune,
            interval: Duration::from_secs(u64::from(interval_minutes.max(1)) * 60),
            connect_limit: BACKGROUND_CONNECT_LIMIT,
        }
    }
}

/// What the background fetches of one worker remember between ticks: remotes backing off after
/// they could not be reached, and remotes whose abandoned helper is still connecting. Shared
/// with the helpers, which leave `connecting` when they end.
#[derive(Clone, Default)]
pub(crate) struct Memory(Arc<Mutex<MemoryState>>);

#[derive(Default)]
struct MemoryState {
    connecting: HashSet<String>,
    backoff: HashMap<String, Backoff>,
}

#[derive(Debug, Clone, Copy)]
struct Backoff {
    /// Failures to reach the remote in a row.
    failures: u32,
    /// Background fetches starting before this skip the remote.
    retry_at: Instant,
}

impl Memory {
    fn state(&self) -> MutexGuard<'_, MemoryState> {
        self.0.lock().unwrap_or_else(|error| error.into_inner())
    }

    /// Whether the background fetch starting at `tick` tries `remote_name`.
    pub(super) fn may_try(&self, remote_name: &str, tick: Instant) -> bool {
        let state = self.state();
        !state.connecting.contains(remote_name)
            && state
                .backoff
                .get(remote_name)
                .is_none_or(|backoff| tick >= backoff.retry_at)
    }

    /// `remote_name` could not be reached by the background fetch that started at `tick`: skip it
    /// until the doubled interval is (about) over. Ticks come every `interval` after the previous
    /// one started, so the next try is due half an interval early, to land on a tick.
    pub(super) fn unreachable(&self, remote_name: &str, tick: Instant, interval: Duration) {
        let mut state = self.state();
        let failures = state
            .backoff
            .get(remote_name)
            .map_or(0, |backoff| backoff.failures)
            .saturating_add(1);
        let delay = interval
            .saturating_mul(2u32.saturating_pow(failures))
            .min(MAX_BACKOFF.max(interval));
        state.backoff.insert(
            remote_name.to_string(),
            Backoff {
                failures,
                retry_at: tick + delay.saturating_sub(interval / 2),
            },
        );
    }

    /// `remote_name` answered (a fetch of it connected, background or not): no more backoff.
    pub(super) fn reachable(&self, remote_name: &str) {
        self.state().backoff.remove(remote_name);
    }

    /// Marks `remote_name` as having a helper; the returned guard unmarks it when dropped.
    pub(super) fn connecting(&self, remote_name: &str) -> ConnectingGuard {
        self.state().connecting.insert(remote_name.to_string());
        ConnectingGuard {
            memory: self.clone(),
            remote_name: remote_name.to_string(),
        }
    }
}

/// Held by a helper thread for as long as it lives (see `Memory::connecting`).
pub(super) struct ConnectingGuard {
    memory: Memory,
    remote_name: String,
}

impl Drop for ConnectingGuard {
    fn drop(&mut self) {
        self.memory.state().connecting.remove(&self.remote_name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: Duration = Duration::from_secs(60);

    #[test]
    fn an_unreachable_remote_backs_off_doubling_the_interval_up_to_an_hour() {
        let memory = Memory::default();
        let start = Instant::now();
        let interval = 5 * MINUTE;
        // Ticks every five minutes; the remote is never reachable.
        let mut tried = Vec::new();
        for minute in (0..=180).step_by(5) {
            let tick = start + minute * MINUTE;
            if memory.may_try("origin", tick) {
                tried.push(minute);
                memory.unreachable("origin", tick, interval);
            }
        }
        assert_eq!(tried, [0, 10, 30, 70, 130]);
    }

    #[test]
    fn a_tick_that_comes_a_little_early_still_tries_again() {
        let memory = Memory::default();
        let start = Instant::now() + 10 * MINUTE;
        let interval = 5 * MINUTE;
        memory.unreachable("origin", start, interval);
        // The tick after next, a second earlier than two intervals.
        let early = start + 2 * interval - Duration::from_secs(1);
        assert!(!memory.may_try("origin", start + interval));
        assert!(memory.may_try("origin", early));
    }

    #[test]
    fn an_interval_of_an_hour_or_more_is_not_lengthened() {
        let memory = Memory::default();
        let start = Instant::now();
        memory.unreachable("origin", start, 90 * MINUTE);
        assert!(!memory.may_try("origin", start + 30 * MINUTE));
        assert!(memory.may_try("origin", start + 90 * MINUTE));
    }

    #[test]
    fn a_reachable_remote_is_tried_again_at_once() {
        let memory = Memory::default();
        let start = Instant::now();
        memory.unreachable("origin", start, 5 * MINUTE);
        assert!(!memory.may_try("origin", start + 5 * MINUTE));
        assert!(memory.may_try("upstream", start + 5 * MINUTE));
        memory.reachable("origin");
        assert!(memory.may_try("origin", start + 5 * MINUTE));
    }

    #[test]
    fn a_remote_with_a_helper_still_connecting_is_skipped() {
        let memory = Memory::default();
        let guard = memory.connecting("origin");
        assert!(!memory.may_try("origin", Instant::now()));
        drop(guard);
        assert!(memory.may_try("origin", Instant::now()));
    }
}

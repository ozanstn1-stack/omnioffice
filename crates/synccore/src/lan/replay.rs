//! A bounded replay window for signed LAN requests (E1/3).
//!
//! Every request carries a timestamp and a fresh nonce inside the MAC, so a
//! recorded request stays valid for `REQUEST_FRESHNESS_SECS`. Without a cache,
//! anyone who captures one - including a passive observer of an unencrypted
//! hop, or a peer replaying its own earlier upload - can re-send it inside that
//! window and have it accepted again. The guard remembers the nonces it has
//! accepted and refuses a second use.
//!
//! The window is pruned by age and capped, so a peer that sends requests as
//! fast as it can cannot grow it without bound. When the cap is reached the
//! oldest entry is dropped: that can only let an already-expired request be
//! replayed if it were still inside the freshness window, which the cap is
//! sized to make impractical for one pairing.

use std::collections::HashMap;
use std::sync::Mutex;

/// Nonces one host remembers. One transfer is a handful of requests (a chunked
/// upload is one request per 512 KiB), so this covers many peers inside the
/// freshness window.
pub(super) const REPLAY_CAPACITY: usize = 8192;

#[derive(Debug)]
pub(super) struct ReplayGuard {
    seen: Mutex<HashMap<String, u64>>,
    window: u64,
    capacity: usize,
}

impl ReplayGuard {
    pub(super) fn new(window: u64, capacity: usize) -> Self {
        Self { seen: Mutex::new(HashMap::new()), window, capacity }
    }

    /// Records `(device_id, nonce)`; `false` when this exact request was
    /// already accepted inside the window.
    pub(super) fn accept(&self, device_id: &str, nonce: &str, now: u64) -> bool {
        if nonce.is_empty() {
            return false;
        }
        let Ok(mut seen) = self.seen.lock() else { return false };
        seen.retain(|_, accepted| now.saturating_sub(*accepted) <= self.window);
        let key = format!("{device_id}\n{nonce}");
        if seen.contains_key(&key) {
            return false;
        }
        if seen.len() >= self.capacity {
            // Drop the oldest entry instead of refusing: a full cache must not
            // become a way to lock a legitimate peer out.
            if let Some(oldest) = seen.iter().min_by_key(|(_, accepted)| **accepted).map(|(key, _)| key.clone()) {
                seen.remove(&oldest);
            }
        }
        seen.insert(key, now);
        true
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.seen.lock().map(|seen| seen.len()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nonce_is_accepted_once_and_forgotten_after_the_window() {
        let guard = ReplayGuard::new(300, 16);
        assert!(guard.accept("device-a", "aa", 1_000));
        assert!(!guard.accept("device-a", "aa", 1_000), "the same request is refused");
        assert!(!guard.accept("device-a", "aa", 1_100), "and stays refused inside the window");
        assert!(guard.accept("device-b", "aa", 1_000), "another device has its own nonce space");
        assert!(guard.accept("device-a", "bb", 1_400), "a fresh nonce is accepted");
        assert_eq!(guard.len(), 1, "the entries outside the window were pruned: {}", guard.len());
        assert!(guard.accept("device-a", "aa", 1_400), "the old nonce left the window");
    }

    #[test]
    fn an_empty_nonce_is_refused_and_the_cache_stays_bounded() {
        let guard = ReplayGuard::new(300, 2);
        assert!(!guard.accept("device", "", 10), "an unsigned nonce is never accepted");
        assert!(guard.accept("device", "one", 10));
        assert!(guard.accept("device", "two", 10));
        assert!(guard.accept("device", "three", 10), "the oldest entry makes room");
        assert_eq!(guard.len(), 2, "the cap is kept");
        assert!(!guard.accept("device", "three", 11), "the newest nonce is still remembered");
    }
}

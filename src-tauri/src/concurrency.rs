//! Process-wide bound on CPU/IO-heavy blocking work.
//!
//! Heavy commands (PDF render/thumbnails, OCR, raster compression, vault
//! scan, office import/export, signing) run on Tauri's shared blocking pool,
//! which is capped only by Tokio's 512-thread default. A handful of concurrent
//! OCR/compression jobs could therefore spawn hundreds of threads and
//! tesseract child processes, multiplying peak RAM and CPU far beyond what the
//! machine can take. Every heavy path takes a slot here first, so peak
//! concurrency stays near the CPU count.
//!
//! The semaphore is global and never closed; a permit is released when the
//! command returns (or is cancelled and its future is dropped).

use std::sync::OnceLock;
use std::sync::{Condvar, Mutex};
use tokio::sync::{Semaphore, SemaphorePermit};

fn bound() -> usize {
    let cores = std::thread::available_parallelism().map(|count| count.get()).unwrap_or(4);
    // Enough parallelism to keep a modern machine busy, but never the
    // 512-thread free-for-all Tokio allows by default.
    cores.clamp(2, 8)
}

fn semaphore() -> &'static Semaphore {
    static HEAVY: OnceLock<Semaphore> = OnceLock::new();
    HEAVY.get_or_init(|| Semaphore::new(bound()))
}

/// A slot in the heavy-work bound, released on drop.
pub struct HeavyPermit(#[allow(dead_code)] SemaphorePermit<'static>);

/// Waits until a heavy-work slot is free.
pub async fn acquire() -> HeavyPermit {
    HeavyPermit(semaphore().acquire().await.expect("the heavy-work semaphore is never closed"))
}

fn blocking_semaphore() -> &'static (Mutex<usize>, Condvar) {
    static LIMIT: OnceLock<(Mutex<usize>, Condvar)> = OnceLock::new();
    LIMIT.get_or_init(|| (Mutex::new(bound()), Condvar::new()))
}

/// A slot in the heavy-work bound for synchronous code that cannot await.
pub struct BlockingPermit;

/// Blocks until a heavy-work slot is free. Synchronous extraction paths (AI
/// page text) take one so they share the same order of magnitude of bound as
/// the async heavy work instead of running unbounded on the async threads.
pub fn acquire_blocking() -> BlockingPermit {
    let (available, ready) = blocking_semaphore();
    let mut slots = available.lock().expect("the heavy-work lock is never poisoned");
    while *slots == 0 {
        slots = ready.wait(slots).expect("the heavy-work lock is never poisoned");
    }
    *slots -= 1;
    BlockingPermit
}

impl Drop for BlockingPermit {
    fn drop(&mut self) {
        let (available, ready) = blocking_semaphore();
        if let Ok(mut slots) = available.lock() {
            *slots += 1;
            ready.notify_one();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bound_is_a_small_constant_not_the_tokio_default() {
        // The isolation promise: heavy work never runs at Tokio's 512-thread
        // default, and an 8-core machine gets at most 8 slots. A regression to
        // an unbounded semaphore would multiply peak RAM and child processes.
        let bound = bound();
        assert!((2..=8).contains(&bound), "heavy-work bound {bound} is outside 2..=8");
        assert_eq!(
            semaphore().available_permits(),
            bound,
            "the async semaphore must expose exactly the configured bound"
        );
    }

    #[test]
    fn a_panicking_worker_returns_its_blocking_slot() {
        // A panic must not leak a slot: the extraction paths catch panics in
        // the parser, and a leaked permit would starve the pool after a few
        // malformed files.
        let released = std::panic::catch_unwind(|| {
            let _permit = acquire_blocking();
            panic!("worker panic");
        });
        assert!(released.is_err(), "the panic must propagate through catch_unwind");
        // The slot is usable again immediately.
        let permit = acquire_blocking();
        drop(permit);
    }

    #[test]
    fn blocking_permits_are_released_on_drop() {
        // Taking the whole bound and releasing it must leave the bound usable
        // again (a leak here would eventually starve every extraction).
        let limit = bound();
        let taken: Vec<BlockingPermit> = (0..limit).map(|_| acquire_blocking()).collect();
        assert_eq!(taken.len(), limit);
        drop(taken);
        let again: Vec<BlockingPermit> = (0..limit).map(|_| acquire_blocking()).collect();
        assert_eq!(again.len(), limit);
    }
}

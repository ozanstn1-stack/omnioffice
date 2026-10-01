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
use tokio::sync::{Semaphore, SemaphorePermit};

fn semaphore() -> &'static Semaphore {
    static HEAVY: OnceLock<Semaphore> = OnceLock::new();
    HEAVY.get_or_init(|| {
        let cores = std::thread::available_parallelism().map(|count| count.get()).unwrap_or(4);
        // Enough parallelism to keep a modern machine busy, but never the
        // 512-thread free-for-all Tokio allows by default.
        Semaphore::new(cores.clamp(2, 8))
    })
}

/// A slot in the heavy-work bound, released on drop.
pub struct HeavyPermit(#[allow(dead_code)] SemaphorePermit<'static>);

/// Waits until a heavy-work slot is free.
pub async fn acquire() -> HeavyPermit {
    HeavyPermit(semaphore().acquire().await.expect("the heavy-work semaphore is never closed"))
}

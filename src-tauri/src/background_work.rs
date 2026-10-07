//! Start gate for the Android background-work service.
//!
//! The job registry reports every change of "is any job running" (see
//! `JobRegistry::set_activity_listener`). A foreground service for each of
//! them would be wasteful - in-app search registers a job per query and ends
//! within milliseconds - and a start/stop pair per keystroke buys nothing, so
//! the service is only started once work has been running for `delay`. A stop
//! always takes effect at once and cancels a start that is still waiting.
//!
//! Platform independent on purpose: Android is the only caller (see
//! `android_background.rs`), but the timing logic is covered by unit tests on
//! every host.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

struct GateState {
    /// Bumped on every request; a delayed start only fires if it is still the latest.
    generation: u64,
    /// Whether the last `sink(true)` has not been followed by `sink(false)`.
    running: bool,
}

type Sink = Arc<dyn Fn(bool) + Send + Sync>;

/// Debounces "work started" / "work ended" requests into service start/stop calls.
pub struct StartGate {
    delay: Duration,
    sink: Sink,
    state: Arc<Mutex<GateState>>,
}

fn lock(state: &Mutex<GateState>) -> MutexGuard<'_, GateState> {
    // The state is two plain values; a panic elsewhere cannot leave it torn.
    state.lock().unwrap_or_else(|poison| poison.into_inner())
}

impl StartGate {
    /// `sink(true)` starts the service, `sink(false)` stops it. Calls are
    /// serialized, so a stop can never overtake the start it follows.
    pub fn new(delay: Duration, sink: impl Fn(bool) + Send + Sync + 'static) -> Self {
        Self { delay, sink: Arc::new(sink), state: Arc::new(Mutex::new(GateState { generation: 0, running: false })) }
    }

    /// Reports that work is running (`true`) or that nothing is running anymore.
    pub fn set(&self, active: bool) {
        let mut state = lock(&self.state);
        state.generation += 1;
        if !active {
            if state.running {
                state.running = false;
                (self.sink)(false);
            }
            return;
        }
        let generation = state.generation;
        drop(state);
        let shared = self.state.clone();
        let sink = self.sink.clone();
        let delay = self.delay;
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            let mut state = lock(&shared);
            if state.generation == generation && !state.running {
                state.running = true;
                sink(true);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const DELAY: Duration = Duration::from_millis(60);
    const SETTLE: Duration = Duration::from_millis(220);

    /// A gate whose sink records every call, in order.
    fn recording_gate() -> (StartGate, Arc<Mutex<Vec<bool>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let sink_calls = calls.clone();
        let gate = StartGate::new(DELAY, move |active| sink_calls.lock().unwrap().push(active));
        (gate, calls)
    }

    fn calls_of(calls: &Arc<Mutex<Vec<bool>>>) -> Vec<bool> {
        calls.lock().unwrap().clone()
    }

    #[test]
    fn work_that_outlasts_the_delay_starts_and_then_stops_the_service() {
        let (gate, calls) = recording_gate();
        gate.set(true);
        assert!(calls_of(&calls).is_empty(), "the start waits for the delay");
        std::thread::sleep(SETTLE);
        assert_eq!(calls_of(&calls), vec![true]);
        gate.set(false);
        assert_eq!(calls_of(&calls), vec![true, false], "a stop is immediate");
    }

    #[test]
    fn work_that_ends_within_the_delay_never_touches_the_service() {
        let (gate, calls) = recording_gate();
        gate.set(true);
        gate.set(false);
        std::thread::sleep(SETTLE);
        assert!(calls_of(&calls).is_empty());
    }

    #[test]
    fn a_stop_without_a_running_service_is_ignored() {
        let (gate, calls) = recording_gate();
        gate.set(false);
        gate.set(false);
        assert!(calls_of(&calls).is_empty());
    }

    #[test]
    fn a_restart_during_the_delay_starts_once() {
        let (gate, calls) = recording_gate();
        // true -> false -> true: the first pending start is cancelled, the
        // second one fires.
        gate.set(true);
        gate.set(false);
        gate.set(true);
        std::thread::sleep(SETTLE);
        assert_eq!(calls_of(&calls), vec![true]);
    }

    #[test]
    fn repeated_work_cycles_alternate_start_and_stop() {
        let (gate, calls) = recording_gate();
        for _ in 0..3 {
            gate.set(true);
            std::thread::sleep(SETTLE);
            gate.set(false);
        }
        assert_eq!(calls_of(&calls), vec![true, false, true, false, true, false]);
    }

    #[test]
    fn a_duplicate_start_request_does_not_start_twice() {
        let starts = Arc::new(AtomicUsize::new(0));
        let counter = starts.clone();
        let gate = StartGate::new(DELAY, move |active| {
            if active {
                counter.fetch_add(1, Ordering::SeqCst);
            }
        });
        gate.set(true);
        std::thread::sleep(SETTLE);
        gate.set(true);
        std::thread::sleep(SETTLE);
        assert_eq!(starts.load(Ordering::SeqCst), 1);
    }
}

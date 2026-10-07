//! Android foreground-service bridge for long jobs (Android builds only).
//!
//! Without it Android freezes or kills the process of an app that is no longer
//! visible, which ends a running OCR, compression or conversion mid-way. While
//! at least one job of the [`JobRegistry`](crate::jobs::JobRegistry) runs, a
//! `dataSync` foreground service (`BackgroundWorkService.kt`) keeps the process
//! alive and shows an ongoing notification.
//!
//! Bridge choice: the same Tauri mobile plugin pattern as the Keystore
//! (`android_keystore.rs`): a Kotlin `@TauriPlugin` in the app module, registered
//! with `register_android_plugin` and called with `run_mobile_plugin`. The
//! command is tiny - `setBackgroundWork { active }` - and idempotent on the
//! Kotlin side; the registry only reports transitions of "any job running", and
//! [`StartGate`] holds a start back for [`START_DELAY`] so a job that ends
//! within moments never causes a service start.
//!
//! `run_mobile_plugin` blocks until the Kotlin side answers, and Tauri runs the
//! Kotlin command on the Android UI thread. A caller on that thread (the window
//! event handler cancelling jobs) would wait for itself, so the calls are never
//! made by the reporting thread: they are handed to one worker thread, which
//! also keeps start and stop in order.
//!
//! Failures are logged and swallowed: the jobs run exactly as before, they are
//! just not protected from the system.

use crate::background_work::StartGate;
use serde::Serialize;
use std::sync::{mpsc, OnceLock};
use std::time::Duration;
use tauri::plugin::{PluginHandle, TauriPlugin};
use tauri::Wry;

const PLUGIN_NAME: &str = "omnioffice-background";
const KOTLIN_PACKAGE: &str = "io.github.ozanstn1.pdfswissarmyknife";
const KOTLIN_CLASS: &str = "BackgroundWorkPlugin";

/// How long a job has to run before it earns a foreground service.
const START_DELAY: Duration = Duration::from_millis(1500);

static HANDLE: OnceLock<PluginHandle<Wry>> = OnceLock::new();
static GATE: OnceLock<StartGate> = OnceLock::new();
static WORKER: OnceLock<mpsc::Sender<bool>> = OnceLock::new();

#[derive(Serialize)]
struct Payload {
    active: bool,
}

/// The plugin to register on the app builder. A failed registration is logged
/// and swallowed: the app must still start without background protection.
pub fn init() -> TauriPlugin<Wry> {
    tauri::plugin::Builder::<Wry>::new(PLUGIN_NAME)
        .setup(|_app, api| {
            match api.register_android_plugin(KOTLIN_PACKAGE, KOTLIN_CLASS) {
                Ok(handle) => {
                    let _ = HANDLE.set(handle);
                }
                Err(error) => eprintln!("could not register the Android background-work plugin: {error}"),
            }
            Ok(())
        })
        .build()
}

fn call(active: bool) {
    let Some(handle) = HANDLE.get() else { return };
    if let Err(error) = handle.run_mobile_plugin::<()>("setBackgroundWork", Payload { active }) {
        eprintln!("background work service could not be {}: {error}", if active { "started" } else { "stopped" });
    }
}

/// Queues a service start/stop for the worker thread (started on first use).
fn enqueue(active: bool) {
    let worker = WORKER.get_or_init(|| {
        let (sender, receiver) = mpsc::channel::<bool>();
        std::thread::spawn(move || {
            for active in receiver {
                call(active);
            }
        });
        sender
    });
    let _ = worker.send(active);
}

/// Registry listener: `true` when the first job starts, `false` when the last
/// one ends.
pub fn set_active(active: bool) {
    GATE.get_or_init(|| StartGate::new(START_DELAY, enqueue)).set(active);
}

//! Android foreground-service bridge for LAN sharing.
//!
//! A shared folder stays reachable only while the process lives; once the app
//! goes to the background Android may freeze or kill it, and mDNS responses
//! are multicast packets that Wi-Fi power saving drops. The Kotlin plugin
//! (`LanSharePlugin.kt`) starts a `dataSync` foreground service
//! (`LanShareService.kt`) with an ongoing notification and holds a
//! `WifiManager.MulticastLock`; this module is the Rust side of that bridge.
//!
//! Same pattern as the Keystore and background-work bridges: a Tauri mobile
//! plugin registered with `register_android_plugin`, called with
//! `run_mobile_plugin` from a dedicated worker thread (the call blocks until
//! Kotlin answers). Failures are logged and swallowed: the host keeps running
//! for as long as the process does.

#[cfg(target_os = "android")]
use serde::Serialize;
#[cfg(target_os = "android")]
use std::sync::{mpsc, OnceLock};
#[cfg(target_os = "android")]
use tauri::plugin::{PluginHandle, TauriPlugin};
#[cfg(target_os = "android")]
use tauri::Wry;

#[cfg(target_os = "android")]
const PLUGIN_NAME: &str = "omnioffice-lan-share";
#[cfg(target_os = "android")]
const KOTLIN_PACKAGE: &str = "io.github.ozanstn1.pdfswissarmyknife";
#[cfg(target_os = "android")]
const KOTLIN_CLASS: &str = "LanSharePlugin";

#[cfg(target_os = "android")]
static HANDLE: OnceLock<PluginHandle<Wry>> = OnceLock::new();
#[cfg(target_os = "android")]
static WORKER: OnceLock<mpsc::Sender<bool>> = OnceLock::new();

#[cfg(target_os = "android")]
#[derive(Serialize)]
struct Payload {
    active: bool,
}

/// The plugin to register on the app builder.
#[cfg(target_os = "android")]
pub fn init() -> TauriPlugin<Wry> {
    tauri::plugin::Builder::<Wry>::new(PLUGIN_NAME)
        .setup(|_app, api| {
            match api.register_android_plugin(KOTLIN_PACKAGE, KOTLIN_CLASS) {
                Ok(handle) => {
                    let _ = HANDLE.set(handle);
                }
                Err(error) => eprintln!("could not register the Android LAN-share plugin: {error}"),
            }
            Ok(())
        })
        .build()
}

#[cfg(target_os = "android")]
fn call(active: bool) {
    let Some(handle) = HANDLE.get() else { return };
    if let Err(error) = handle.run_mobile_plugin::<()>("setLanSharing", Payload { active }) {
        eprintln!("LAN share service could not be {}: {error}", if active { "started" } else { "stopped" });
    }
}

/// Starts or stops the foreground service and the multicast lock.
#[cfg(target_os = "android")]
pub fn set_active(active: bool) {
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

/// Desktop and other platforms have no foreground service; sharing simply ends
/// with the process.
#[cfg(not(target_os = "android"))]
pub fn set_active(_active: bool) {}

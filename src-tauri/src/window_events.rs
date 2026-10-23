//! Window event wiring: running-job cancellation on close, and capability
//! tokens for dropped files. The drop handler lives here so `lib.rs` stays a
//! thin wiring file and the paths never reach the app code.

use tauri::{Manager, Runtime, Window, WindowEvent};

pub fn handle<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    match event {
        WindowEvent::CloseRequested { .. } => {
            if let Some(registry) = window.app_handle().try_state::<crate::jobs::JobRegistry>() {
                registry.cancel_all();
            }
        }
        WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) => {
            crate::files::emit_drop_tokens(window, paths);
        }
        _ => {}
    }
}

use crate::error::{PdfError, PdfResult};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Progress payload sent to the UI for long running operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressEvent {
    pub stage: String,
    pub current: u64,
    pub total: u64,
    pub message: Option<String>,
}

impl ProgressEvent {
    pub fn new(stage: &str, current: u64, total: u64) -> Self {
        Self { stage: stage.to_string(), current, total, message: None }
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }
}

/// Cooperative cancellation flag shared between the UI and a running job.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    /// Returns an error when the job has been cancelled.
    pub fn check(&self) -> PdfResult<()> {
        if self.is_cancelled() {
            Err(PdfError::Cancelled)
        } else {
            Ok(())
        }
    }
}

pub type ProgressCallback<'a> = dyn Fn(ProgressEvent) + Send + Sync + 'a;

/// A zero-sized progress sink for operations that do not need to report.
pub struct NoProgress;

impl NoProgress {
    pub fn call(&self, _event: ProgressEvent) {}
}

/// Helper that forwards progress to the caller's callback.
pub struct ProgressReporter<'a> {
    callback: &'a ProgressCallback<'a>,
}

impl<'a> ProgressReporter<'a> {
    pub fn new(callback: &'a ProgressCallback<'a>) -> Self {
        Self { callback }
    }

    pub fn emit(&self, event: ProgressEvent) {
        (self.callback)(event)
    }

    pub fn emit_step(&self, stage: &str, current: u64, total: u64) {
        self.emit(ProgressEvent::new(stage, current, total))
    }
}

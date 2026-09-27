//! Job registry and persistent job store.
//!
//! Two responsibilities live here:
//!
//! 1. Cancellation tokens (`JobRegistry`). These are process-local by
//!    definition: a token only means something while the worker thread that
//!    checks it is alive.
//! 2. Job records (`JobStore`). These are persisted to
//!    `<app config dir>/jobs.json` so the Jobs screen still knows what was
//!    going on after an Android activity recreation or a full app restart.
//!
//! Android honesty: a Rust worker keeps running while the *process* lives.
//! This store makes the state survive process death; it does NOT keep the
//! process alive. If Android kills the process in the background the work
//! stops, and the next start flips every `running`/`queued` record to
//! `interrupted` so the UI never pretends a dead job is still running.
//! Actually continuing work in the background needs a foreground service,
//! which is out of scope for V3.1.
//!
//! Persistence contract:
//! - writes are atomic (temp file + rename over the target),
//! - progress-only updates are throttled to at most one write per job per
//!   [`PERSIST_THROTTLE`] (500 ms) so page-by-page progress does not hammer
//!   the disk,
//! - every status transition (register/finish/cancel/interrupt/clear) writes
//!   immediately.
//!
//! Event contract (shared with `src/lib/jobs.ts`, never change one side alone):
//! - `job:progress`: `{ jobId, stage, current, total, message? }` (camelCase,
//!   emitted from [`emit_progress`]).
//! - Commands: `jobs_list`, `jobs_register`, `jobs_progress`, `jobs_finish`,
//!   `jobs_retry`, `jobs_clear_finished`.

use pdfcore::progress::{CancelToken, ProgressEvent};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};

/// Minimum interval between two progress-only disk writes for the same job.
pub const PERSIST_THROTTLE: Duration = Duration::from_millis(500);

/// Upper bound for the persisted history; the oldest finished records are
/// dropped first so the file stays small (the frontend caps its view at 50).
pub const MAX_RECORDS: usize = 200;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

/// The lifecycle of a persisted job. Serialized lowercase to match the
/// frontend type (`src/lib/jobs.ts` maps `done` to the UI's `succeeded`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
    Interrupted,
}

impl JobStatus {
    /// A terminal status will never change on its own anymore.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            JobStatus::Done | JobStatus::Failed | JobStatus::Cancelled | JobStatus::Interrupted
        )
    }
}

/// One background job as persisted to `<app config dir>/jobs.json`.
///
/// `payload` is the serialized operation input captured at job start. It is
/// never document content, only paths/options the frontend already had; the
/// UI hands it back to the originating screen when Retry is pressed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobRecord {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub status: JobStatus,
    /// Normalized 0..1; stays at the previous value while `total` is unknown.
    pub progress: f64,
    /// Human-readable last stage/message (never document content).
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Default)]
struct StoreInner {
    records: HashMap<String, JobRecord>,
    /// Last throttled progress write per job; status transitions bypass it.
    last_persist_by_job: HashMap<String, Instant>,
}

/// Persistent job history shared by the Tauri commands and the registry.
///
/// The store is managed as `Arc<JobStore>` state: the registry holds the same
/// `Arc`, and [`emit_progress`] reaches it through the app handle so every
/// existing producer keeps its call site.
pub struct JobStore {
    inner: Mutex<StoreInner>,
    path: OnceLock<PathBuf>,
}

impl Default for JobStore {
    fn default() -> Self {
        Self {
            inner: Mutex::new(StoreInner::default()),
            path: OnceLock::new(),
        }
    }
}

impl JobStore {
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, StoreInner> {
        // A poisoned lock would only mean a previous panic while a record was
        // being updated; the map itself is still consistent enough to use.
        self.inner.lock().unwrap_or_else(|poison| poison.into_inner())
    }

    /// Binds the store to its JSON file, loads the history and marks every job
    /// that was still `running`/`queued` as `interrupted`: the previous process
    /// died (activity recreation or app restart), so those jobs are not
    /// running anymore. The normalized history is written back immediately.
    pub fn attach_path(&self, path: PathBuf) {
        let _ = self.path.set(path.clone());
        let loaded: Vec<JobRecord> = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let now = now_ms();
        let mut inner = self.lock();
        for mut record in loaded {
            if matches!(record.status, JobStatus::Queued | JobStatus::Running) {
                record.status = JobStatus::Interrupted;
                record.updated_at = now;
            }
            inner.records.insert(record.id.clone(), record);
        }
        Self::prune_locked(&mut inner.records);
        self.persist_locked(&inner);
    }

    /// All records, newest first.
    pub fn records(&self) -> Vec<JobRecord> {
        let inner = self.lock();
        let mut records: Vec<JobRecord> = inner.records.values().cloned().collect();
        records.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        records
    }

    pub fn record(&self, id: &str) -> Option<JobRecord> {
        self.lock().records.get(id).cloned()
    }

    /// Frontend-driven registration: the producer knows kind, title and the
    /// serialized payload. Re-registering an existing id starts a fresh run
    /// (tool screens reuse their job id when the user presses Run again).
    pub fn upsert(&self, id: &str, kind: &str, title: &str, payload: Option<serde_json::Value>) {
        let now = now_ms();
        let mut inner = self.lock();
        let record = inner.records.entry(id.to_string()).or_insert_with(|| JobRecord {
            id: id.to_string(),
            kind: kind.to_string(),
            title: title.to_string(),
            status: JobStatus::Running,
            progress: 0.0,
            detail: None,
            payload: None,
            error: None,
            created_at: now,
            updated_at: now,
        });
        record.kind = kind.to_string();
        record.title = title.to_string();
        if payload.is_some() {
            record.payload = payload;
        }
        record.status = JobStatus::Running;
        record.progress = 0.0;
        record.detail = None;
        record.error = None;
        record.updated_at = now;
        Self::prune_locked(&mut inner.records);
        self.persist_locked(&inner);
        inner.last_persist_by_job.insert(id.to_string(), Instant::now());
    }

    /// Rust-side fallback registration: guarantees a record exists even when
    /// the frontend did not call `jobs_register` (older producer wiring, a lost
    /// IPC call). Existing kind/title/payload are preserved; a new run of a
    /// reused id (the vault scan always uses `vault-scan`) is reset to running.
    pub fn ensure_tracked(&self, id: &str, kind: &str) {
        let now = now_ms();
        let mut inner = self.lock();
        let record = inner.records.entry(id.to_string()).or_insert_with(|| JobRecord {
            id: id.to_string(),
            kind: kind.to_string(),
            title: id.to_string(),
            status: JobStatus::Running,
            progress: 0.0,
            detail: None,
            payload: None,
            error: None,
            created_at: now,
            updated_at: now,
        });
        if record.status.is_terminal() {
            record.status = JobStatus::Running;
            record.progress = 0.0;
            record.detail = None;
            record.error = None;
            record.updated_at = now;
        }
        Self::prune_locked(&mut inner.records);
        self.persist_locked(&inner);
        inner.last_persist_by_job.insert(id.to_string(), Instant::now());
    }

    /// Progress update from [`emit_progress`] or the `jobs_progress` command.
    /// Unknown ids and terminal records are ignored: progress must not
    /// resurrect finished work. Only the disk write is throttled.
    pub fn set_progress(
        &self,
        id: &str,
        stage: &str,
        current: u64,
        total: u64,
        message: Option<&str>,
    ) {
        let now = now_ms();
        let mut inner = self.lock();
        {
            let Some(record) = inner.records.get_mut(id) else { return };
            if record.status != JobStatus::Running {
                return;
            }
            if total > 0 {
                record.progress = (current as f64 / total as f64).clamp(0.0, 1.0);
            }
            let detail = match message {
                Some(text) if !text.trim().is_empty() => Some(text.to_string()),
                _ if !stage.trim().is_empty() => Some(stage.to_string()),
                _ => None,
            };
            if detail.is_some() {
                record.detail = detail;
            }
            record.updated_at = now;
        }
        let throttled = inner
            .last_persist_by_job
            .get(id)
            .map(|last| last.elapsed() < PERSIST_THROTTLE)
            .unwrap_or(false);
        if !throttled {
            self.persist_locked(&inner);
            inner.last_persist_by_job.insert(id.to_string(), Instant::now());
        }
    }

    /// Explicit status transition (frontend `jobs_finish`, cancel). Always
    /// forces a disk write.
    pub fn set_status(&self, id: &str, status: JobStatus, error: Option<String>) {
        let mut inner = self.lock();
        let Some(record) = inner.records.get_mut(id) else { return };
        record.status = status;
        record.error = error;
        record.updated_at = now_ms();
        if status == JobStatus::Done {
            record.progress = 1.0;
        }
        self.persist_locked(&inner);
        inner.last_persist_by_job.insert(id.to_string(), Instant::now());
    }

    /// Rust-side end of an operation: only a still-active record becomes
    /// `done`. A cancelled/failed status set by the frontend is kept, and the
    /// frontend call to `jobs_finish` can still correct this fallback (the
    /// command wrapper cannot see whether the operation returned an error).
    pub fn finish(&self, id: &str) {
        let mut inner = self.lock();
        let Some(record) = inner.records.get_mut(id) else { return };
        if record.status.is_terminal() {
            return;
        }
        record.status = JobStatus::Done;
        record.progress = 1.0;
        record.updated_at = now_ms();
        self.persist_locked(&inner);
        inner.last_persist_by_job.insert(id.to_string(), Instant::now());
    }

    /// Removes every terminal record (done/failed/cancelled/interrupted) and
    /// returns the remaining ones, newest first.
    pub fn clear_finished(&self) -> Vec<JobRecord> {
        let mut inner = self.lock();
        inner.records.retain(|_, record| !record.status.is_terminal());
        inner.last_persist_by_job.clear();
        self.persist_locked(&inner);
        let mut records: Vec<JobRecord> = inner.records.values().cloned().collect();
        records.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        records
    }

    /// Marks every active record as cancelled; used by `cancel_all` on desktop
    /// window close so the persisted history does not claim those jobs were
    /// still running.
    pub fn cancel_active(&self) {
        let mut inner = self.lock();
        let now = now_ms();
        let mut changed = false;
        for record in inner.records.values_mut() {
            if !record.status.is_terminal() {
                record.status = JobStatus::Cancelled;
                record.updated_at = now;
                changed = true;
            }
        }
        if changed {
            self.persist_locked(&inner);
        }
    }

    /// Atomic write: serialize the whole history to a temp file next to the
    /// target and rename it over the old file. Renames are atomic on both NTFS
    /// and Android's ext4/f2fs, so a crash mid-write can never truncate the
    /// previous history.
    fn persist_locked(&self, inner: &StoreInner) {
        let Some(path) = self.path.get() else { return };
        let mut records: Vec<&JobRecord> = inner.records.values().collect();
        records.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        let Ok(json) = serde_json::to_vec_pretty(&records) else { return };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, &json).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }

    /// Keeps the history bounded: oldest terminal records go first, and if
    /// that is not enough, the oldest records overall.
    fn prune_locked(records: &mut HashMap<String, JobRecord>) {
        if records.len() <= MAX_RECORDS {
            return;
        }
        let mut terminal: Vec<(i64, String)> = records
            .values()
            .filter(|record| record.status.is_terminal())
            .map(|record| (record.created_at, record.id.clone()))
            .collect();
        terminal.sort_by_key(|(created, _)| *created);
        for (_, id) in terminal {
            if records.len() <= MAX_RECORDS {
                return;
            }
            records.remove(&id);
        }
        if records.len() > MAX_RECORDS {
            let mut all: Vec<(i64, String)> = records
                .values()
                .map(|record| (record.created_at, record.id.clone()))
                .collect();
            all.sort_by_key(|(created, _)| *created);
            for (_, id) in all {
                if records.len() <= MAX_RECORDS {
                    break;
                }
                records.remove(&id);
            }
        }
    }
}

/// Cancellation registry. Keeps the existing method surface used by
/// commands.rs/ai.rs/vault.rs/pdf_v3.rs and additionally maintains the
/// persistent store.
pub struct JobRegistry {
    jobs: Mutex<HashMap<String, CancelToken>>,
    /// AI jobs use their own cancellation primitive (aicore must not depend on
    /// the PDF engine), so both maps are kept side by side.
    ai_jobs: Mutex<HashMap<String, aicore::CancelToken>>,
    store: Arc<JobStore>,
}

impl JobRegistry {
    pub fn new(store: Arc<JobStore>) -> Self {
        Self {
            jobs: Mutex::new(HashMap::new()),
            ai_jobs: Mutex::new(HashMap::new()),
            store,
        }
    }

    pub fn register(&self, job_id: &str) -> CancelToken {
        self.store.ensure_tracked(job_id, "pdf");
        let token = CancelToken::new();
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.retain(|_, token| !token.is_cancelled());
            jobs.insert(job_id.to_string(), token.clone());
        }
        token
    }

    /// Registers an AI job and returns its cancellation token.
    pub fn register_ai(&self, job_id: &str) -> aicore::CancelToken {
        self.store.ensure_tracked(job_id, "ai");
        let token = aicore::CancelToken::new();
        if let Ok(mut jobs) = self.ai_jobs.lock() {
            jobs.retain(|_, token| !token.is_cancelled());
            jobs.insert(job_id.to_string(), token.clone());
        }
        token
    }

    /// Cancels a running job and drops its token. The persisted record is
    /// marked `cancelled` only when a live token existed; cancelling an
    /// interrupted/unknown job is a no-op (it is not running).
    pub fn cancel(&self, job_id: &str) {
        let mut found = false;
        if let Ok(mut jobs) = self.jobs.lock() {
            if let Some(token) = jobs.remove(job_id) {
                token.cancel();
                found = true;
            }
        }
        if let Ok(mut jobs) = self.ai_jobs.lock() {
            if let Some(token) = jobs.remove(job_id) {
                token.cancel();
                found = true;
            }
        }
        if found {
            self.store.set_status(job_id, JobStatus::Cancelled, None);
        }
    }

    /// Cleans up the cancel token and lets the store decide the fallback
    /// terminal status (a frontend `jobs_finish` can still override it).
    pub fn finish(&self, job_id: &str) {
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.remove(job_id);
        }
        if let Ok(mut jobs) = self.ai_jobs.lock() {
            jobs.remove(job_id);
        }
        self.store.finish(job_id);
    }

    pub fn cancel_all(&self) {
        if let Ok(mut jobs) = self.jobs.lock() {
            for token in jobs.values() {
                token.cancel();
            }
            jobs.clear();
        }
        if let Ok(mut jobs) = self.ai_jobs.lock() {
            for token in jobs.values() {
                token.cancel();
            }
            jobs.clear();
        }
        self.store.cancel_active();
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressPayload {
    pub job_id: String,
    pub stage: String,
    pub current: u64,
    pub total: u64,
    pub message: Option<String>,
}

/// Emits the `job:progress` event (payload shape is a frontend contract) and
/// mirrors the update into the persistent store.
pub fn emit_progress(app: &AppHandle, job_id: &str, event: &ProgressEvent) {
    if let Some(store) = app.try_state::<Arc<JobStore>>() {
        store.set_progress(
            job_id,
            &event.stage,
            event.current,
            event.total,
            event.message.as_deref(),
        );
    }
    let payload = ProgressPayload {
        job_id: job_id.to_string(),
        stage: event.stage.clone(),
        current: event.current,
        total: event.total,
        message: event.message.clone(),
    };
    // Progress is best-effort: a closed window must not abort the job.
    let _ = app.emit("job:progress", payload);
}

// ---------------------------------------------------------------------------
// Tauri commands (frontend contracts - see src/lib/jobs.ts)
// ---------------------------------------------------------------------------

/// Full persisted history, newest first.
#[tauri::command]
pub fn jobs_list(store: State<'_, Arc<JobStore>>) -> Vec<JobRecord> {
    store.records()
}

/// Drops done/failed/cancelled/interrupted records and returns what remains.
#[tauri::command]
pub fn jobs_clear_finished(store: State<'_, Arc<JobStore>>) -> Vec<JobRecord> {
    store.clear_finished()
}

/// Registers (or restarts) a job from the frontend with kind/title/payload.
#[tauri::command]
pub fn jobs_register(
    store: State<'_, Arc<JobStore>>,
    id: String,
    kind: String,
    title: String,
    payload: Option<serde_json::Value>,
) -> Result<JobRecord, String> {
    store.upsert(&id, &kind, &title, payload);
    store
        .record(&id)
        .ok_or_else(|| format!("job {id} missing after register"))
}

/// Progress mirror for events the Rust side does not emit itself (AI jobs emit
/// `ai:progress` directly); idempotent with `job:progress`.
#[tauri::command]
pub fn jobs_progress(
    store: State<'_, Arc<JobStore>>,
    id: String,
    stage: String,
    current: u64,
    total: u64,
    message: Option<String>,
) {
    store.set_progress(&id, &stage, current, total, message.as_deref());
}

/// Terminal status reported by the producer ("succeeded" | "failed" |
/// "cancelled" | "interrupted").
#[tauri::command]
pub fn jobs_finish(
    store: State<'_, Arc<JobStore>>,
    id: String,
    status: String,
    error: Option<String>,
) -> Result<(), String> {
    let status = match status.as_str() {
        "succeeded" | "done" => JobStatus::Done,
        "failed" => JobStatus::Failed,
        "cancelled" => JobStatus::Cancelled,
        "interrupted" => JobStatus::Interrupted,
        other => return Err(format!("unsupported job status {other}")),
    };
    store.set_status(&id, status, error);
    Ok(())
}

/// Returns the persisted record (including its payload) so the frontend can
/// route it back to the originating screen. The status is intentionally left
/// untouched: the screen starts a new run through the normal `start` path.
#[tauri::command]
pub fn jobs_retry(store: State<'_, Arc<JobStore>>, id: String) -> Result<JobRecord, String> {
    store.record(&id).ok_or_else(|| format!("job {id} not found"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("pdfsak-jobs-{tag}-{}.json", uuid::Uuid::new_v4()))
    }

    fn read_records(path: &PathBuf) -> Vec<JobRecord> {
        serde_json::from_slice(&std::fs::read(path).expect("jobs file")).expect("valid jobs json")
    }

    #[test]
    fn register_progress_finish_roundtrip() {
        let path = temp_path("roundtrip");
        let store = JobStore::default();
        store.attach_path(path.clone());
        store.upsert("job-1", "ocr", "OCR scan", Some(serde_json::json!({"path": "in.pdf"})));
        assert_eq!(store.record("job-1").unwrap().status, JobStatus::Running);

        store.set_progress("job-1", "ocr", 3, 10, Some("page 3"));
        let record = store.record("job-1").unwrap();
        assert!((record.progress - 0.3).abs() < f64::EPSILON);
        assert_eq!(record.detail.as_deref(), Some("page 3"));

        store.set_status("job-1", JobStatus::Done, None);
        store.finish("job-1");
        let persisted = read_records(&path);
        assert_eq!(persisted.len(), 1);
        assert_eq!(persisted[0].status, JobStatus::Done);
        assert_eq!(persisted[0].progress, 1.0);
        assert_eq!(
            persisted[0].payload.as_ref().unwrap()["path"].as_str(),
            Some("in.pdf")
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn reload_marks_running_and_queued_interrupted() {
        let path = temp_path("interrupt");
        let first = JobStore::default();
        first.attach_path(path.clone());
        first.upsert("job-running", "pdf", "Merge", None);
        first.upsert("job-queued", "pdf", "Split", None);
        first.set_status("job-queued", JobStatus::Queued, None);
        first.upsert("job-done", "pdf", "Compress", None);
        first.set_status("job-done", JobStatus::Done, None);
        drop(first);

        // Simulates a fresh process: everything active in the old process was
        // killed without a chance to report a terminal status.
        let second = JobStore::default();
        second.attach_path(path.clone());
        assert_eq!(second.record("job-running").unwrap().status, JobStatus::Interrupted);
        assert_eq!(second.record("job-queued").unwrap().status, JobStatus::Interrupted);
        assert_eq!(second.record("job-done").unwrap().status, JobStatus::Done);
        let persisted = read_records(&path);
        assert!(persisted
            .iter()
            .all(|record| record.status != JobStatus::Running));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn progress_write_is_throttled_but_terminal_transition_flushes() {
        let path = temp_path("throttle");
        let store = JobStore::default();
        store.attach_path(path.clone());
        store.upsert("job-1", "pdf", "Merge", None);

        // Registration itself counts as a write; wait out the throttle window
        // so the next progress update is allowed to hit the disk.
        std::thread::sleep(PERSIST_THROTTLE + Duration::from_millis(20));
        store.set_progress("job-1", "merge", 1, 10, None);
        assert!((store.record("job-1").unwrap().progress - 0.1).abs() < f64::EPSILON);
        // Second update within the throttle window updates memory but must not
        // hit the disk yet.
        store.set_progress("job-1", "merge", 9, 10, None);
        let on_disk = read_records(&path);
        assert!((on_disk[0].progress - 0.1).abs() < f64::EPSILON);

        store.set_status("job-1", JobStatus::Done, None);
        let on_disk = read_records(&path);
        assert_eq!(on_disk[0].status, JobStatus::Done);
        assert_eq!(on_disk[0].progress, 1.0);
        assert!(!path.with_extension("json.tmp").exists(), "temp file must be renamed away");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn progress_ignores_unknown_and_terminal_records() {
        let path = temp_path("guards");
        let store = JobStore::default();
        store.attach_path(path.clone());
        store.set_progress("missing", "page", 1, 2, None);
        assert!(store.records().is_empty());

        store.upsert("job-1", "pdf", "Merge", None);
        store.set_status("job-1", JobStatus::Cancelled, None);
        store.set_progress("job-1", "merge", 5, 10, None);
        assert_eq!(store.record("job-1").unwrap().status, JobStatus::Cancelled);
        assert_eq!(store.record("job-1").unwrap().progress, 0.0);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn clear_finished_keeps_active_records() {
        let path = temp_path("clear");
        let store = JobStore::default();
        store.attach_path(path.clone());
        store.upsert("done", "pdf", "A", None);
        store.set_status("done", JobStatus::Done, None);
        store.upsert("failed", "pdf", "B", None);
        store.set_status("failed", JobStatus::Failed, Some("disk full".into()));
        store.upsert("running", "pdf", "C", None);

        let remaining = store.clear_finished();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, "running");
        assert_eq!(read_records(&path).len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn registry_cancel_marks_cancelled_and_cleans_token() {
        let path = temp_path("cancel");
        let store = JobStore::shared();
        store.attach_path(path.clone());
        let registry = JobRegistry::new(store.clone());
        let token = registry.register("job-1");
        assert_eq!(store.record("job-1").unwrap().status, JobStatus::Running);

        registry.cancel("job-1");
        assert!(token.is_cancelled());
        assert_eq!(store.record("job-1").unwrap().status, JobStatus::Cancelled);

        // A late finish from the worker must not overwrite the cancellation,
        // and a fresh run of the same id gets a fresh token.
        registry.finish("job-1");
        assert_eq!(store.record("job-1").unwrap().status, JobStatus::Cancelled);
        let fresh = registry.register("job-1");
        assert!(!fresh.is_cancelled());
        assert_eq!(store.record("job-1").unwrap().status, JobStatus::Running);

        // Cancelling an unknown job does not fabricate a record.
        registry.cancel("ghost");
        assert!(store.record("ghost").is_none());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn cancel_all_marks_active_records_cancelled() {
        let path = temp_path("cancel-all");
        let store = JobStore::shared();
        store.attach_path(path.clone());
        let registry = JobRegistry::new(store.clone());
        let pdf = registry.register("pdf-job");
        let ai = registry.register_ai("ai-job");
        registry.cancel_all();
        assert!(pdf.is_cancelled());
        assert!(ai.is_cancelled());
        assert_eq!(store.record("pdf-job").unwrap().status, JobStatus::Cancelled);
        assert_eq!(store.record("ai-job").unwrap().status, JobStatus::Cancelled);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn ensure_tracked_preserves_enriched_fields_but_restarts_terminal_ids() {
        let path = temp_path("ensure");
        let store = JobStore::default();
        store.attach_path(path.clone());
        store.ensure_tracked("vault-scan", "pdf");
        assert_eq!(store.record("vault-scan").unwrap().status, JobStatus::Running);

        // Frontend enrichment arrives after the Rust register call.
        store.upsert("vault-scan", "vault", "Vault scan", Some(serde_json::json!({"folders": []})));
        store.set_status("vault-scan", JobStatus::Done, None);
        // Next scan reuses the id: the record must go back to running but keep
        // the enriched kind/title/payload.
        store.ensure_tracked("vault-scan", "pdf");
        let record = store.record("vault-scan").unwrap();
        assert_eq!(record.status, JobStatus::Running);
        assert_eq!(record.kind, "vault");
        assert_eq!(record.title, "Vault scan");
        assert!(record.payload.is_some());
        let _ = std::fs::remove_file(&path);
    }
}

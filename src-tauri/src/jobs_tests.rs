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
    assert_eq!(persisted[0].payload.as_ref().unwrap()["path"].as_str(), Some("in.pdf"));
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
    assert!(persisted.iter().all(|record| record.status != JobStatus::Running));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn an_unknown_job_status_is_a_typed_error() {
    // `jobs_finish` used to answer with a bare string; the UI now gets a
    // code it can localize (E2.4).
    let error = job_status("not-a-status").expect_err("refused");
    let wire = serde_json::to_value(&error).expect("serializes");
    assert_eq!(wire["code"], "invalid_input", "{wire}");
    assert!(job_status("done").is_ok());
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
fn complete_marks_a_failure_as_failed_not_done() {
    let path = temp_path("complete");
    let store = JobStore::shared();
    store.attach_path(path.clone());
    let registry = JobRegistry::new(store.clone());

    let _ = registry.register("job-fail");
    registry.complete("job-fail", false);
    assert_eq!(store.record("job-fail").unwrap().status, JobStatus::Failed);
    assert_eq!(read_records(&path)[0].status, JobStatus::Failed);

    let _ = registry.register("job-ok");
    registry.complete("job-ok", true);
    assert_eq!(store.record("job-ok").unwrap().status, JobStatus::Done);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn reusing_an_id_cancels_the_previous_run() {
    let path = temp_path("reuse");
    let store = JobStore::shared();
    store.attach_path(path.clone());
    let registry = JobRegistry::new(store.clone());

    let first = registry.register("vault-scan");
    let second = registry.register("vault-scan");
    assert!(first.is_cancelled(), "the old worker must be cancelled, not orphaned");
    assert!(!second.is_cancelled());

    // Cancel now reaches the run the UI sees.
    registry.cancel("vault-scan");
    assert!(second.is_cancelled());
    assert_eq!(store.record("vault-scan").unwrap().status, JobStatus::Cancelled);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn corrupt_history_is_quarantined_not_destroyed() {
    let path = temp_path("corrupt");
    std::fs::write(&path, b"{not json").expect("write corrupt");
    let store = JobStore::default();
    store.attach_path(path.clone());
    assert!(store.records().is_empty());

    let dir = path.parent().expect("temp dir").to_path_buf();
    let preserved = std::fs::read_dir(&dir).expect("read temp dir").filter_map(|entry| entry.ok()).any(|entry| {
        let name = entry.file_name().to_string_lossy().to_string();
        name.starts_with("jobs.corrupt-")
            && std::fs::read(entry.path()).map(|bytes| bytes == b"{not json").unwrap_or(false)
    });
    assert!(preserved, "the unreadable history must be preserved in a quarantine file");
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

/// Records every report of the activity listener, in order.
fn recording_registry(tag: &str) -> (JobRegistry, Arc<Mutex<Vec<bool>>>, PathBuf) {
    let path = temp_path(tag);
    let store = JobStore::shared();
    store.attach_path(path.clone());
    let registry = JobRegistry::new(store);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    registry.set_activity_listener(Box::new(move |running| sink.lock().unwrap().push(running)));
    (registry, seen, path)
}

fn seen_of(seen: &Arc<Mutex<Vec<bool>>>) -> Vec<bool> {
    seen.lock().unwrap().clone()
}

#[test]
fn activity_listener_hears_only_first_start_and_last_end() {
    let (registry, seen, path) = recording_registry("activity");
    let _pdf = registry.register("pdf-job");
    assert_eq!(seen_of(&seen), vec![true]);

    // A second job (either kind) does not repeat the report, and the first
    // one ending does not stop the service while the other still runs.
    let _ai = registry.register_ai("ai-job");
    registry.complete("pdf-job", true);
    assert_eq!(seen_of(&seen), vec![true]);

    registry.cancel("ai-job");
    assert_eq!(seen_of(&seen), vec![true, false]);

    // Ending something that is not running reports nothing.
    registry.finish("ai-job");
    registry.complete("ghost", false);
    registry.cancel("ghost");
    assert_eq!(seen_of(&seen), vec![true, false]);

    let _next = registry.register("next-job");
    assert_eq!(seen_of(&seen), vec![true, false, true]);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn activity_listener_covers_every_way_a_job_can_end() {
    let (registry, seen, path) = recording_registry("activity-ends");
    let _ = registry.register("a");
    registry.finish("a");
    let _ = registry.register("b");
    registry.complete("b", false);
    let _ = registry.register("c");
    registry.fail_active("c");
    let _ = registry.register("d");
    let _ = registry.register_ai("e");
    registry.cancel_all();
    assert_eq!(seen_of(&seen), vec![true, false, true, false, true, false, true, false]);

    // The managed AI guard ends its job on drop.
    let (_token, guard) = registry.register_ai_managed("guarded");
    assert_eq!(seen_of(&seen).last(), Some(&true));
    drop(guard);
    assert_eq!(seen_of(&seen).last(), Some(&false));
    assert_eq!(seen_of(&seen).len(), 10);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn reusing_an_id_keeps_the_service_running() {
    let (registry, seen, path) = recording_registry("activity-reuse");
    let _ = registry.register("vault-scan");
    let _ = registry.register("vault-scan");
    assert_eq!(seen_of(&seen), vec![true], "the second run replaces the first without a stop in between");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_registry_without_a_listener_works_as_before() {
    let path = temp_path("no-listener");
    let store = JobStore::shared();
    store.attach_path(path.clone());
    let registry = JobRegistry::new(store.clone());
    let token = registry.register("job");
    registry.complete("job", true);
    assert!(!token.is_cancelled());
    assert_eq!(store.record("job").unwrap().status, JobStatus::Done);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_managed_guard_only_reports_done_when_the_worker_succeeded() {
    let path = temp_path("guard-outcomes");
    let store = JobStore::shared();
    store.attach_path(path.clone());
    let registry = JobRegistry::new(store.clone());

    // Dropped without `succeed`: the command never finished its work.
    {
        let (_cancel, _guard) = registry.register_ai_managed("job-failed");
    }
    assert_eq!(store.record("job-failed").unwrap().status, JobStatus::Failed);

    // Cancelled through the registry (the UI path): the sticky Cancelled
    // state survives the guard's failure report.
    {
        let (_cancel, _guard) = registry.register_ai_managed("job-cancelled");
        registry.cancel("job-cancelled");
    }
    assert_eq!(store.record("job-cancelled").unwrap().status, JobStatus::Cancelled);

    // A succeeded guard reports Done.
    {
        let (_cancel, mut guard) = registry.register_ai_managed("job-done");
        guard.succeed();
    }
    assert_eq!(store.record("job-done").unwrap().status, JobStatus::Done);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn register_managed_cancels_token_and_stops_activity_on_early_drop() {
    let (registry, seen, path) = recording_registry("managed-pdf");
    let store = registry.store.clone();

    let (pdf_token, guard) = registry.register_managed("vault-index");
    assert_eq!(seen_of(&seen), vec![true]);
    assert!(!pdf_token.is_cancelled());
    assert_eq!(store.record("vault-index").unwrap().status, JobStatus::Running);

    // Simulates an early `?` return or a dropped command future before
    // `succeed()`: the worker's cancel token must be tripped immediately and
    // the foreground-service activity listener must hear `false`.
    drop(guard);
    assert!(pdf_token.is_cancelled(), "early drop must cancel the running worker token");
    assert_eq!(seen_of(&seen), vec![true, false]);
    assert_eq!(store.record("vault-index").unwrap().status, JobStatus::Failed);

    // A subsequent managed run that calls `succeed()` completes cleanly without
    // cancelling its token.
    let (ok_token, mut ok_guard) = registry.register_managed("vault-index");
    assert!(!ok_token.is_cancelled());
    ok_guard.succeed();
    drop(ok_guard);
    assert!(!ok_token.is_cancelled());
    assert_eq!(store.record("vault-index").unwrap().status, JobStatus::Done);
    assert_eq!(seen_of(&seen), vec![true, false, true, false]);
    let _ = std::fs::remove_file(&path);
}

/// AI commands register their job through `ensure_tracked`, which never
/// captures the request, so a chat question or history can never reach
/// `jobs.json`.
#[test]
fn ai_jobs_persist_no_payload() {
    let path = std::env::temp_dir().join(format!("pdfsak-jobs-ai-{}.json", uuid::Uuid::new_v4()));
    let store = JobStore::default();
    store.attach_path(path.clone());
    store.ensure_tracked("ai-1", "ai");
    store.set_status("ai-1", JobStatus::Done, None);
    store.finish("ai-1");
    let persisted: Vec<JobRecord> =
        serde_json::from_slice(&std::fs::read(&path).expect("jobs file")).expect("valid jobs json");
    assert_eq!(persisted.len(), 1);
    assert!(persisted[0].payload.is_none(), "AI jobs must not persist their input");
    let _ = std::fs::remove_file(&path);
}

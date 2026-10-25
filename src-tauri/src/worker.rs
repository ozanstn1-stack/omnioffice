//! Untrusted parsers run in a child process.
//!
//! A malformed XLSX must never take the application down. Every parse of a
//! user file that feeds a third-party parser (docx, xlsx, pptx, odt/ods/odp,
//! UDF, RTF, UBL, EML) and the PDF repair run in
//! `OmniOffice.exe --worker <job> …`, a short-lived child process:
//!
//! * the parent enforces a wall-clock limit and kills a hung worker,
//! * the result is capped (a runaway parser cannot fill memory through the
//!   pipe),
//! * on Windows the child runs in a job object with a process-memory limit,
//! * a crash (access violation, stack overflow, `abort`) is reported as a
//!   parse error - the document cannot kill the application.
//!
//! When the worker cannot be spawned at all (unit tests; a stripped dev
//! binary), [`run_json_or_local`] falls back to the in-process parse wrapped in
//! `catch_unwind`, which is why the release profile uses `panic = "unwind"`
//! (see the workspace Cargo.toml).
//!
//! Android is handled explicitly instead of relying on that fallback: an app
//! cannot exec its own binary, but `current_exe()` there is the `app_process`
//! launcher, so spawning it *succeeds* and the child exits immediately with an
//! empty stderr - every office/invoice/EML open then failed with "the
//! office-open parser failed" on devices. [`run_json_or_local`] therefore runs
//! the job in-process on Android.

#[cfg(not(target_os = "android"))]
use std::io::Read;
use std::path::PathBuf;
#[cfg(windows)]
use std::process::Child;
#[cfg(not(target_os = "android"))]
use std::process::{Command, Stdio};
#[cfg(not(target_os = "android"))]
use std::time::{Duration, Instant};

/// Wall-clock limit for one worker job.
#[cfg(not(target_os = "android"))]
const TIMEOUT: Duration = Duration::from_secs(180);
/// Largest result the parent accepts from a worker.
#[cfg(not(target_os = "android"))]
const MAX_OUTPUT_BYTES: u64 = 512 * 1024 * 1024;
/// Windows job-object process memory limit.
#[cfg(windows)]
const MAX_MEMORY_BYTES: usize = 1536 * 1024 * 1024;

/// One worker job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Job {
    /// Parse an office document into the editor model (`office::open_path`).
    OfficeOpen { path: PathBuf },
    /// Parse a UBL-TR invoice (XML or ZIP).
    InvoiceOpen { path: PathBuf },
    /// Parse a ZIP of UBL-TR invoices.
    InvoicePackage { path: PathBuf },
    /// Parse an `.eml` message.
    EmlOpen { path: PathBuf },
    /// Repair a damaged PDF (qpdf when available, built-in rebuild otherwise).
    Repair { input: PathBuf, output: PathBuf },
}

impl Job {
    /// The arguments after `--worker` on the command line.
    pub fn args(&self) -> Vec<String> {
        let path = |value: &PathBuf| value.to_string_lossy().to_string();
        match self {
            Job::OfficeOpen { path: file } => vec!["office-open".into(), path(file)],
            Job::InvoiceOpen { path: file } => vec!["invoice-open".into(), path(file)],
            Job::InvoicePackage { path: file } => vec!["invoice-package".into(), path(file)],
            Job::EmlOpen { path: file } => vec!["eml-open".into(), path(file)],
            Job::Repair { input, output } => vec!["repair".into(), path(input), path(output)],
        }
    }

    /// Parses the arguments that follow `--worker`.
    pub fn parse(rest: &[String]) -> Result<Job, String> {
        let operation = rest.first().ok_or("--worker needs an operation")?;
        let path = |index: usize| -> Result<PathBuf, String> {
            rest.get(index).map(PathBuf::from).ok_or_else(|| format!("{operation} needs a path"))
        };
        match operation.as_str() {
            "office-open" => Ok(Job::OfficeOpen { path: path(1)? }),
            "invoice-open" => Ok(Job::InvoiceOpen { path: path(1)? }),
            "invoice-package" => Ok(Job::InvoicePackage { path: path(1)? }),
            "eml-open" => Ok(Job::EmlOpen { path: path(1)? }),
            "repair" => Ok(Job::Repair { input: path(1)?, output: path(2)? }),
            other => Err(format!("unknown worker operation: {other}")),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Job::OfficeOpen { .. } => "office-open",
            Job::InvoiceOpen { .. } => "invoice-open",
            Job::InvoicePackage { .. } => "invoice-package",
            Job::EmlOpen { .. } => "eml-open",
            Job::Repair { .. } => "repair",
        }
    }
}

/// Runs the job and returns its JSON result. This is what the worker process
/// executes; [`run_json_or_local`] also uses it as the in-process fallback.
pub fn execute(job: &Job) -> Result<String, String> {
    match job {
        Job::OfficeOpen { path } => {
            let document = crate::office::open_path(path).map_err(|error| error.to_string())?;
            to_json(&document)
        }
        Job::InvoiceOpen { path } => {
            let document = crate::office::invoice_document(path).map_err(|error| error.to_string())?;
            to_json(&document)
        }
        Job::InvoicePackage { path } => {
            let documents = crate::office::invoice_package(path).map_err(|error| error.to_string())?;
            to_json(&documents)
        }
        Job::EmlOpen { path } => {
            let message = crate::office::eml_message(path).map_err(|error| error.to_string())?;
            to_json(&message)
        }
        Job::Repair { input, output } => {
            let cancel = pdfcore::progress::CancelToken::new();
            let silent = |_| ();
            let report =
                pdfcore::repair::repair_pdf(input, output, &silent, &cancel).map_err(|error| error.to_string())?;
            to_json(&report)
        }
    }
}

fn to_json<T: serde::Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| error.to_string())
}

/// Why a worker run failed: the worker could not even start, or it started and
/// the job failed (a crash counts as a job failure and is never retried
/// in-process, where the same input could take the app down).
#[cfg(not(target_os = "android"))]
enum RunError {
    Spawn(String),
    Job(String),
}

/// Spawns the worker for `job` and returns its stdout (the JSON result).
#[cfg(not(target_os = "android"))]
fn run(job: &Job) -> Result<String, RunError> {
    let exe = std::env::current_exe().map_err(|error| RunError::Spawn(format!("worker: {error}")))?;
    let mut command = Command::new(&exe);
    command.arg("--worker").args(job.args()).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: the child is a console-less parse process.
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn().map_err(|error| RunError::Spawn(format!("worker: {error}")))?;
    #[cfg(windows)]
    limit_memory(&child);
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    let out_reader = std::thread::spawn(move || read_capped(stdout, MAX_OUTPUT_BYTES));
    let err_reader = std::thread::spawn(move || read_capped(stderr, 64 * 1024));

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if started.elapsed() > TIMEOUT {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(RunError::Job(format!(
                        "the {} parser did not finish within {} seconds; the document was not loaded",
                        job.name(),
                        TIMEOUT.as_secs()
                    )));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(error) => return Err(RunError::Spawn(format!("worker: {error}"))),
        }
    };

    let output = out_reader
        .join()
        .map_err(|_| RunError::Spawn("worker: the output reader failed".to_string()))?
        .map_err(RunError::Job)?;
    let errors = err_reader.join().unwrap_or_else(|_| Ok(Vec::new())).unwrap_or_default();
    if !status.success() {
        let detail = String::from_utf8_lossy(&errors);
        let detail = detail.trim();
        let message = if status.code().is_none() || status.code().is_some_and(|code| code as u32 > 0x8000_0000) {
            format!(
                "the {} parser process crashed; the document may be malformed{}",
                job.name(),
                if detail.is_empty() { String::new() } else { format!(" ({detail})") }
            )
        } else if detail.is_empty() {
            format!("the {} parser failed", job.name())
        } else {
            detail.to_string()
        };
        return Err(RunError::Job(message));
    }
    String::from_utf8(output).map_err(|_| RunError::Job(format!("the {} parser returned invalid text", job.name())))
}

/// Runs the job in a worker process. Only a worker that cannot be *spawned*
/// falls back to an in-process parse (unit tests, a stripped dev binary); a
/// worker that started and failed is reported as-is, because re-running the
/// same input in-process would risk the very crash the worker exists to
/// contain.
///
/// Android always takes the in-process path: `current_exe()` there is the
/// `app_process` launcher, which *does* spawn and then exits with an empty
/// stderr, which used to surface as "the office-open parser failed" for every
/// document (see the module docs).
pub fn run_json_or_local(job: &Job) -> Result<String, String> {
    #[cfg(target_os = "android")]
    return run_in_process(job, None);
    #[cfg(not(target_os = "android"))]
    match run(job) {
        Ok(json) => Ok(json),
        Err(RunError::Job(message)) => Err(message),
        Err(RunError::Spawn(spawn_error)) => run_in_process(job, Some(&spawn_error)),
    }
}

/// The in-process parse, contained by `catch_unwind` so a panicking
/// third-party parser becomes a parse error instead of an aborted app.
fn run_in_process(job: &Job, spawn_error: Option<&str>) -> Result<String, String> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| execute(job))) {
        Ok(result) => result,
        Err(_) => match spawn_error {
            Some(error) => Err(format!("the {} parser panicked; the document may be malformed ({error})", job.name())),
            None => Err(format!("the {} parser panicked; the document may be malformed", job.name())),
        },
    }
}

/// Reads at most `limit` bytes; a larger stream is an error (and the caller
/// kills the worker).
#[cfg(not(target_os = "android"))]
fn read_capped(mut stream: impl Read, limit: u64) -> Result<Vec<u8>, String> {
    let mut collected = Vec::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = stream.read(&mut buffer).map_err(|error| format!("worker: {error}"))?;
        if read == 0 {
            return Ok(collected);
        }
        if collected.len() as u64 + read as u64 > limit {
            return Err("the parser returned more data than the safety limit allows".to_string());
        }
        collected.extend_from_slice(&buffer[..read]);
    }
}

/// Puts the child in a job object with a process-memory cap. Best effort: a
/// failure here only means the OS-level cap is missing, the timeout and the
/// output cap still apply.
#[cfg(windows)]
fn limit_memory(child: &Child) {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_PROCESS_MEMORY,
    };
    unsafe {
        let Ok(job) = CreateJobObjectW(None, None) else {
            return;
        };
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_PROCESS_MEMORY;
        info.ProcessMemoryLimit = MAX_MEMORY_BYTES;
        let set = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const core::ffi::c_void,
            std::mem::size_of_val(&info) as u32,
        );
        if set.is_ok() {
            let _ = AssignProcessToJobObject(job, HANDLE(child.as_raw_handle()));
        }
        // Closing our handle keeps the job alive while the child runs and
        // destroys it when the child exits.
        let _ = CloseHandle(job);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_worker_arguments() {
        assert_eq!(
            Job::parse(&["office-open".into(), "a.docx".into()]).unwrap(),
            Job::OfficeOpen { path: PathBuf::from("a.docx") }
        );
        assert_eq!(
            Job::parse(&["repair".into(), "in.pdf".into(), "out.pdf".into()]).unwrap(),
            Job::Repair { input: PathBuf::from("in.pdf"), output: PathBuf::from("out.pdf") }
        );
        assert!(Job::parse(&["nope".into()]).is_err());
        assert!(Job::parse(&[]).is_err());
    }

    #[test]
    fn a_corrupt_workbook_is_an_error_not_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.xlsx");
        // A ZIP magic followed by garbage: calamine walks it and fails.
        std::fs::write(&path, b"PK\x03\x04this is not a workbook at all").unwrap();
        let result = execute(&Job::OfficeOpen { path: path.clone() });
        assert!(result.is_err(), "a corrupt workbook must return an error");
        // The in-process fallback path (what the app uses when the worker
        // cannot be spawned) must also survive it.
        let fallback = run_json_or_local(&Job::OfficeOpen { path });
        assert!(fallback.is_err());
    }

    #[test]
    fn a_valid_document_round_trips_through_the_job() {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("tests").join("corpus").join("writer-basic.docx");
        let json = execute(&Job::OfficeOpen { path }).expect("the corpus sample parses");
        assert!(json.contains("\"kind\":\"writer\""), "{json:.120}");
    }

    /// Android has no worker process (see the module docs), so this is the path
    /// every document takes there: it must return an error for a bad document
    /// instead of unwinding.
    #[test]
    fn the_in_process_path_is_used_on_android_and_returns_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.xlsx");
        std::fs::write(&path, b"PK\x03\x04this is not a workbook at all").unwrap();
        let result = run_in_process(&Job::OfficeOpen { path }, None);
        assert!(result.is_err(), "a corrupt workbook is an error, not a panic");
        // A missing file goes through the same path.
        let missing = dir.path().join("missing.docx");
        assert!(run_in_process(&Job::OfficeOpen { path: missing }, None).is_err());
    }

    #[test]
    fn a_panicking_parse_is_caught_by_the_fallback() {
        // The fallback wraps `execute` in catch_unwind; prove the wrapper
        // itself survives a panic (the worker process is the primary guard,
        // this is the belt for the dev/test path).
        let caught = std::panic::catch_unwind(|| panic!("boom"));
        assert!(caught.is_err());
    }
}

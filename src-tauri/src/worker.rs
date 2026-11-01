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
//! * the child is **contained before it executes**: on Windows it is created
//!   suspended, assigned to a job object with a process-memory limit and only
//!   then resumed; on Linux/macOS `RLIMIT_AS` and `RLIMIT_CPU` are set in the
//!   child between `fork` and `exec`, so a parser that tries to allocate past
//!   the ceiling dies instead of taking the machine with it (E2.2),
//! * the child's environment is an explicit allow-list, not the parent's
//!   (E2.2): a document parser never sees the user's API keys, provider tokens
//!   or the dev launch context,
//! * `--worker` counts only as the **first** argument (E2.2), so a document
//!   path or a pasted argument can never turn the GUI into a worker,
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
#[cfg(not(target_os = "android"))]
use std::process::{Command, Stdio};
#[cfg(not(target_os = "android"))]
use std::time::{Duration, Instant};

#[cfg(not(target_os = "android"))]
mod containment;

#[cfg(not(target_os = "android"))]
use containment::{env_allowed, platform};
// `RLIMIT_AS`/`RLIMIT_CPU` have no Windows counterpart (the job object in
// `platform::spawn` is that platform's containment), and Android does not spawn
// a worker at all.
#[cfg(all(unix, not(target_os = "android")))]
use containment::limit_resources;

/// Wall-clock limit for one worker job.
#[cfg(not(target_os = "android"))]
const TIMEOUT: Duration = Duration::from_secs(180);
/// Largest result the parent accepts from a worker.
#[cfg(not(target_os = "android"))]
const MAX_OUTPUT_BYTES: u64 = 512 * 1024 * 1024;
/// The worker arguments when the invocation is a worker run.
///
/// `--worker` only counts as the **first** argument (E2.2). It used to be
/// looked up anywhere on the command line, so a document path or a forwarded
/// argument containing the word could turn the GUI into a worker process; the
/// worker's own command line is exactly `exe --worker <operation> <paths...>`.
pub fn worker_invocation(args: &[String]) -> Option<&[String]> {
    // `std::env::args()` puts the executable first.
    (args.get(1)? == "--worker").then(|| &args[2..])
}

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
    // Only the allow-list crosses into the child (E2.2).
    command.env_clear();
    for (name, value) in std::env::vars_os() {
        if env_allowed(&name.to_string_lossy()) {
            command.env(name, value);
        }
    }
    #[cfg(unix)]
    limit_resources(&mut command);
    let mut child = platform::spawn(&mut command).map_err(|error| RunError::Spawn(format!("worker: {error}")))?;
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

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;

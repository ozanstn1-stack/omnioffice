//! How a worker child is contained before it runs (E2.2).
//!
//! The parent already limits what a worker may *do*: the job kinds it accepts,
//! the wall-clock budget and the output cap. This module limits what the child
//! may *touch* while it runs, and it installs those limits **before the child
//! executes a single instruction**:
//!
//! * **Environment.** The child receives an explicit allow-list instead of the
//!   parent's environment, so a parser that processes untrusted documents never
//!   sees the user's API keys, provider tokens or the dev launch context
//!   (`PDFSAK_START_SCREEN`, `PDFSAK_DEV_*`).
//! * **Memory.** Windows: a job object with a process-memory limit. Linux/
//!   macOS: `RLIMIT_AS`, installed between `fork` and `exec`.
//! * **CPU.** Linux/macOS: `RLIMIT_CPU` as a backstop under the wall clock.
//! * **Windows ordering.** The child is created suspended, assigned to the job
//!   object and only then resumed. The previous code assigned the job *after*
//!   `spawn`, so a parser could allocate before the limit applied.

// `Command` is only used by `limit_resources` (the Windows path takes the
// command by reference from `platform::spawn`).
#[cfg(unix)]
use std::process::Command;

/// Ceiling for the worker's memory: the Windows job object's process limit and,
/// on Linux/macOS, `RLIMIT_AS`.
pub(super) const MAX_MEMORY_BYTES: usize = 1536 * 1024 * 1024;
/// CPU seconds the child may burn on Linux/macOS (`RLIMIT_CPU`); the wall-clock
/// limit in the parent still applies, this one catches a spinning parser
/// earlier.
#[cfg(unix)]
const CPU_SECONDS: u64 = 120;

/// Environment variables a worker may inherit. Everything else is dropped: the
/// parse jobs need the engine search path, a temporary directory and the
/// platform basics, and nothing else they read is worth handing to a parser
/// that handles untrusted input.
pub(super) const WORKER_ENV: [&str; 13] = [
    "PATH",
    // The locations the app itself may point at (dev builds, portable
    // installs); `engines::candidate_bases` also probes next to the exe.
    "PDFSAK_ENGINES_DIR",
    // The LibreOffice binary the office fallback may use, when the user set it.
    "PDFSAK_SOFFICE",
    // Platform basics: Windows resolves system DLLs through these, and the
    // Unix parser stack reads the locale and the temp directory.
    "SystemRoot",
    "windir",
    "TEMP",
    "TMP",
    "TMPDIR",
    "HOME",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TZ",
];

/// True when a worker may inherit `name`.
pub(super) fn env_allowed(name: &str) -> bool {
    WORKER_ENV.iter().any(|allowed| allowed.eq_ignore_ascii_case(name))
}

/// Platform containment: how a child is created so the limits apply before it
/// runs.
pub(super) mod platform {
    use std::process::{Child, Command};

    /// On Unix the limits are installed between `fork` and `exec` (see
    /// [`super::limit_resources`]), so the plain spawn is already contained.
    #[cfg(unix)]
    pub(in crate::worker) fn spawn(command: &mut Command) -> std::io::Result<Child> {
        command.spawn()
    }

    /// On Windows the child is created suspended, assigned to the job object
    /// and only then resumed.
    #[cfg(windows)]
    pub(in crate::worker) fn spawn(command: &mut Command) -> std::io::Result<Child> {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: the child is a console-less parse process.
        // CREATE_SUSPENDED: it must not execute before the job object holds it.
        command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
        let mut child = command.spawn()?;
        let job = super::create_limited_job();
        if let Some(job) = job {
            // The only way this fails is the process already being gone, which
            // is fine: a dead process cannot misbehave.
            let _ =
                unsafe { windows::Win32::System::JobObjects::AssignProcessToJobObject(job, process_handle(&child)) };
        }
        // The child must be resumed whether or not the job object exists: a
        // process left suspended would only burn the wall clock.
        if !super::resume_main_thread(child.id()) {
            let _ = child.kill();
            let _ = child.wait();
            if let Some(job) = job {
                let _ = unsafe { windows::Win32::Foundation::CloseHandle(job) };
            }
            return Err(std::io::Error::other("worker: the contained child could not be resumed"));
        }
        // Closing our handle keeps the job alive while the child runs and
        // destroys it when the child exits.
        if let Some(job) = job {
            let _ = unsafe { windows::Win32::Foundation::CloseHandle(job) };
        }
        Ok(child)
    }

    #[cfg(windows)]
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    #[cfg(windows)]
    const CREATE_SUSPENDED: u32 = 0x0000_0004;

    #[cfg(windows)]
    fn process_handle(child: &Child) -> windows::Win32::Foundation::HANDLE {
        use std::os::windows::io::AsRawHandle;
        windows::Win32::Foundation::HANDLE(child.as_raw_handle())
    }
}

/// Installs the memory and CPU ceilings in the child between `fork` and `exec`.
#[cfg(unix)]
pub(super) fn limit_resources(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    unsafe {
        command.pre_exec(|| {
            let memory = libc::rlimit { rlim_cur: MAX_MEMORY_BYTES as u64, rlim_max: MAX_MEMORY_BYTES as u64 };
            if libc::setrlimit(libc::RLIMIT_AS, &memory) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            let cpu = libc::rlimit { rlim_cur: CPU_SECONDS, rlim_max: CPU_SECONDS };
            if libc::setrlimit(libc::RLIMIT_CPU, &cpu) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

/// Creates the job object a Windows worker runs in, with its memory ceiling.
/// `None` when the system refuses, in which case the worker still runs (it only
/// loses the OS-level containment); the wall clock and the output cap stay.
#[cfg(windows)]
fn create_limited_job() -> Option<windows::Win32::Foundation::HANDLE> {
    use windows::Win32::System::JobObjects::{
        CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_PROCESS_MEMORY,
    };
    unsafe {
        let job = CreateJobObjectW(None, None).ok()?;
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_PROCESS_MEMORY;
        info.ProcessMemoryLimit = MAX_MEMORY_BYTES;
        let set = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const core::ffi::c_void,
            std::mem::size_of_val(&info) as u32,
        );
        if set.is_err() {
            let _ = windows::Win32::Foundation::CloseHandle(job);
            return None;
        }
        Some(job)
    }
}

/// Resumes the single thread a suspended process starts with: a process created
/// with `CREATE_SUSPENDED` has exactly one, so the first entry that belongs to
/// `pid` is the one `CreateProcess` left suspended. `false` when the thread
/// cannot be found or opened within the retry budget, so the caller can kill the
/// child instead of leaving it suspended.
#[cfg(windows)]
fn resume_main_thread(pid: u32) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME};
    // The thread exists once `CreateProcess` returns, but a snapshot can still
    // race the kernel bookkeeping; a few short retries cost nothing.
    for attempt in 0..10 {
        if let Some(thread_id) = first_thread_of(pid) {
            // `binherithandle` is a plain `bool` in windows-rs, not a `BOOL`.
            unsafe {
                if let Ok(thread) = OpenThread(THREAD_SUSPEND_RESUME, false, thread_id) {
                    let _ = ResumeThread(thread);
                    let _ = CloseHandle(thread);
                    return true;
                }
            }
        }
        if attempt < 9 {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    false
}

/// The id of the first thread owned by `pid`, from a thread snapshot.
#[cfg(windows)]
fn first_thread_of(pid: u32) -> Option<u32> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
    };
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0).ok()?;
        let mut entry = THREADENTRY32 { dwSize: std::mem::size_of::<THREADENTRY32>() as u32, ..Default::default() };
        let mut thread_id = None;
        if Thread32First(snapshot, &mut entry).is_ok() {
            loop {
                if entry.th32OwnerProcessID == pid {
                    thread_id = Some(entry.th32ThreadID);
                    break;
                }
                if Thread32Next(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
        thread_id
    }
}

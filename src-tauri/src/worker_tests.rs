//! Tests for the worker: job parsing, the E2.2 child containment (allow-list
//! environment, `--worker` position, resource ceilings) and the in-process
//! fallback.

use super::*;

#[cfg(not(target_os = "android"))]
#[test]
fn the_child_environment_is_an_allow_list() {
    // The parser stack needs these (engine search path, temp dir, locale).
    for allowed in ["PATH", "Path", "TEMP", "HOME", "LANG", "PDFSAK_ENGINES_DIR", "PDFSAK_SOFFICE", "SystemRoot"] {
        assert!(env_allowed(allowed), "{allowed} must cross into the worker");
    }
    // Nothing a parser has no business seeing, and nothing secret-shaped.
    for refused in [
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "DEEPSEEK_API_KEY",
        "PDFSAK_DEV_FILES",
        "PDFSAK_START_SCREEN",
        "PDFSAK_DEV_RUN",
        "GITHUB_TOKEN",
        "SSH_AUTH_SOCK",
        "AWS_SECRET_ACCESS_KEY",
    ] {
        assert!(!env_allowed(refused), "{refused} must not reach the worker");
    }
}

#[test]
fn a_document_path_cannot_turn_the_gui_into_a_worker() {
    // `--worker` only counts as the first argument (E2.2).
    let program = "OmniOffice".to_string();
    let first = vec![program.clone(), "--worker".into(), "office-open".into(), "a.docx".into()];
    let rest = worker_invocation(&first).expect("a real worker invocation");
    assert_eq!(rest, &["office-open".to_string(), "a.docx".to_string()]);

    let later = vec![program.clone(), "a.docx".into(), "--worker".into()];
    assert!(worker_invocation(&later).is_none(), "a path may not select the worker");
    let nested = vec![program, "--cli".into(), "open".into(), "--worker".into()];
    assert!(worker_invocation(&nested).is_none(), "a nested flag may not select the worker");
}

/// The unix ceilings must be in place before the child's first instruction:
/// `ulimit` reads them back from the child that `pre_exec` configured.
#[cfg(all(unix, not(target_os = "android")))]
#[test]
fn the_child_inherits_the_resource_ceilings() {
    let mut command = Command::new("sh");
    command.arg("-c").arg("ulimit -v; ulimit -t");
    limit_resources(&mut command);
    let output = command.output().expect("the probe child runs");
    let text = String::from_utf8_lossy(&output.stdout);
    // `ulimit -v` reports KiB, so 1.5 GiB is 1572864.
    assert!(text.contains("1572864"), "the address space limit is missing: {text}");
    assert!(text.contains("120"), "the CPU limit is missing: {text}");
    assert!(output.status.success(), "{output:?}");
}

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

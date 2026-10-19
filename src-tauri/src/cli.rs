//! Command-line interface.
//!
//! `OmniOffice.exe --cli <command> …` runs a batch operation and returns an
//! exit code (0 success, 1 a command error, 2 a usage error), so automation
//! scripts and the Explorer "Send to" entry can drive the app:
//!
//! ```text
//! OmniOffice.exe --cli merge a.pdf b.pdf -o out.pdf
//! OmniOffice.exe --cli compress big.pdf -o small.pdf
//! OmniOffice.exe --cli ocr scan.pdf -o searchable.pdf --lang tur+eng
//! ```
//!
//! Without `--cli` the app starts as usual. On Windows the process is a GUI
//! subsystem binary, so [`attach_console`] attaches to the parent console and
//! the output is written to `CONOUT$`/`CONERR$`.

use pdfcore::docutil::OverwritePolicy;
use pdfcore::progress::CancelToken;
use std::path::PathBuf;

pub const USAGE: &str = "\
OmniOffice CLI

  OmniOffice.exe --cli merge <in.pdf> <in.pdf> … -o <out.pdf>
  OmniOffice.exe --cli compress <in.pdf> -o <out.pdf>
  OmniOffice.exe --cli ocr <in.pdf> -o <out.pdf> [--lang tur+eng]

Exit codes: 0 success, 1 command error, 2 usage error.";

#[derive(Debug, PartialEq, Eq)]
pub enum CliCommand {
    Merge { inputs: Vec<PathBuf>, output: PathBuf },
    Compress { input: PathBuf, output: PathBuf },
    Ocr { input: PathBuf, output: PathBuf, languages: Vec<String> },
}

/// Parses `--cli …`. `Ok(None)` means the arguments are not a CLI invocation
/// (start the GUI); `Err` is a usage error (`""` means "show the help").
pub fn parse(args: &[String]) -> Result<Option<CliCommand>, String> {
    let Some(start) = args.iter().position(|arg| arg == "--cli") else {
        return Ok(None);
    };
    let rest = &args[start + 1..];
    let Some(subcommand) = rest.first() else {
        return Err("missing subcommand".into());
    };
    if matches!(subcommand.as_str(), "--help" | "-h" | "help") {
        return Err(String::new());
    }
    let mut positional: Vec<String> = Vec::new();
    let mut output: Option<String> = None;
    let mut languages: Option<String> = None;
    let mut index = 1;
    while index < rest.len() {
        match rest[index].as_str() {
            "-o" | "--output" => {
                index += 1;
                output = Some(rest.get(index).cloned().ok_or("-o needs a path")?);
            }
            "--lang" => {
                index += 1;
                languages = Some(rest.get(index).cloned().ok_or("--lang needs a value")?);
            }
            "--help" | "-h" => return Err(String::new()),
            other => positional.push(other.to_string()),
        }
        index += 1;
    }
    let output = PathBuf::from(output.ok_or("missing -o <output>")?);
    match subcommand.as_str() {
        "merge" => {
            if positional.len() < 2 {
                return Err("merge needs at least two input files".into());
            }
            Ok(Some(CliCommand::Merge { inputs: positional.into_iter().map(PathBuf::from).collect(), output }))
        }
        "compress" => {
            let input = positional.into_iter().next().ok_or("compress needs an input file")?;
            Ok(Some(CliCommand::Compress { input: PathBuf::from(input), output }))
        }
        "ocr" => {
            let input = positional.into_iter().next().ok_or("ocr needs an input file")?;
            let languages = languages
                .map(|value| {
                    value.split(['+', ',']).map(str::trim).filter(|part| !part.is_empty()).map(str::to_string).collect()
                })
                .unwrap_or_else(|| vec!["tur".to_string(), "eng".to_string()]);
            Ok(Some(CliCommand::Ocr { input: PathBuf::from(input), output, languages }))
        }
        other => Err(format!("unknown subcommand: {other}")),
    }
}

/// Runs a parsed command and returns a short success message.
pub fn execute(command: &CliCommand) -> Result<String, String> {
    let silent = |_| ();
    let cancel = CancelToken::new();
    let policy = OverwritePolicy::Replace;
    match command {
        CliCommand::Merge { inputs, output } => {
            let (path, pages) =
                pdfcore::merge::merge_files(inputs, output, &Default::default(), policy, &silent, &cancel)
                    .map_err(|error| error.to_string())?;
            Ok(format!("merged {pages} page(s) into {}", path.display()))
        }
        CliCommand::Compress { input, output } => {
            pdfcore::compress::compress_pdf(input, output, &Default::default(), policy, None, &silent, &cancel)
                .map_err(|error| error.to_string())?;
            Ok(format!("compressed into {}", output.display()))
        }
        CliCommand::Ocr { input, output, languages } => {
            let options = pdfcore::ocr::OcrOptions { languages: languages.clone(), ..Default::default() };
            let result = pdfcore::ocr::ocr_pdf(input, output, &options, policy, None, &silent, &cancel)
                .map_err(|error| error.to_string())?;
            Ok(format!("OCR'd {} page(s) into {}", result.pages_processed, output.display()))
        }
    }
}

/// Attaches to the parent process console so CLI output is visible. The app is
/// built as a GUI subsystem binary, so without this a `--cli` run prints
/// nothing.
#[cfg(windows)]
pub fn attach_console() {
    use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
    unsafe {
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

#[cfg(not(windows))]
pub fn attach_console() {}

fn write_line(message: &str, error: bool) {
    use std::io::Write;
    #[cfg(windows)]
    {
        let target = if error { "CONERR$" } else { "CONOUT$" };
        if let Ok(mut console) = std::fs::OpenOptions::new().write(true).open(target) {
            let _ = writeln!(console, "{message}");
            return;
        }
    }
    if error {
        let _ = writeln!(std::io::stderr(), "{message}");
    } else {
        let _ = writeln!(std::io::stdout(), "{message}");
    }
}

/// Runs the CLI when `--cli` is present, returning the process exit code.
/// `None` means "not a CLI invocation; start the GUI".
pub fn run_if_present(args: Vec<String>) -> Option<i32> {
    if let Some(index) = args.iter().position(|arg| arg == "--worker") {
        return Some(run_worker(&args[index + 1..]));
    }
    match parse(&args) {
        Ok(None) => None,
        Ok(Some(command)) => {
            attach_console();
            match execute(&command) {
                Ok(message) => {
                    write_line(&message, false);
                    Some(0)
                }
                Err(error) => {
                    write_line(&format!("error: {error}"), true);
                    Some(1)
                }
            }
        }
        Err(message) => {
            attach_console();
            if message.is_empty() {
                write_line(USAGE, false);
                Some(0)
            } else {
                write_line(&format!("error: {message}\n\n{USAGE}"), true);
                Some(2)
            }
        }
    }
}

/// Runs one worker job (see `crate::worker`): the JSON result goes to stdout,
/// a failure to stderr with exit code 1. This process exists to contain a
/// parser crash, so nothing else may be written to stdout.
fn run_worker(rest: &[String]) -> i32 {
    use std::io::Write as _;
    let job = match crate::worker::Job::parse(rest) {
        Ok(job) => job,
        Err(error) => {
            let _ = writeln!(std::io::stderr(), "error: {error}");
            return 2;
        }
    };
    match crate::worker::execute(&job) {
        Ok(json) => {
            let _ = std::io::stdout().write_all(json.as_bytes());
            0
        }
        Err(error) => {
            let _ = writeln!(std::io::stderr(), "error: {error}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn a_gui_launch_is_not_a_cli_invocation() {
        assert_eq!(parse(&args(&["OmniOffice.exe", "a.pdf"])), Ok(None));
        assert_eq!(parse(&args(&[])), Ok(None));
    }

    #[test]
    fn parses_merge_with_output() {
        let command = parse(&args(&["app", "--cli", "merge", "a.pdf", "b.pdf", "-o", "out.pdf"])).unwrap();
        assert_eq!(
            command,
            Some(CliCommand::Merge {
                inputs: vec![PathBuf::from("a.pdf"), PathBuf::from("b.pdf")],
                output: PathBuf::from("out.pdf"),
            })
        );
    }

    #[test]
    fn parses_ocr_languages_and_defaults() {
        let command = parse(&args(&["app", "--cli", "ocr", "s.pdf", "-o", "o.pdf", "--lang", "tur+eng"])).unwrap();
        assert_eq!(
            command,
            Some(CliCommand::Ocr {
                input: PathBuf::from("s.pdf"),
                output: PathBuf::from("o.pdf"),
                languages: vec!["tur".to_string(), "eng".to_string()],
            })
        );
        let default = parse(&args(&["app", "--cli", "ocr", "s.pdf", "-o", "o.pdf"])).unwrap();
        assert_eq!(
            default,
            Some(CliCommand::Ocr {
                input: PathBuf::from("s.pdf"),
                output: PathBuf::from("o.pdf"),
                languages: vec!["tur".to_string(), "eng".to_string()],
            })
        );
    }

    #[test]
    fn reports_usage_errors() {
        assert!(parse(&args(&["app", "--cli"])).is_err());
        assert!(parse(&args(&["app", "--cli", "merge", "a.pdf"])).is_err(), "one input is not enough");
        assert!(parse(&args(&["app", "--cli", "merge", "a.pdf", "b.pdf"])).is_err(), "no output");
        assert!(parse(&args(&["app", "--cli", "frobnicate", "-o", "x"])).is_err());
        assert_eq!(parse(&args(&["app", "--cli", "--help"])), Err(String::new()));
    }

    #[test]
    fn merge_command_produces_a_pdf() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("a.pdf");
        let second = dir.path().join("b.pdf");
        for path in [&first, &second] {
            let mut doc = lopdf::Document::with_version("1.5");
            let pages_id = doc.new_object_id();
            let content = doc.add_object(lopdf::Stream::new(lopdf::Dictionary::new(), b"BT ET".to_vec()));
            let page = doc.add_object(lopdf::dictionary! {
                "Type" => "Page", "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()], "Contents" => content,
            });
            let pages = lopdf::dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 };
            doc.objects.insert(pages_id, lopdf::Object::Dictionary(pages));
            let catalog = doc.add_object(lopdf::dictionary! { "Type" => "Catalog", "Pages" => pages_id });
            doc.trailer.set("Root", catalog);
            doc.save(path).unwrap();
        }
        let output = dir.path().join("merged.pdf");
        let command = CliCommand::Merge { inputs: vec![first, second], output: output.clone() };
        let message = execute(&command).unwrap();
        assert!(message.contains("merged 2 page"), "{message}");
        assert!(std::fs::read(&output).unwrap().starts_with(b"%PDF"));
    }
}

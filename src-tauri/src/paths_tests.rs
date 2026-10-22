//! Unit tests for the path validators (kept apart to respect the file-size ratchet).
use super::*;

fn scratch_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pdfsak-paths-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

#[test]
fn input_requires_an_existing_regular_file() {
    let dir = scratch_dir();
    let file = dir.join("input.pdf");
    std::fs::write(&file, b"%PDF").expect("write fixture");
    assert_eq!(input_file(file.to_str().unwrap()).unwrap().as_path(), file);

    assert!(matches!(input_file(dir.to_str().unwrap()), Err(PdfError::InvalidInput(_))));
    assert!(matches!(input_file(dir.join("missing.pdf").to_str().unwrap()), Err(PdfError::NotFound(_))));
}

#[test]
fn traversal_and_device_names_are_refused() {
    assert!(input_file("C:\\docs\\..\\secret.pdf").is_err());
    assert!(input_file("relative.pdf").is_err());
    assert!(input_file("").is_err());
    assert!(input_file("C:\\docs\\nul.pdf").is_err());
    assert!(input_file("C:\\docs\\con").is_err());
    assert!(output_file("C:\\docs\\..\\out.pdf").is_err());
    assert!(output_file("out.pdf").is_err());
    assert!(output_file("").is_err());
}

#[test]
fn unc_and_extended_paths_are_refused() {
    assert!(input_file("\\\\server\\share\\a.pdf").is_err());
    assert!(output_file(r"\\?\C:\docs\out.pdf").is_err());
    assert!(directory("\\\\server\\share").is_err());
}

#[test]
fn nul_and_control_characters_are_refused() {
    assert!(input_file("C:\\docs\\a\0b.pdf").is_err());
    assert!(output_file("C:\\docs\\a\u{7}b.pdf").is_err());
}

#[test]
fn output_requires_an_existing_parent_folder() {
    let dir = scratch_dir();
    let target = dir.join("out.pdf");
    assert_eq!(output_file(target.to_str().unwrap()).unwrap().as_path(), target);
    assert!(matches!(
        output_file(dir.join("missing-dir").join("out.pdf").to_str().unwrap()),
        Err(PdfError::InvalidInput(_))
    ));
}

#[test]
fn directory_requires_an_existing_folder() {
    let dir = scratch_dir();
    assert_eq!(directory(dir.to_str().unwrap()).unwrap().as_path(), dir);
    assert!(matches!(directory(dir.join("nope").to_str().unwrap()), Err(PdfError::NotFound(_))));

    let file = dir.join("a-file.pdf");
    std::fs::write(&file, b"%PDF").expect("write");
    assert!(matches!(directory(file.to_str().unwrap()), Err(PdfError::InvalidInput(_))));
}

#[test]
fn unicode_and_space_paths_are_accepted() {
    let dir = scratch_dir();
    let file = dir.join("belge ç Ğ ü.pdf");
    std::fs::write(&file, b"%PDF").expect("write");
    assert_eq!(input_file(file.to_str().unwrap()).unwrap().as_path(), file);
}

#[test]
fn a_file_token_flows_through_the_input_and_output_validators() {
    let dir = scratch_dir();
    let input = dir.join("token-input.pdf");
    std::fs::write(&input, b"%PDF").expect("write fixture");
    let read = crate::files::issue(&input, crate::files::FileAccess::Read);
    assert_eq!(input_file(&read.token).unwrap().as_path(), input);
    assert!(
        matches!(output_file(&read.token), Err(PdfError::InvalidInput(_))),
        "a read token must not be accepted as a write target"
    );

    let output = dir.join("token-output.pdf");
    std::fs::write(&output, b"%PDF").expect("write fixture");
    let write = crate::files::issue(&output, crate::files::FileAccess::ReadWrite);
    assert_eq!(output_file(&write.token).unwrap().as_path(), output);
    assert_eq!(input_file(&write.token).unwrap().as_path(), output);
    assert_eq!(lexical(&read.token).unwrap().as_path(), input);
}

#[test]
fn read_input_file_enforces_the_size_cap() {
    let dir = scratch_dir();
    let file = dir.join("big.pdf");
    std::fs::write(&file, vec![0u8; 2048]).expect("write");
    assert!(matches!(read_input_file(file.to_str().unwrap(), 1024), Err(PdfError::InvalidInput(_))));
    assert_eq!(read_input_file(file.to_str().unwrap(), 4096).unwrap().len(), 2048);
}

#[cfg(unix)]
#[test]
fn read_input_file_rejects_a_symlink_swap() {
    use std::os::unix::fs::symlink;
    let dir = scratch_dir();
    let real = dir.join("real.pdf");
    std::fs::write(&real, b"%PDF real").expect("write");
    let link = dir.join("link.pdf");
    let _ = std::fs::remove_file(&link);
    symlink(&real, &link).expect("symlink");
    // A symlink pointing at a regular file is still a valid input (dialog
    // paths can be symlinks); the swap guard only fires on a race, which a
    // single-threaded test cannot trigger. Assert it reads the target.
    assert_eq!(read_input_file(link.to_str().unwrap(), 4096).unwrap(), b"%PDF real");
}

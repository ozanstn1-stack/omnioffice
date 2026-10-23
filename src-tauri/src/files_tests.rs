//! Unit tests for the file capability tokens.
use super::*;

fn temp_file(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pdfsak-token-{label}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let file = dir.join(format!("{label}.pdf"));
    std::fs::write(&file, b"%PDF-1.7\n").expect("temp file");
    file
}

#[test]
fn a_token_resolves_to_the_registered_path_and_keeps_only_the_name() {
    let path = temp_file("resolve");
    let issued = issue(&path, FileAccess::Read);
    assert!(issued.name.ends_with("resolve.pdf"));
    assert!(!issued.name.contains(std::path::MAIN_SEPARATOR));
    assert_eq!(resolve(&issued.token, FileAccess::Read).expect("resolve"), path);
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

#[test]
fn a_read_token_cannot_be_used_for_writing() {
    let path = temp_file("readonly");
    let issued = issue(&path, FileAccess::Read);
    assert!(resolve(&issued.token, FileAccess::ReadWrite).is_err());
    let writable = issue(&path, FileAccess::ReadWrite);
    assert_eq!(resolve(&writable.token, FileAccess::Read).expect("read via rw"), path);
    assert_eq!(resolve(&writable.token, FileAccess::ReadWrite).expect("write"), path);
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

#[test]
fn releasing_a_token_invalidates_it() {
    let path = temp_file("release");
    let issued = issue(&path, FileAccess::Read);
    assert!(revoke(&issued.token));
    assert!(resolve(&issued.token, FileAccess::Read).is_err());
    assert!(!revoke(&issued.token), "a second release is a no-op");
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

#[test]
fn names_resolve_tokens_and_plain_paths_without_a_path() {
    let path = temp_file("names");
    let issued = issue(&path, FileAccess::Read);
    let names = file_names(vec![issued.token.clone(), path.to_string_lossy().into_owned(), "missing".to_string()]);
    assert_eq!(names[0].as_deref(), Some("names.pdf"));
    assert_eq!(names[1].as_deref(), Some("names.pdf"));
    assert_eq!(names[2].as_deref(), Some("missing"));
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

#[test]
fn unknown_tokens_are_rejected_and_issued_tokens_are_unique() {
    assert!(resolve("not-a-token", FileAccess::Read).is_err());
    let path = temp_file("unique");
    let first = issue(&path, FileAccess::Read);
    let second = issue(&path, FileAccess::Read);
    assert_ne!(first.token, second.token);
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

use super::*;

fn temp_file(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pdfsak-recent-{label}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let file = dir.join(format!("{label}.pdf"));
    std::fs::write(&file, b"%PDF").expect("write");
    file
}

#[test]
fn entries_become_token_refs_and_missing_files_are_dropped() {
    let file = temp_file("kept");
    let missing = file.parent().unwrap().join("gone.pdf");
    let entries = vec![
        RecentEntry {
            path: file.display().to_string(),
            file_name: "kept.pdf".into(),
            tool: "merge".into(),
            timestamp: 2,
        },
        RecentEntry {
            path: missing.display().to_string(),
            file_name: "gone.pdf".into(),
            tool: "merge".into(),
            timestamp: 1,
        },
    ];
    let refs = refs(entries);
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].file_name, "kept.pdf");
    assert_eq!(files::resolve(&refs[0].token, FileAccess::Read).unwrap(), file);
    std::fs::remove_dir_all(file.parent().unwrap()).ok();
}

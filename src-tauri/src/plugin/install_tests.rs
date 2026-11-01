//! Tests for plugin installation: the plan a human confirms, the recorded
//! SHA-256 and the integrity state `plugin_list` reports.

use super::*;

fn manifest() -> String {
    serde_json::json!({
        "id": "test.tool",
        "name": "Test Tool",
        "version": "1.2.0",
        "apiVersion": 1,
        "compatibility": { "app": ">=3.1.0" },
        "permissions": ["read_files", "network"],
        "capabilities": ["command", "files"],
        "commands": [{ "id": "run", "title": "Run" }]
    })
    .to_string()
}

const SOURCE: &str = "self.onPluginMessage = () => 1;";

fn root() -> tempfile::TempDir {
    tempfile::tempdir().expect("temp dir")
}

#[test]
fn a_plan_summarises_what_the_user_confirms() {
    let plan = plan_install(&manifest(), SOURCE).expect("plan");
    assert_eq!(plan.id, "test.tool");
    assert_eq!(plan.name, "Test Tool");
    assert_eq!(plan.version, "1.2.0");
    assert_eq!(plan.permissions, vec!["read_files".to_string(), "network".to_string()]);
    assert_eq!(plan.capabilities, vec!["command".to_string(), "files".to_string()]);
    assert_eq!(plan.source_bytes, SOURCE.len() as u64);
    assert_eq!(plan.sha256.len(), 64);
    let summary = plan.summary();
    for expected in ["Test Tool", "test.tool", "1.2.0", "read_files, network", &plan.sha256] {
        assert!(summary.contains(expected), "the summary omits {expected}: {summary}");
    }
    assert!(summary.contains("Only install plugins you trust"), "{summary}");
}

#[test]
fn a_declined_install_writes_nothing() {
    let dir = root();
    let error = install_confirmed(dir.path(), &manifest(), SOURCE, |plan| {
        assert_eq!(plan.id, "test.tool");
        false
    })
    .expect_err("declined");
    assert_eq!(error.code, "forbidden");
    assert!(error.message.contains("not confirmed"), "{}", error.message);
    assert_eq!(std::fs::read_dir(dir.path()).expect("readable").count(), 0, "no file was written");
}

#[test]
fn a_confirmed_install_records_the_hash_and_verifies_it() {
    let dir = root();
    let entry = install_confirmed(dir.path(), &manifest(), SOURCE, |_| true).expect("install");
    assert_eq!(entry.integrity, "verified");
    assert_eq!(entry.sha256, bundle_sha256(&manifest(), SOURCE));
    let record = std::fs::read_to_string(dir.path().join("test.tool").join("install.json")).expect("record");
    assert!(record.contains(&entry.sha256), "{record}");
    assert!(record.contains("\"confirmed\": true"), "{record}");
    let listed = list_plugins(dir.path());
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].integrity, "verified");
    assert_eq!(listed[0].sha256, entry.sha256);
}

#[test]
fn a_plugin_changed_on_disk_is_reported_as_modified() {
    let dir = root();
    install_confirmed(dir.path(), &manifest(), SOURCE, |_| true).expect("install");
    std::fs::write(dir.path().join("test.tool").join("main.js"), "self.onPluginMessage = () => 2;").expect("rewrite");
    let listed = list_plugins(dir.path());
    assert_eq!(listed[0].integrity, "modified", "a changed plugin must not read as verified");
    assert_eq!(listed[0].sha256, bundle_sha256(&manifest(), SOURCE), "the record keeps the approved hash");
}

#[test]
fn a_plugin_without_a_record_is_unrecorded() {
    let dir = root();
    install_confirmed(dir.path(), &manifest(), SOURCE, |_| true).expect("install");
    std::fs::remove_file(dir.path().join("test.tool").join("install.json")).expect("remove record");
    let listed = list_plugins(dir.path());
    assert_eq!(listed[0].integrity, "unrecorded");
    assert_eq!(listed[0].sha256, bundle_sha256(&manifest(), SOURCE), "the current bytes are described");
}

#[test]
fn the_hash_covers_the_manifest_and_the_source() {
    let base = bundle_sha256(&manifest(), SOURCE);
    assert_eq!(base, bundle_sha256(&manifest(), SOURCE), "the hash is stable");
    assert_ne!(base, bundle_sha256(&manifest(), "other source"), "the source is covered");
    let other_manifest = manifest().replace("1.2.0", "1.2.1");
    assert_ne!(base, bundle_sha256(&other_manifest, SOURCE), "the manifest is covered");
}

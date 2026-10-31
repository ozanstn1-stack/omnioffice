//! Tests for the secret storage: the platform-cipher tiers, the plain-file
//! fallback with its `0600` permission, the keyring tier and the migrations
//! between them.
//!
//! The platform ciphers are replaced with a reversible fake, so the prefix
//! handling, the failure paths and the migration guards run on the host.

use super::*;
use std::cell::Cell;

/// Reversible stand-in for the platform crypto: reverses the bytes.
struct FakeCipher {
    prefix: &'static str,
    migrates: bool,
    fail_protect: bool,
    key_lost: bool,
    protect_calls: Cell<u32>,
}

impl FakeCipher {
    fn keystore() -> Self {
        Self {
            prefix: KEYSTORE_PREFIX,
            migrates: true,
            fail_protect: false,
            key_lost: false,
            protect_calls: Cell::new(0),
        }
    }
}

impl SecretCipher for FakeCipher {
    fn prefix(&self) -> &'static str {
        self.prefix
    }
    fn protect(&self, plaintext: &[u8]) -> Result<Vec<u8>, CipherError> {
        self.protect_calls.set(self.protect_calls.get() + 1);
        if self.fail_protect {
            return Err(CipherError::Failed("encryption unavailable".into()));
        }
        Ok(plaintext.iter().rev().copied().collect())
    }
    fn unprotect(&self, blob: &[u8]) -> Result<Vec<u8>, CipherError> {
        if self.key_lost {
            return Err(CipherError::KeyLost);
        }
        Ok(blob.iter().rev().copied().collect())
    }
    fn migrates_plain(&self) -> bool {
        self.migrates
    }
}

fn plain_payload(value: &str) -> String {
    format!("{PLAIN_PREFIX}{}", base64_encode(value.as_bytes()))
}

fn secret_file(content: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("secret.key");
    std::fs::write(&path, content).unwrap();
    (dir, path)
}

#[test]
fn seal_uses_the_cipher_prefix_and_round_trips() {
    let cipher = FakeCipher::keystore();
    let payload = seal(Some(&cipher), "sk-abc");
    assert!(payload.starts_with("keystore:"));
    assert!(!payload.contains("sk-abc"));
    assert_eq!(cipher.protect_calls.get(), 1);
    let opened = open_via(Some(&cipher), None, &payload).unwrap();
    assert_eq!(opened.value, "sk-abc");
    assert!(!opened.migrate);
}

#[test]
fn seal_falls_back_to_plain_without_a_working_cipher() {
    assert_eq!(seal(None, "k"), plain_payload("k"));
    let mut cipher = FakeCipher::keystore();
    cipher.fail_protect = true;
    assert_eq!(seal(Some(&cipher), "k"), plain_payload("k"));
}

#[test]
fn open_plain_asks_for_migration_only_when_the_cipher_wants_it() {
    let payload = plain_payload("old-key");
    let cipher = FakeCipher::keystore();
    let opened = open_via(Some(&cipher), None, &payload).unwrap();
    assert_eq!((opened.value.as_str(), opened.migrate), ("old-key", true));

    let mut no_migrate = FakeCipher::keystore();
    no_migrate.migrates = false;
    assert!(!open_via(Some(&no_migrate), None, &payload).unwrap().migrate);
    assert!(!open_via(None, None, &payload).unwrap().migrate);
}

#[test]
fn open_reports_a_lost_key_clearly() {
    let mut cipher = FakeCipher::keystore();
    let payload = seal(Some(&cipher), "sk-abc");
    cipher.key_lost = true;
    let message = open_via(Some(&cipher), None, &payload).err().unwrap().to_string();
    assert!(message.contains("can no longer be decrypted"), "{message}");
    assert!(message.contains("enter it again"), "{message}");
}

#[test]
fn open_rejects_values_from_another_platform() {
    let payload = format!("{KEYSTORE_PREFIX}{}", base64_encode(b"xyz"));
    assert!(open_via(None, None, &payload).is_err());
    let mut dpapi_like = FakeCipher::keystore();
    dpapi_like.prefix = DPAPI_PREFIX;
    assert!(open_via(Some(&dpapi_like), None, &payload).is_err());
}

#[test]
fn open_treats_empty_unknown_and_corrupt_content() {
    let cipher = FakeCipher::keystore();
    assert_eq!(open_via(Some(&cipher), None, "").unwrap().value, "");
    assert_eq!(open_via(Some(&cipher), None, "something-else").unwrap().value, "");
    assert!(open_via(Some(&cipher), None, "plain:***not base64***").is_err());
    assert!(open_via(Some(&cipher), None, "keystore:***not base64***").is_err());
    assert_eq!(open_via(Some(&cipher), None, &format!("{}\n", plain_payload("k"))).unwrap().value, "k");
}

#[test]
fn kind_of_names_every_prefix() {
    assert_eq!(kind_of("dpapi:AAAA"), "dpapi");
    assert_eq!(kind_of("keystore:AAAA"), "keystore");
    assert_eq!(kind_of("keyring:secret-00"), "keyring");
    assert_eq!(kind_of("plain:AAAA"), "plain");
    assert_eq!(kind_of(""), "plain");
}

#[test]
fn storage_kind_reads_the_file() {
    let (dir, path) = secret_file("keystore:AAAA");
    assert_eq!(storage_kind(&path), "keystore");
    assert_eq!(storage_kind(&dir.path().join("missing.key")), "none");
}

#[test]
fn save_then_load_round_trips_encrypted() {
    let cipher = FakeCipher::keystore();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("secret.key");
    assert!(save_via(Some(&cipher), None, &path, "  sk-abc \n").unwrap());
    assert_eq!(storage_kind(&path), "keystore");
    assert_eq!(load_via(Some(&cipher), None, &path).unwrap(), "sk-abc");
}

#[test]
fn save_reports_plain_when_encryption_fails() {
    let mut cipher = FakeCipher::keystore();
    cipher.fail_protect = true;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("secret.key");
    assert!(!save_via(Some(&cipher), None, &path, "sk-abc").unwrap());
    assert_eq!(storage_kind(&path), "plain");
    assert_eq!(load_via(Some(&cipher), None, &path).unwrap(), "sk-abc");
}

#[test]
fn load_migrates_a_plain_value_to_encrypted_storage() {
    let cipher = FakeCipher::keystore();
    let (_dir, path) = secret_file(&plain_payload("old-key"));
    assert_eq!(load_via(Some(&cipher), None, &path).unwrap(), "old-key");
    assert_eq!(storage_kind(&path), "keystore");
    assert_eq!(load_via(Some(&cipher), None, &path).unwrap(), "old-key");
}

#[test]
fn load_keeps_the_plain_value_when_migration_cannot_encrypt() {
    let mut cipher = FakeCipher::keystore();
    cipher.fail_protect = true;
    let payload = plain_payload("old-key");
    let (_dir, path) = secret_file(&payload);
    assert_eq!(load_via(Some(&cipher), None, &path).unwrap(), "old-key");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), payload);
}

#[test]
fn load_does_not_migrate_without_a_migrating_cipher() {
    let payload = plain_payload("old-key");
    let (_dir, path) = secret_file(&payload);
    assert_eq!(load_via(None, None, &path).unwrap(), "old-key");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), payload);
}

#[test]
fn load_surfaces_a_lost_key_and_leaves_the_file_alone() {
    let mut cipher = FakeCipher::keystore();
    let (_dir, path) = secret_file(&seal(Some(&cipher), "sk-abc"));
    let before = std::fs::read_to_string(&path).unwrap();
    cipher.key_lost = true;
    let error = load_via(Some(&cipher), None, &path).err().unwrap();
    assert!(error.to_string().contains("enter it again"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
}

#[test]
fn load_of_a_missing_file_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(load_via(None, None, &dir.path().join("none.key")).unwrap(), "");
}

#[test]
fn migration_never_overwrites_a_concurrently_saved_value() {
    let cipher = FakeCipher::keystore();
    let (_dir, path) = secret_file(&plain_payload("newer"));
    // The migration read an older value, but the file has changed since.
    migrate_plain(&cipher, &path, &plain_payload("old"), "old");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), plain_payload("newer"));
}

/// The vault tier (E1/4): a platform without a cipher keeps the secret in the
/// OS vault and only a marker in the file, and it never pretends otherwise.
mod vault_tier {
    use super::vault::test_vault::MapVault;
    use super::*;

    fn secret_dir() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deepseek.key");
        (dir, path)
    }

    #[test]
    fn save_uses_the_vault_when_there_is_no_cipher() {
        let vault = MapVault::new();
        let (_dir, path) = secret_dir();
        assert!(save_via(None, Some(&vault), &path, " sk-vault \n").expect("save"));
        let payload = std::fs::read_to_string(&path).expect("file");
        assert!(payload.starts_with(vault::KEYRING_PREFIX), "{payload}");
        assert!(!payload.contains("sk-vault"), "the file carries no secret");
        assert_eq!(storage_kind(&path), "keyring");
        assert_eq!(load_via(None, Some(&vault), &path).expect("load"), "sk-vault");
    }

    #[test]
    fn save_falls_back_to_plain_when_the_vault_refuses() {
        let vault = MapVault::failing();
        let (_dir, path) = secret_dir();
        assert!(!save_via(None, Some(&vault), &path, "sk-abc").expect("save"));
        assert_eq!(storage_kind(&path), "plain");
        assert_eq!(load_via(None, Some(&vault), &path).expect("load"), "sk-abc");
    }

    #[test]
    fn load_migrates_a_plain_value_into_the_vault() {
        let vault = MapVault::new();
        let (_dir, path) = secret_file(&plain_payload("old-key"));
        assert_eq!(load_via(None, Some(&vault), &path).expect("load"), "old-key");
        assert_eq!(storage_kind(&path), "keyring");
        assert_eq!(load_via(None, Some(&vault), &path).expect("load"), "old-key");
    }

    #[test]
    fn load_keeps_the_plain_value_when_the_vault_does_not_answer() {
        let vault = MapVault::failing();
        let payload = plain_payload("old-key");
        let (_dir, path) = secret_file(&payload);
        assert_eq!(load_via(None, Some(&vault), &path).expect("load"), "old-key");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), payload);
    }

    #[test]
    fn a_marker_without_a_vault_is_reported_instead_of_read_as_empty() {
        let vault = MapVault::new();
        let (_dir, path) = secret_dir();
        save_via(None, Some(&vault), &path, "sk-vault").expect("save");
        let message = load_via(None, None, &path).expect_err("no vault").to_string();
        assert!(message.contains("not available on this platform"), "{message}");
    }

    #[test]
    fn delete_removes_the_vault_entry_and_the_file() {
        let vault = MapVault::new();
        let (_dir, path) = secret_dir();
        save_via(None, Some(&vault), &path, "sk-vault").expect("save");
        delete_via(Some(&vault), &path).expect("delete");
        assert!(!path.exists(), "the marker file is gone");
        assert!(load_via(None, Some(&vault), &path).expect("load").is_empty(), "nothing is left to read");
    }

    #[test]
    fn a_plain_file_is_restricted_when_it_is_read() {
        let vault = MapVault::failing();
        let (_dir, path) = secret_file(&plain_payload("old-key"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("chmod");
        }
        assert_eq!(load_via(None, Some(&vault), &path).expect("load"), "old-key");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).expect("metadata").permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "an old plain file is tightened on read");
        }
        assert_eq!(std::fs::read_to_string(&path).expect("read"), plain_payload("old-key"));
    }

    #[test]
    fn a_plain_fallback_file_is_created_owner_only() {
        let vault = MapVault::failing();
        let (_dir, path) = secret_dir();
        assert!(!save_via(None, Some(&vault), &path, "sk-abc").expect("save"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).expect("metadata").permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "the fallback must not be world readable");
        }
        assert_eq!(load_via(None, Some(&vault), &path).expect("load"), "sk-abc");
    }
}

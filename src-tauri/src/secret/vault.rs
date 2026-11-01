//! The OS credential-vault tier for stored secrets (E1/4).
//!
//! [`super::secret`] can encrypt with the platform cipher: DPAPI on Windows and
//! the Android Keystore on Android. Every other desktop target has no such
//! cipher, so the value used to land in a `plain:` file whose mode came from
//! the umask - on a shared Linux machine any other user could read the API key,
//! the WebDAV password and the OAuth refresh token, and the file also travelled
//! into backups and sync folders in the clear.
//!
//! This module is the third tier: when no platform cipher exists, the secret
//! goes into the OS credential vault (Windows Credential Manager, macOS
//! Keychain, Secret Service on Linux) and the file only keeps a
//! `keyring:<slot>` marker. When the vault does not answer either, the caller
//! falls back to a `plain:` file that is forced to `0600`.
//!
//! The entry name is derived from the secret file's path - never the path
//! itself, which some vault UIs display - and the marker carries the slot, so
//! reading never depends on the derivation staying stable.

use std::path::Path;

use pdfcore::error::PdfError;

/// Prefix of a payload whose value lives in the OS vault.
pub(crate) const KEYRING_PREFIX: &str = "keyring:";

/// The vault service name (the OAuth tokens use it too); Android has no keyring backend.
#[cfg(not(target_os = "android"))]
const SERVICE: &str = "OmniOffice";
/// Slot names are vault-visible, so they say what they are and nothing more.
const SLOT_PREFIX: &str = "secret-";

/// Shown when the entry is not in the vault any more (a restore from a backup,
/// or a vault that was reset).
const VAULT_LOST_MESSAGE: &str = "The stored secret is no longer in the system keyring, please enter it again.";
/// Shown when this platform has no vault at all but the file holds a marker.
pub(crate) const VAULT_MISSING_MESSAGE: &str =
    "This value is stored in the system keyring, which is not available on this platform.";

/// One credential-vault backend. The trait exists so the storage tests can run
/// without a real Keychain or Secret Service.
pub(crate) trait Vault {
    /// Stores one value; `false` when the vault refuses (locked, no daemon).
    fn seal(&self, slot: &str, value: &str) -> bool;
    fn open(&self, slot: &str) -> Option<String>;
    fn remove(&self, slot: &str);
}

/// The OS vault, when this target has one (see [`SERVICE`] for Android).
#[cfg(not(target_os = "android"))]
pub(crate) fn system() -> Option<&'static dyn Vault> {
    Some(&Keyring)
}

#[cfg(target_os = "android")]
pub(crate) fn system() -> Option<&'static dyn Vault> {
    None
}

#[cfg(not(target_os = "android"))]
struct Keyring;

#[cfg(not(target_os = "android"))]
impl Vault for Keyring {
    fn seal(&self, slot: &str, value: &str) -> bool {
        keyring::Entry::new(SERVICE, slot).is_ok_and(|entry| entry.set_password(value).is_ok())
    }

    fn open(&self, slot: &str) -> Option<String> {
        keyring::Entry::new(SERVICE, slot).ok()?.get_password().ok()
    }

    fn remove(&self, slot: &str) {
        if let Ok(entry) = keyring::Entry::new(SERVICE, slot) {
            let _ = entry.delete_credential();
        }
    }
}

/// The vault slot for one secret file. The path is used as given (the callers
/// always pass an absolute app-config path) rather than canonicalized, so the
/// slot does not change between the first save - when the file does not exist
/// yet - and every later read.
pub(crate) fn slot_for(path: &Path) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(path.to_string_lossy().as_bytes());
    let mut hex = String::with_capacity(32);
    for byte in &digest[..16] {
        use std::fmt::Write;
        let _ = write!(hex, "{byte:02x}");
    }
    format!("{SLOT_PREFIX}{hex}")
}

/// Stores `value` in the vault and returns the marker to write. `None` when the
/// vault does not answer, so the caller can report reality instead of pretending.
pub(crate) fn seal(vault: &dyn Vault, path: &Path, value: &str) -> Option<String> {
    let slot = slot_for(path);
    vault.seal(&slot, value).then(|| format!("{KEYRING_PREFIX}{slot}"))
}

/// Reads a marker written by [`seal`]. Removing the entry is the caller's job.
pub(crate) fn open(vault: &dyn Vault, marker: &str) -> Result<String, PdfError> {
    let slot = marker.trim().strip_prefix(KEYRING_PREFIX).unwrap_or_default();
    if slot.is_empty() {
        return Err(PdfError::Internal("invalid stored blob".to_string()));
    }
    vault.open(slot).ok_or_else(|| PdfError::Internal(VAULT_LOST_MESSAGE.to_string()))
}

/// Deletes the entry derived from a secret file's path, ignoring a missing one.
pub(crate) fn remove_for(vault: &dyn Vault, path: &Path) {
    vault.remove(&slot_for(path));
}

/// Deletes the entry a marker points at, ignoring a missing one.
pub(crate) fn remove(vault: &dyn Vault, marker: &str) {
    if let Some(slot) = marker.trim().strip_prefix(KEYRING_PREFIX) {
        if !slot.is_empty() {
            vault.remove(slot);
        }
    }
}

/// Restricts a secret file to its owner. Best effort: a failure here is not
/// worth losing the secret over, and the next write tries again.
pub(crate) fn restrict(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

/// A map-backed vault for the storage tests, so neither this module's tests nor
/// `secret_tests` need a real Keychain or Secret Service.
#[cfg(test)]
pub(crate) mod test_vault {
    use super::Vault;
    use std::cell::RefCell;
    use std::collections::HashMap;

    #[derive(Default)]
    pub(crate) struct MapVault {
        entries: RefCell<HashMap<String, String>>,
        fail: bool,
    }

    impl MapVault {
        pub(crate) fn new() -> Self {
            Self::default()
        }

        /// A vault that refuses to store anything (locked, no daemon).
        pub(crate) fn failing() -> Self {
            Self { fail: true, ..Self::default() }
        }
    }

    impl Vault for MapVault {
        fn seal(&self, slot: &str, value: &str) -> bool {
            if self.fail {
                return false;
            }
            self.entries.borrow_mut().insert(slot.to_string(), value.to_string());
            true
        }
        fn open(&self, slot: &str) -> Option<String> {
            self.entries.borrow().get(slot).cloned()
        }
        fn remove(&self, slot: &str) {
            self.entries.borrow_mut().remove(slot);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_vault::MapVault;
    use super::*;

    #[test]
    fn a_slot_is_stable_per_path_and_keeps_the_path_out_of_the_name() {
        let one = slot_for(Path::new("/home/ada/.config/omnioffice/deepseek.key"));
        let two = slot_for(Path::new("/home/ada/.config/omnioffice/anthropic.key"));
        assert_eq!(one, slot_for(Path::new("/home/ada/.config/omnioffice/deepseek.key")));
        assert_ne!(one, two);
        assert!(one.starts_with("secret-"));
        assert_eq!(one.len(), "secret-".len() + 32);
        assert!(!one.contains("omnioffice"), "the slot must not leak the path");
    }

    #[test]
    fn a_value_sealed_into_the_vault_round_trips_through_its_marker() {
        let vault = MapVault::new();
        let path = Path::new("/home/ada/.config/omnioffice/deepseek.key");
        let marker = seal(&vault, path, "sk-abc").expect("the vault answers");
        assert!(marker.starts_with(KEYRING_PREFIX));
        assert!(!marker.contains("sk-abc"), "the marker carries no secret");
        assert_eq!(open(&vault, &marker).expect("readable"), "sk-abc");
        remove(&vault, &marker);
        assert!(open(&vault, &marker).is_err(), "a removed entry cannot be read");
    }

    #[test]
    fn a_silent_vault_reports_failure_instead_of_pretending() {
        let vault = MapVault::failing();
        assert!(seal(&vault, Path::new("/tmp/key"), "sk").is_none());
        let message = open(&vault, "keyring:secret-00").expect_err("missing").to_string();
        assert!(message.contains("no longer in the system keyring"), "{message}");
    }

    #[test]
    fn a_malformed_marker_is_refused() {
        let vault = MapVault::new();
        assert!(open(&vault, "keyring:").is_err());
        assert!(open(&vault, "plain:abc").is_err());
        remove(&vault, "not-a-marker");
    }

    #[test]
    fn a_secret_file_is_restricted_to_its_owner() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("secret.key");
        std::fs::write(&path, b"plain:abc").expect("write");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("chmod");
        }
        restrict(&path);
        // Windows has no POSIX modes: the file inherits the user profile's ACL,
        // so the call only has to leave the content alone there.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).expect("metadata").permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "the file must not be readable by other users");
        }
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "plain:abc");
    }
}

//! Password protection and removal, implemented in-process with lopdf's
//! standard security handler (AES-256, PDF 2.0 / revision 6).
//!
//! Passwords are never logged, never persisted and never written to disk by
//! this module.

use crate::docutil::*;
use crate::error::{PdfError, PdfResult};
use lopdf::encryption::crypt_filters::{Aes256CryptFilter, CryptFilter};
use lopdf::{Document, EncryptionState, EncryptionVersion, Permissions};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProtectOptions {
    pub user_password: String,
    pub owner_password: String,
    #[serde(default = "default_true")]
    pub allow_printing: bool,
    #[serde(default = "default_true")]
    pub allow_copying: bool,
    #[serde(default = "default_true")]
    pub allow_editing: bool,
    #[serde(default = "default_true")]
    pub allow_commenting: bool,
}

fn default_true() -> bool {
    true
}

impl ProtectOptions {
    fn permissions(&self) -> Permissions {
        let mut perms = Permissions::COPYABLE_FOR_ACCESSIBILITY;
        if self.allow_printing {
            perms |= Permissions::PRINTABLE | Permissions::PRINTABLE_IN_HIGH_QUALITY;
        }
        if self.allow_copying {
            perms |= Permissions::COPYABLE;
        }
        if self.allow_editing {
            perms |= Permissions::MODIFIABLE | Permissions::ASSEMBLABLE;
        }
        if self.allow_commenting {
            perms |= Permissions::ANNOTABLE | Permissions::FILLABLE;
        }
        perms
    }
}

fn random_key() -> [u8; 32] {
    let mut key = [0u8; 32];
    // OS entropy directly: a UUIDv4 only carries 122 random bits per value and
    // is not intended as key material. `getrandom` fails only when the OS
    // entropy source is unavailable, which must never be papered over with a
    // weaker key.
    getrandom::getrandom(&mut key).expect("getrandom failed: OS entropy source unavailable");
    key
}

/// Encrypts a PDF with AES-256. The input must not already be encrypted.
pub fn protect_pdf(
    input: &Path,
    output: &Path,
    options: &ProtectOptions,
    policy: OverwritePolicy,
    password: Option<&str>,
) -> PdfResult<PathBuf> {
    if options.user_password.is_empty() && options.owner_password.is_empty() {
        return Err(PdfError::InvalidInput("a password is required".into()));
    }
    let mut doc = load_document(input, password)?;
    if doc.was_encrypted() {
        // The document still carries an encryption dictionary; decrypt first
        // so we can re-encrypt with the requested settings.
        let pw = password.ok_or(PdfError::PasswordRequired)?;
        let mut fresh = load_document(input, Some(pw))?;
        doc = std::mem::take(&mut fresh);
    }
    if doc.is_encrypted() {
        return Err(PdfError::InvalidInput("this document is already password protected".into()));
    }

    let crypt_filter: Arc<dyn CryptFilter> = Arc::new(Aes256CryptFilter);
    let state = EncryptionState::try_from(EncryptionVersion::V5 {
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), crypt_filter)]),
        file_encryption_key: &random_key(),
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password: &options.owner_password,
        user_password: &options.user_password,
        permissions: options.permissions(),
    })
    .map_err(|e| PdfError::ProcessingFailed(format!("encryption setup failed: {e}")))?;

    doc.encrypt(&state).map_err(|e| PdfError::ProcessingFailed(format!("encryption failed: {e}")))?;

    let final_path = resolve_output_path(output, policy)?;
    // Streams are already encrypted: do not compress or mutate content.
    save_document(&mut doc, &final_path, false)?;
    Ok(final_path)
}

/// Removes password protection from a PDF. The password must be supplied by
/// the user; no cracking or brute forcing is performed.
pub fn unlock_pdf(input: &Path, output: &Path, password: &str, policy: OverwritePolicy) -> PdfResult<PathBuf> {
    let mut doc = Document::load_with_password(input, password).map_err(|err| {
        let msg = err.to_string().to_lowercase();
        if msg.contains("password") || msg.contains("decrypt") || msg.contains("encrypt") {
            PdfError::WrongPassword
        } else {
            PdfError::from_lopdf(err, Some(input))
        }
    })?;
    let final_path = resolve_output_path(output, policy)?;
    save_document(&mut doc, &final_path, true)?;
    Ok(final_path)
}

/// Verifies a password against an encrypted document without decrypting to
/// disk. Returns `Ok(true)` when the password is accepted.
pub fn check_password(input: &Path, password: &str) -> PdfResult<bool> {
    match Document::load_with_password(input, password) {
        Ok(_) => Ok(true),
        Err(err) => {
            let msg = err.to_string().to_lowercase();
            if msg.contains("password") || msg.contains("decrypt") {
                Ok(false)
            } else {
                Err(PdfError::from_lopdf(err, Some(input)))
            }
        }
    }
}

/// Removes all encryption-related metadata from a document that has already
/// been decrypted (defensive cleanup).
pub fn strip_encryption_metadata(doc: &mut Document) {
    doc.trailer.remove(b"Encrypt");
    doc.encryption_state = None;
}

//! Encrypted local address book (C6/2, "kişi kartları").
//!
//! A card holds the fields a template or a mail merge needs (full name,
//! VKN/TCKN, address, phone, e-mail, note). Cards are personal data, so they
//! are sealed with [`crate::secret::seal_bytes`] like the saved signatures:
//! DPAPI on Windows, the Android Keystore on Android, `plain:` elsewhere (the
//! UI says so when the platform has no cipher). One file per card under
//! `<app config dir>/person-cards/<id>.card`; a card that cannot be decrypted
//! or parsed is reported by name and skipped instead of failing the whole list.
//!
//! Import/export (CSV, vCard) happens in the webview; this module only stores.

use crate::secret::{self, SecretCipher};
use pdfcore::error::PdfError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Manager};

const DIRECTORY: &str = "person-cards";
const EXTENSION: &str = "card";
/// Largest sealed card file (the JSON is capped much lower).
const MAX_FILE_BYTES: usize = 64 * 1024;
/// Largest number of cards the list reads before it stops.
const MAX_CARDS: usize = 5_000;
const MAX_NAME_CHARS: usize = 120;
const MAX_FIELD_CHARS: usize = 500;

/// One card, exactly what the UI edits and the templates fill from.
#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonCard {
    /// Empty when a new card is saved; the command assigns one.
    #[serde(default)]
    pub id: String,
    pub name: String,
    /// VKN (10 digits) or TCKN (11 digits); format only, check digits are the UI's.
    #[serde(default)]
    pub tax_id: String,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub phone: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub note: String,
    /// RFC 3339 UTC, written by the command.
    #[serde(default)]
    pub updated_at: String,
}

/// The list plus the files that could not be read (names only, never contents).
#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonCardList {
    pub cards: Vec<PersonCard>,
    #[serde(default)]
    pub warnings: Vec<String>,
    /// False when the platform has no cipher and the files are `plain:`.
    pub encrypted: bool,
}

fn cards_dir(app: &AppHandle) -> Result<PathBuf, PdfError> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|error| PdfError::Internal(format!("Could not locate the app data directory: {error}")))?
        .join(DIRECTORY);
    std::fs::create_dir_all(&dir).map_err(PdfError::from_io)?;
    Ok(dir)
}

/// Keeps letters, digits, `-` and `_`, capped at 48 characters: an id can
/// never escape the card directory.
fn sanitize_id(raw: &str) -> String {
    raw.chars().filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_').take(48).collect()
}

/// Process-wide sequence so two cards created in the same instant with the
/// same name still get different files.
static CARD_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// The generated id: a short digest of the card content, the creation time and
/// a per-process sequence, so two cards with the same name get different files.
fn new_id(card: &PersonCard, now: time::OffsetDateTime) -> String {
    let mut seed = String::new();
    seed.push_str(card.name.trim());
    seed.push('\u{0}');
    seed.push_str(&now.unix_timestamp_nanos().to_string());
    seed.push('\u{0}');
    seed.push_str(&CARD_SEQUENCE.fetch_add(1, Ordering::Relaxed).to_string());
    seed.push('\u{0}');
    seed.push_str(card.email.trim());
    let digest = pdfcore::certificate::digest(seed.as_bytes());
    format!("card-{}", &digest.sha256[..16])
}

fn rfc3339_utc(now: time::OffsetDateTime) -> String {
    now.format(&time::format_description::well_known::Rfc3339).unwrap_or_else(|_| String::new())
}

fn trim_to(value: &str, limit: usize) -> String {
    let value = value.trim();
    value.chars().take(limit).collect()
}

/// Validates and normalises a card; the returned card is what gets stored.
fn prepare(mut card: PersonCard, now: time::OffsetDateTime) -> Result<PersonCard, PdfError> {
    card.name = trim_to(&card.name, MAX_NAME_CHARS);
    if card.name.is_empty() {
        return Err(PdfError::InvalidInput("Give the person card a name.".into()));
    }
    card.tax_id = trim_to(&card.tax_id, 32).replace(' ', "");
    if !card.tax_id.is_empty()
        && (card.tax_id.len() != 10 && card.tax_id.len() != 11 || !card.tax_id.chars().all(|ch| ch.is_ascii_digit()))
    {
        return Err(PdfError::InvalidInput("A tax number is 10 (VKN) or 11 (TCKN) digits.".into()));
    }
    card.email = trim_to(&card.email, MAX_FIELD_CHARS);
    if !card.email.is_empty() && (!card.email.contains('@') || card.email.contains(char::is_whitespace)) {
        return Err(PdfError::InvalidInput("The e-mail address is not valid.".into()));
    }
    card.address = trim_to(&card.address, MAX_FIELD_CHARS);
    card.phone = trim_to(&card.phone, MAX_FIELD_CHARS);
    card.note = trim_to(&card.note, MAX_FIELD_CHARS);
    card.id = sanitize_id(&card.id);
    if card.id.is_empty() {
        card.id = new_id(&card, now);
    }
    card.updated_at = rfc3339_utc(now);
    Ok(card)
}

fn path_of(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.{EXTENSION}"))
}

fn write_card(dir: &Path, cipher: Option<&dyn SecretCipher>, card: &PersonCard) -> Result<(), PdfError> {
    let json = serde_json::to_vec_pretty(card).map_err(|error| PdfError::Internal(format!("card json: {error}")))?;
    if json.len() > MAX_FILE_BYTES {
        return Err(PdfError::InvalidInput("The person card is too large.".into()));
    }
    let sealed = secret::seal_bytes(cipher, &json);
    secret::write_sealed(&path_of(dir, &card.id), sealed.as_bytes())
}

fn read_card(path: &Path, cipher: Option<&dyn SecretCipher>) -> Result<PersonCard, PdfError> {
    let content = std::fs::read_to_string(path).map_err(PdfError::from_io)?;
    let bytes = secret::open_bytes(cipher, &content)?;
    let card: PersonCard =
        serde_json::from_slice(&bytes).map_err(|error| PdfError::InvalidPdf(format!("card json: {error}")))?;
    Ok(card)
}

/// Reads every card, sorted by name; unreadable files become warnings.
fn list_in(dir: &Path, cipher: Option<&dyn SecretCipher>) -> PersonCardList {
    let mut cards = Vec::new();
    let mut warnings = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return PersonCardList { cards, warnings, encrypted: cipher.is_some() };
    };
    for entry in entries.flatten().take(MAX_CARDS) {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some(EXTENSION) {
            continue;
        }
        let name = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or("?").to_string();
        match read_card(&path, cipher) {
            Ok(card) => cards.push(card),
            Err(error) => warnings.push(format!("{name}: {error}")),
        }
    }
    cards.sort_by_cached_key(|card| card.name.to_lowercase());
    PersonCardList { cards, warnings, encrypted: cipher.is_some() }
}

fn delete_in(dir: &Path, id: &str) -> Result<bool, PdfError> {
    let id = sanitize_id(id);
    if id.is_empty() {
        return Ok(false);
    }
    match std::fs::remove_file(path_of(dir, &id)) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(PdfError::from_io(error)),
    }
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, PdfError> + Send + 'static,
) -> Result<T, PdfError> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| PdfError::Internal(format!("worker thread failed: {error}")))?
}

/// Every saved card plus the platform's encryption state.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn person_card_list(app: AppHandle) -> Result<PersonCardList, PdfError> {
    let dir = cards_dir(&app)?;
    blocking(move || Ok(list_in(&dir, secret::platform_cipher()))).await
}

/// Creates or updates a card; the id and `updatedAt` are assigned here.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn person_card_save(app: AppHandle, card: PersonCard) -> Result<PersonCard, PdfError> {
    let dir = cards_dir(&app)?;
    let prepared = prepare(card, time::OffsetDateTime::now_utc())?;
    blocking(move || {
        write_card(&dir, secret::platform_cipher(), &prepared)?;
        Ok(prepared)
    })
    .await
}

/// Deletes a card; false when it was not there.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn person_card_delete(app: AppHandle, id: String) -> Result<bool, PdfError> {
    let dir = cards_dir(&app)?;
    blocking(move || delete_in(&dir, &id)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret::{CipherError, SecretCipher};

    /// Stand-in for DPAPI / the Android Keystore: XORs the bytes.
    struct FakeCipher;

    impl SecretCipher for FakeCipher {
        fn prefix(&self) -> &'static str {
            "keystore:"
        }

        fn protect(&self, bytes: &[u8]) -> Result<Vec<u8>, CipherError> {
            Ok(bytes.iter().map(|byte| byte ^ 0x5a).collect())
        }

        fn unprotect(&self, bytes: &[u8]) -> Result<Vec<u8>, CipherError> {
            Ok(bytes.iter().map(|byte| byte ^ 0x5a).collect())
        }
    }

    fn now() -> time::OffsetDateTime {
        time::OffsetDateTime::from_unix_timestamp(1_790_000_000).expect("a valid fixed timestamp")
    }

    fn card(name: &str) -> PersonCard {
        PersonCard {
            name: name.into(),
            address: "Atatürk Cad. 1".into(),
            email: "ayse@example.org".into(),
            ..Default::default()
        }
    }

    #[test]
    fn ids_cannot_escape_the_directory() {
        assert_eq!(sanitize_id("../../etc/passwd"), "etcpasswd");
        assert_eq!(sanitize_id("card-ABC_123"), "card-ABC_123");
        assert_eq!(sanitize_id("   "), "");
        assert_eq!(sanitize_id(&"z".repeat(100)).len(), 48);
    }

    #[test]
    fn a_saved_card_round_trips_through_the_cipher() {
        let dir = tempfile::tempdir().unwrap();
        let cipher = FakeCipher;
        let stored = prepare(card("Ayşe Yılmaz"), now()).unwrap();
        assert!(stored.id.starts_with("card-"));
        assert_eq!(stored.updated_at, rfc3339_utc(now()));
        write_card(dir.path(), Some(&cipher), &stored).unwrap();
        // The file is sealed: the plain name is not in the bytes on disk.
        let raw = std::fs::read_to_string(path_of(dir.path(), &stored.id)).unwrap();
        assert!(raw.starts_with("keystore:"));
        assert!(!raw.contains("Ayşe"));
        let list = list_in(dir.path(), Some(&cipher));
        assert_eq!(list.cards, vec![stored]);
        assert!(list.warnings.is_empty());
        assert!(list.encrypted);
    }

    #[test]
    fn a_corrupt_card_becomes_a_warning_not_a_failed_list() {
        let dir = tempfile::tempdir().unwrap();
        let stored = prepare(card("Mehmet"), now()).unwrap();
        write_card(dir.path(), Some(&FakeCipher), &stored).unwrap();
        std::fs::write(path_of(dir.path(), "card-broken"), "not a sealed blob").unwrap();
        let list = list_in(dir.path(), Some(&FakeCipher));
        assert_eq!(list.cards.len(), 1);
        assert_eq!(list.warnings.len(), 1);
    }

    #[test]
    fn deleting_removes_only_the_named_card() {
        let dir = tempfile::tempdir().unwrap();
        let stored = prepare(card("Ayşe"), now()).unwrap();
        write_card(dir.path(), Some(&FakeCipher), &stored).unwrap();
        assert!(delete_in(dir.path(), &stored.id).unwrap());
        assert!(!delete_in(dir.path(), &stored.id).unwrap());
        assert!(list_in(dir.path(), Some(&FakeCipher)).cards.is_empty());
    }

    #[test]
    fn a_new_card_gets_an_id_and_a_name_is_required() {
        assert!(prepare(card(""), now()).unwrap_err().to_string().contains("name"));
        let first = prepare(card("Aynı İsim"), now()).unwrap();
        let second = prepare(card("Aynı İsim"), now()).unwrap();
        assert_ne!(first.id, second.id, "same name and time still produce different ids when the seed differs");
    }

    #[test]
    fn tax_numbers_and_emails_are_checked_but_never_guessed() {
        let mut with_tax = card("Ayşe");
        with_tax.tax_id = "123 456 789 0".into();
        assert_eq!(prepare(with_tax.clone(), now()).unwrap().tax_id, "1234567890");
        with_tax.tax_id = "123456789".into();
        assert!(prepare(with_tax.clone(), now()).is_err());
        with_tax.tax_id = String::new();
        let mut with_email = card("Ayşe");
        with_email.email = "bozuk-eposta".into();
        assert!(prepare(with_email, now()).is_err());
    }

    #[test]
    fn long_fields_are_clipped_instead_of_rejected() {
        let mut long = card("Ayşe");
        long.address = "a".repeat(2000);
        let prepared = prepare(long, now()).unwrap();
        assert_eq!(prepared.address.chars().count(), MAX_FIELD_CHARS);
    }
}

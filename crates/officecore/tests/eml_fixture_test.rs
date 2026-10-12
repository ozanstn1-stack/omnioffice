//! Fixture-based test for the KEP `.eml` reader.
//!
//! `tests/fixtures/kep-ornek.eml` is a hand-authored MIME message (an RFC 2047
//! subject, a `multipart/alternative` body and a base64 PDF attachment), so the
//! parser is exercised against a real file rather than only an inline string.

use officecore::eml::{parse_eml, read_eml_file};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/kep-ornek.eml");

#[test]
fn opens_the_kep_fixture() {
    let bytes = std::fs::read(FIXTURE).expect("the fixture is committed");
    let message = parse_eml(&bytes).expect("the fixture parses");
    assert_eq!(message.subject, "Tebligat örneği");
    assert!(message.from.contains("KEP Gönderici"), "from: {}", message.from);
    assert_eq!(message.to, "avukat@example.org");
    assert_eq!(message.cc, "muvekkil@example.org");
    assert_eq!(message.message_id, "<abc123@kep.example.org>");
    assert!(message.body_text.contains("Tebligat ekte gönderilmiştir"));
    assert!(message.body_html.contains("Sayın Avukat"));
    assert_eq!(message.attachments.len(), 1);
    assert_eq!(message.attachments[0].name, "tebligat.pdf");
    assert!(message.attachments[0].size > 0);
    assert!(message.warnings.is_empty(), "warnings: {:?}", message.warnings);
}

#[test]
fn extracts_the_attachment_to_disk() {
    let directory = tempfile::tempdir().expect("tempdir");
    let message = read_eml_file(std::path::Path::new(FIXTURE), directory.path()).expect("read the fixture");
    let attachment = &message.attachments[0];
    assert!(!attachment.path.is_empty(), "the attachment must be written to disk");
    let bytes = std::fs::read(&attachment.path).expect("read the extracted attachment");
    assert!(bytes.starts_with(b"%PDF"), "the extracted attachment is the PDF");
    assert!(attachment.path.contains("tebligat.pdf"));
}

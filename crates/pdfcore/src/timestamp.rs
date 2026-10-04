//! RFC 3161 timestamp requests and responses.
//!
//! The signing flow is split so the network stays out of `pdfcore`: this
//! module builds the `TimeStampReq` DER and parses the `TimeStampResp` /
//! timestamp token, while the application layer performs the HTTP exchange
//! (and only over HTTPS). A granted timestamp token is embedded by `sign.rs`
//! as the `id-aa-timeStampToken` unsigned attribute of the SignerInfo.
//!
//! Verification reads the same token back out and reports its `genTime`; it
//! never claims the TSA itself is trusted - that needs the TSA certificate
//! chain and, online, its revocation status.

use crate::error::{PdfError, PdfResult};
use crate::sign::{der_integer_u64, der_null, der_octet_string, der_oid, der_sequence, der_tlv};
use const_oid::ObjectIdentifier;
use sha2::{Digest, Sha256};

/// id-aa-timeStampToken, the CMS unsigned attribute that carries the token.
pub const OID_ATTR_TIMESTAMP_TOKEN: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.16.2.14");
/// id-ct-TSTInfo, the eContentType of the token's SignedData.
pub const OID_TST_INFO: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.16.1.4");
const OID_SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.1");

/// Builds a `TimeStampReq` for a signature value: SHA-256 message imprint,
/// `certReq` set so the response carries the TSA certificate, plus a random
/// 16-byte nonce.
pub fn build_request(signature: &[u8]) -> Vec<u8> {
    let digest = Sha256::digest(signature);
    let algorithm = der_sequence(&[der_oid(OID_SHA256), der_null()]);
    let message_imprint = der_sequence(&[algorithm, der_octet_string(&digest)]);
    let mut nonce_bytes = [0u8; 16];
    let _ = getrandom::getrandom(&mut nonce_bytes);
    // A positive INTEGER: clear the top bit so it is not read as negative.
    nonce_bytes[0] &= 0x7F;
    let nonce = der_tlv(0x02, &nonce_bytes);
    // certReq BOOLEAN TRUE (default is FALSE and omitting it is ambiguous).
    let cert_req = der_tlv(0x01, &[0xFF]);
    der_sequence(&[der_integer_u64(1), message_imprint, nonce, cert_req])
}

/// A granted timestamp token plus what it says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimestampToken {
    /// The DER `ContentInfo` of the timestamp token, ready to embed.
    pub token: Vec<u8>,
    /// `genTime` from the TSTInfo, if the structure could be read.
    pub time: Option<String>,
}

/// Minimal DER reader for the response walk.
struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.offset)
    }

    /// Reads one TLV and returns `(tag, content, full bytes including header)`.
    fn read(&mut self) -> PdfResult<(u8, &'a [u8], &'a [u8])> {
        let start = self.offset;
        if self.remaining() < 2 {
            return Err(PdfError::InvalidInput("truncated DER".into()));
        }
        let tag = self.bytes[self.offset];
        self.offset += 1;
        let first = self.bytes[self.offset];
        self.offset += 1;
        let length = if first & 0x80 == 0 {
            first as usize
        } else {
            let count = (first & 0x7F) as usize;
            if count == 0 || count > 4 || self.remaining() < count {
                return Err(PdfError::InvalidInput("unsupported DER length".into()));
            }
            let mut value = 0usize;
            for _ in 0..count {
                value = (value << 8) | self.bytes[self.offset] as usize;
                self.offset += 1;
            }
            value
        };
        if self.remaining() < length {
            return Err(PdfError::InvalidInput("truncated DER value".into()));
        }
        let content = &self.bytes[self.offset..self.offset + length];
        self.offset += length;
        Ok((tag, content, &self.bytes[start..self.offset]))
    }
}

/// Parses a `TimeStampResp`. Returns `Ok(None)` when the TSA refused the
/// request (the status is not `granted`), with the status as the error text.
pub fn parse_response(response: &[u8]) -> PdfResult<Option<TimestampToken>> {
    let mut outer = Reader::new(response);
    let (tag, content, _) = outer.read()?;
    if tag != 0x30 {
        return Err(PdfError::InvalidInput("the TSA response is not a SEQUENCE".into()));
    }
    let mut body = Reader::new(content);
    let (status_tag, status_content, _) = body.read()?;
    if status_tag != 0x30 {
        return Err(PdfError::InvalidInput("the TSA response has no PKIStatusInfo".into()));
    }
    let mut status_reader = Reader::new(status_content);
    let (integer_tag, integer_content, _) = status_reader.read()?;
    if integer_tag != 0x02 {
        return Err(PdfError::InvalidInput("the TSA status is not an INTEGER".into()));
    }
    let status = integer_content.iter().fold(0u32, |value, byte| (value << 8) | u32::from(*byte));
    // 0 granted, 1 grantedWithMods; 2 rejection, 3 waiting, 4 revocationWarning.
    if status > 1 {
        return Err(PdfError::ProcessingFailed(format!(
            "the timestamp authority refused the request (status {status})"
        )));
    }
    // timeStampToken is the next element when granted.
    let (token_tag, token_content, token_all) = body.read()?;
    if token_tag != 0x30 {
        return Err(PdfError::InvalidInput("the granted response carries no timestamp token".into()));
    }
    let token = token_all.to_vec();
    let time = parse_tst_info_time(token_content);
    Ok(Some(TimestampToken { token, time }))
}

/// Extracts `genTime` from a timestamp token `ContentInfo` (the full TLV).
pub fn parse_token_time(token: &[u8]) -> Option<String> {
    let mut outer = Reader::new(token);
    let (tag, content, _) = outer.read().ok()?;
    if tag != 0x30 {
        return None;
    }
    parse_tst_info_time(content)
}

/// `ContentInfo` -> `[0] SignedData` -> `encapContentInfo` -> `[0] OCTET
/// STRING` -> `TSTInfo` -> `genTime`.
fn parse_tst_info_time(content_info_content: &[u8]) -> Option<String> {
    let mut reader = Reader::new(content_info_content);
    // contentType OID
    let (oid_tag, _, _) = reader.read().ok()?;
    if oid_tag != 0x06 {
        return None;
    }
    // [0] EXPLICIT SignedData
    let (context_tag, signed_data_content, _) = reader.read().ok()?;
    if context_tag != 0xA0 {
        return None;
    }
    let mut signed_data = Reader::new(signed_data_content);
    let (sequence_tag, signed_data_body, _) = signed_data.read().ok()?;
    if sequence_tag != 0x30 {
        return None;
    }
    let mut fields = Reader::new(signed_data_body);
    // version
    let (version_tag, _, _) = fields.read().ok()?;
    if version_tag != 0x02 {
        return None;
    }
    // digestAlgorithms SET
    fields.read().ok()?;
    // encapContentInfo SEQUENCE
    let (encap_tag, encap_content, _) = fields.read().ok()?;
    if encap_tag != 0x30 {
        return None;
    }
    let mut encap = Reader::new(encap_content);
    let (content_type_tag, _, _) = encap.read().ok()?;
    if content_type_tag != 0x06 {
        return None;
    }
    let (explicit_tag, octet_all, _) = encap.read().ok()?;
    if explicit_tag != 0xA0 {
        return None;
    }
    // [0] holds an OCTET STRING containing the TSTInfo.
    let mut wrapper = Reader::new(octet_all);
    let (octet_tag, octet_content, _) = wrapper.read().ok()?;
    if octet_tag != 0x04 {
        return None;
    }
    let mut tst = Reader::new(octet_content);
    let (tst_tag, tst_content, _) = tst.read().ok()?;
    if tst_tag != 0x30 {
        return None;
    }
    let mut fields = Reader::new(tst_content);
    // version INTEGER, policy OID, messageImprint SEQUENCE, serialNumber INTEGER
    fields.read().ok()?;
    fields.read().ok()?;
    fields.read().ok()?;
    let (serial_tag, _, _) = fields.read().ok()?;
    if serial_tag != 0x02 {
        return None;
    }
    // genTime GeneralizedTime (tag 0x18)
    let (time_tag, time_content, _) = fields.read().ok()?;
    if time_tag != 0x18 {
        return None;
    }
    Some(String::from_utf8_lossy(time_content).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sign::{der_context, der_sequence};

    /// A minimal but structurally valid timestamp token whose TSTInfo carries
    /// `gen_time`. Built with the same DER writer the signer uses.
    fn test_token(gen_time: &str) -> Vec<u8> {
        let tst_info = der_sequence(&[
            der_integer_u64(1),
            der_oid(ObjectIdentifier::new_unwrap("1.2.3.4.5")),
            der_sequence(&[der_sequence(&[der_oid(OID_SHA256), der_null()]), der_octet_string(&[0u8; 32])]),
            der_integer_u64(42),
            der_tlv(0x18, gen_time.as_bytes()),
        ]);
        let encap = der_sequence(&[der_oid(OID_TST_INFO), der_context(0, &der_octet_string(&tst_info))]);
        let signed_data = der_sequence(&[der_integer_u64(1), der_sequence(&[]), encap]);
        // ContentInfo ::= SEQUENCE { contentType OID, [0] EXPLICIT SignedData }
        der_sequence(&[der_oid(ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.2")), der_context(0, &signed_data)])
    }

    #[test]
    fn the_request_is_a_well_formed_timestamp_query() {
        let request = build_request(b"signature-bytes");
        assert_eq!(request[0], 0x30, "TimeStampReq is a SEQUENCE");
        let mut reader = Reader::new(&request[2..]);
        // version INTEGER 1
        let (tag, content, _) = reader.read().unwrap();
        assert_eq!(tag, 0x02);
        assert_eq!(content, &[1]);
        // messageImprint
        let (tag, _, _) = reader.read().unwrap();
        assert_eq!(tag, 0x30);
        // nonce INTEGER and certReq BOOLEAN are present
        let (tag, _, _) = reader.read().unwrap();
        assert_eq!(tag, 0x02);
        let (tag, content, _) = reader.read().unwrap();
        assert_eq!(tag, 0x01);
        assert_eq!(content, &[0xFF]);
        // SHA-256 of the signature is inside the request.
        let digest = Sha256::digest(b"signature-bytes");
        assert!(request.windows(digest.len()).any(|window| window == digest.as_slice()));
    }

    #[test]
    fn a_granted_response_exposes_the_token_and_time() {
        let token = test_token("20260102120000Z");
        let status = der_sequence(&[der_integer_u64(0)]);
        let response = der_sequence(&[status, token.clone()]);
        let parsed = parse_response(&response).expect("parse").expect("granted");
        assert_eq!(parsed.token, token);
        assert_eq!(parsed.time.as_deref(), Some("20260102120000Z"));
        assert_eq!(parse_token_time(&token).as_deref(), Some("20260102120000Z"));
    }

    #[test]
    fn a_refused_response_is_reported_not_ignored() {
        let response = der_sequence(&[der_sequence(&[der_integer_u64(2)])]);
        let error = parse_response(&response).expect_err("refused");
        assert!(format!("{error}").contains("status 2"));
    }

    #[test]
    fn garbage_is_an_error() {
        assert!(parse_response(b"not der").is_err());
        assert_eq!(parse_token_time(b""), None);
    }
}

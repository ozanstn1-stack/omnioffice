//! Tests for the request signature versions: the golden vectors, what each
//! version binds, and the rule that picks one for a peer.

use super::sig::*;
use super::*;

/// A request with every field the two canonical forms look at, so a mutation
/// in the tests below is one field away from a real one.
fn request() -> Request {
    Request {
        op: "get_chunk".into(),
        device_id: "device-7".into(),
        timestamp: 1_760_000_000,
        nonce: "0123456789abcdef0123456789abcdef".into(),
        path: "rapor.docx".into(),
        offset: 524_288,
        body_len: 0,
        total: 1_048_576,
        sha256: "e55d2e7cbb136374c4590cf2c88e903e77d2b199e50b44e17a7be5f0d41c5fd3".into(),
        ..Request::default()
    }
}

fn key() -> String {
    "aa".repeat(32)
}

/// SHA-256 of the empty body, which is what a `get_chunk` request hashes.
const EMPTY_BODY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

#[test]
fn a_version_one_signature_still_matches_its_golden_vector() {
    // The vector is fixed: version 1 is what a paired device on an older build
    // sends, so these bytes must never change.
    assert_eq!(
        canonical_v1(&request(), EMPTY_BODY),
        format!("device-7\n1760000000\n0123456789abcdef0123456789abcdef\nget_chunk\nrapor.docx\n{EMPTY_BODY}")
    );
    let signed = sign(&key(), &request(), EMPTY_BODY, "host-1", SigVersion::V1).expect("sign");
    assert_eq!(signed, "f544fda1926ae7f1504f5fe0ebda57303ccea4e27a1f880564fb723849bcee3f");
    let mut signed_request = request();
    signed_request.mac = signed;
    assert!(verify(&hex_decode(&key()).expect("key"), &signed_request, EMPTY_BODY, "host-1", SigVersion::V1));
}

#[test]
fn a_version_two_signature_matches_its_golden_vector() {
    let canonical = canonical_v2(&request(), EMPTY_BODY, "host-1");
    assert_eq!(canonical.len(), 263, "one length-prefixed field per canonical field");
    assert!(canonical.starts_with(b"omnioffice-lan-sig/v2\0"));
    let signed = sign(&key(), &request(), EMPTY_BODY, "host-1", SigVersion::V2).expect("sign");
    assert_eq!(signed, "b38a06d6065695d0168e2b1baf1cb87db0e9c38563dc12fac6862f59b9fe4956");
    let mut signed_request = request();
    signed_request.mac = signed;
    assert!(verify(&hex_decode(&key()).expect("key"), &signed_request, EMPTY_BODY, "host-1", SigVersion::V2));
}

#[test]
fn the_two_versions_never_produce_the_same_tag() {
    let v1 = sign(&key(), &request(), EMPTY_BODY, "host-1", SigVersion::V1).expect("v1");
    let v2 = sign(&key(), &request(), EMPTY_BODY, "host-1", SigVersion::V2).expect("v2");
    assert_ne!(v1, v2);
    // A tag from one version never verifies as the other: the domain prefix
    // (and the extra fields) keep them apart.
    let key_bytes = hex_decode(&key()).expect("key");
    let mut claimed = request();
    claimed.mac = v1.clone();
    assert!(!verify(&key_bytes, &claimed, EMPTY_BODY, "host-1", SigVersion::V2), "a v1 tag is not a v2 tag");
    claimed.mac = v2;
    assert!(!verify(&key_bytes, &claimed, EMPTY_BODY, "host-1", SigVersion::V1), "a v2 tag is not a v1 tag");
}

#[test]
fn version_two_binds_the_range_a_transfer_asked_for() {
    let base = sign(&key(), &request(), EMPTY_BODY, "host-1", SigVersion::V2).expect("sign");
    let mutations: [(&str, fn(&mut Request)); 4] = [
        ("offset", |request| request.offset += 1),
        ("length", |request| request.body_len += 1),
        ("total", |request| request.total += 1),
        ("sha256", |request| request.sha256 = "0".repeat(63) + "1"),
    ];
    for (field, mutate) in mutations {
        let mut changed = request();
        mutate(&mut changed);
        let signed = sign(&key(), &changed, EMPTY_BODY, "host-1", SigVersion::V2).expect("sign");
        assert_ne!(signed, base, "{field} is not covered by the version 2 signature");
    }
}

#[test]
fn version_two_binds_who_asked_for_what() {
    let base = sign(&key(), &request(), EMPTY_BODY, "host-1", SigVersion::V2).expect("sign");
    let mutations: [(&str, fn(&mut Request)); 5] = [
        ("op", |request| request.op = "begin_get".into()),
        ("sender", |request| request.device_id = "device-8".into()),
        ("path", |request| request.path = "rapor-2.docx".into()),
        ("nonce", |request| request.nonce = "f".repeat(32)),
        ("timestamp", |request| request.timestamp += 1),
    ];
    for (field, mutate) in mutations {
        let mut changed = request();
        mutate(&mut changed);
        let signed = sign(&key(), &changed, EMPTY_BODY, "host-1", SigVersion::V2).expect("sign");
        assert_ne!(signed, base, "{field} is not covered by the version 2 signature");
    }
    // The receiver is part of the message too: two hosts with the same key
    // (a copied store) do not accept each other's requests.
    let elsewhere = sign(&key(), &request(), EMPTY_BODY, "host-2", SigVersion::V2).expect("sign");
    assert_ne!(elsewhere, base);
    let body = sign(&key(), &request(), &"11".repeat(32), "host-1", SigVersion::V2).expect("sign");
    assert_ne!(body, base, "the body hash is covered");
}

#[test]
fn a_length_prefix_stops_two_fields_from_running_together() {
    // The raw concatenation of op and path is the same for both; only the
    // length prefix tells them apart.
    let mut first = request();
    first.op = "ab".into();
    first.path = "c".into();
    let mut second = request();
    second.op = "a".into();
    second.path = "bc".into();
    let (first, second) = (canonical_v2(&first, EMPTY_BODY, "host-1"), canonical_v2(&second, EMPTY_BODY, "host-1"));
    assert_ne!(first, second, "field boundaries must not be ambiguous");
}

#[test]
fn an_unknown_signature_version_is_refused_rather_than_guessed() {
    assert_eq!(SigVersion::from_wire(0), Some(SigVersion::V1), "a build that never wrote the field");
    assert_eq!(SigVersion::from_wire(1), Some(SigVersion::V1));
    assert_eq!(SigVersion::from_wire(2), Some(SigVersion::V2));
    assert_eq!(SigVersion::from_wire(3), None);
    assert_eq!(SigVersion::from_wire(u8::MAX), None);
    assert_eq!(SigVersion::V2.to_wire(), 2);
    assert_eq!(SigVersion::from_wire(SigVersion::to_wire(SigVersion::V1)), Some(SigVersion::V1));
}

#[test]
fn a_request_without_a_known_size_or_hash_still_signs_one_way() {
    let mut unknown = request();
    unknown.total = 0;
    unknown.sha256.clear();
    let first = sign(&key(), &unknown, EMPTY_BODY, "host-1", SigVersion::V2).expect("sign");
    let second = sign(&key(), &unknown.clone(), EMPTY_BODY, "host-1", SigVersion::V2).expect("sign");
    assert_eq!(first, second, "the reserved encoding is deterministic");
    // Zero is "unknown", so the reserved value is what it signs as.
    let mut reserved = request();
    reserved.total = u64::MAX;
    reserved.sha256 = "0".repeat(64);
    assert_eq!(canonical_v2(&reserved, EMPTY_BODY, "host-1"), canonical_v2(&unknown, EMPTY_BODY, "host-1"));
}

#[test]
fn a_version_two_tag_is_not_accepted_under_a_different_transfer() {
    // The transfer id is derived from the path and the announced version, so a
    // signed range cannot be moved to another document that happens to share
    // its size.
    let mut first = request();
    let mut second = request();
    second.path = "diger.docx".into();
    let signed = sign(&key(), &first, EMPTY_BODY, "host-1", SigVersion::V2).expect("sign");
    first.mac = signed;
    assert!(verify(&hex_decode(&key()).expect("key"), &first, EMPTY_BODY, "host-1", SigVersion::V2));
    second.mac = first.mac.clone();
    assert!(!verify(&hex_decode(&key()).expect("key"), &second, EMPTY_BODY, "host-1", SigVersion::V2));
    assert_ne!(transfer_id(&first), transfer_id(&second));
}

#[test]
fn a_peer_negotiates_version_two_only_when_both_ends_said_so() {
    let announced = vec![FEATURE_SIG_V2.to_string()];
    assert_eq!(SigVersion::for_peer(&announced, 0), SigVersion::V2);
    // An older peer announced nothing: version 1 keeps working.
    assert_eq!(SigVersion::for_peer(&[], 0), SigVersion::V1);
    // Once a version 2 request was accepted the pin holds, even if the feature
    // list is lost with an older store file.
    assert_eq!(SigVersion::for_peer(&[], 2), SigVersion::V2);
    // An unknown feature is not a version.
    assert_eq!(SigVersion::for_peer(&["sig-v3".to_string()], 0), SigVersion::V1);
    assert_eq!(SigVersion::for_peer(&[FEATURE_SIG_V2.to_string()], 2), SigVersion::V2);
}

#[test]
fn a_feature_list_is_cleaned_capped_and_sorted() {
    let raw: Vec<String> = vec![
        " sig-v2 ".into(),
        "sig-v2".into(),
        String::new(),
        "bad feature".into(),
        "bad/feature".into(),
        "x".repeat(MAX_FEATURE_LEN + 1),
        "zeta".into(),
        "alpha".into(),
        "b1".into(),
        "b2".into(),
        "b3".into(),
    ];
    let kept = sanitize_features(&raw);
    assert_eq!(kept, vec!["alpha".to_string(), "b1".into(), "b2".into(), "b3".into(), "sig-v2".into(), "zeta".into()]);
    // A peer that announces more than the cap is cut off, not rejected.
    let many: Vec<String> = (0..MAX_FEATURES * 2).map(|index| format!("f{index}")).collect();
    assert_eq!(sanitize_features(&many).len(), MAX_FEATURES);
    assert!(!kept.iter().any(|feature| feature.contains(' ')), "no whitespace survives");
    // The order does not depend on what the peer sent, so both ends agree.
    let mut shuffled = raw.clone();
    shuffled.reverse();
    assert_eq!(sanitize_features(&shuffled), kept);
}

#[test]
fn a_wrong_key_or_a_broken_mac_is_refused_without_detail() {
    let key_bytes = hex_decode(&key()).expect("key");
    let mut signed = request();
    signed.mac = sign(&key(), &signed, EMPTY_BODY, "host-1", SigVersion::V2).expect("sign");
    assert!(verify(&key_bytes, &signed, EMPTY_BODY, "host-1", SigVersion::V2));
    // A different key, a truncated tag, a non-hex tag and no tag at all.
    assert!(!verify(&hex_decode(&"bb".repeat(32)).expect("key"), &signed, EMPTY_BODY, "host-1", SigVersion::V2));
    let mut short = signed.clone();
    short.mac.truncate(32);
    assert!(!verify(&key_bytes, &short, EMPTY_BODY, "host-1", SigVersion::V2));
    let mut odd = signed.clone();
    odd.mac.pop();
    assert!(!verify(&key_bytes, &odd, EMPTY_BODY, "host-1", SigVersion::V2));
    let mut missing = signed.clone();
    missing.mac.clear();
    assert!(!verify(&key_bytes, &missing, EMPTY_BODY, "host-1", SigVersion::V2));
    // A tampered body hash is a tampered body.
    assert!(!verify(&key_bytes, &signed, &"22".repeat(32), "host-1", SigVersion::V2));
}

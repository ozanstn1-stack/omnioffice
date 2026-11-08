//! Tests for the code type: the value and the width that carries it.

use super::*;

#[test]
fn only_one_and_two_bytes_are_codes() {
    assert_eq!(Code::from_bytes(&[]), None);
    assert_eq!(Code::from_bytes(&[0x41]), Some(Code::one(0x41)));
    assert_eq!(Code::from_bytes(&[0x81, 0x40]), Some(Code::two(0x8140)));
    assert_eq!(Code::from_bytes(&[0x01, 0x02, 0x03]), None, "three bytes is not a code");
}

#[test]
fn the_hex_body_follows_the_width() {
    assert_eq!(Code::one(0x41).hex(), "41");
    assert_eq!(Code::two(0x41).hex(), "0041");
    assert_eq!(Code::two(0x8140).hex(), "8140");
    assert_eq!(Code::default().hex(), "0000", "an unused slot keeps writing four digits");
}

#[test]
fn the_value_orders_before_the_width() {
    let mut codes = [Code::two(0x41), Code::one(0x42), Code::one(0x41), Code::two(0x8140)];
    codes.sort();
    assert_eq!(codes, [Code::one(0x41), Code::two(0x41), Code::one(0x42), Code::two(0x8140)]);
}

#[test]
fn an_offset_keeps_the_width_or_gives_up() {
    assert_eq!(Code::two(0x8140).offset(0x5F), Some(Code::two(0x819F)));
    assert_eq!(Code::one(0x40).offset(1), Some(Code::one(0x41)));
    assert_eq!(Code::one(0xFE).offset(2), None, "0x100 does not fit one byte");
    assert_eq!(Code::two(0xFFFF).offset(1), None, "past the two-byte space");
}

#[test]
fn the_bytes_match_the_hex() {
    assert_eq!(Code::one(0x41).to_bytes(), vec![0x41]);
    assert_eq!(Code::two(0x8140).to_bytes(), vec![0x81, 0x40]);
}

//! Unit tests for the binary NUMERIC decoder, on hand-built wire buffers.
//!
//! These cover the pure rendering logic and the malformed-buffer error paths;
//! the differential test against PostgreSQL's own `::text` rendering lives in
//! `super::super::integration_tests` (it needs a live server).
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::{
    NUMERIC_NAN, NUMERIC_NEG, NUMERIC_NINF, NUMERIC_PINF, NUMERIC_POS, decode_numeric_text,
};

/// Build a binary NUMERIC wire buffer from its parts.
fn wire(digits: &[u16], weight: i16, sign: u16, dscale: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + 2 * digits.len());
    out.extend_from_slice(&u16::try_from(digits.len()).unwrap().to_be_bytes());
    out.extend_from_slice(&weight.to_be_bytes());
    out.extend_from_slice(&sign.to_be_bytes());
    out.extend_from_slice(&dscale.to_be_bytes());
    for d in digits {
        out.extend_from_slice(&d.to_be_bytes());
    }
    out
}

fn decode(digits: &[u16], weight: i16, sign: u16, dscale: u16) -> String {
    decode_numeric_text(&wire(digits, weight, sign, dscale)).unwrap()
}

#[test]
fn zero_renders_bare() {
    assert_eq!(decode(&[], 0, NUMERIC_POS, 0), "0");
}

#[test]
fn zero_keeps_its_display_scale() {
    assert_eq!(decode(&[], 0, NUMERIC_POS, 3), "0.000");
}

#[test]
fn integer_and_fraction_groups_render_in_place() {
    // 300.75 → groups [300, 7500], weight 0, dscale 2.
    assert_eq!(decode(&[300, 7500], 0, NUMERIC_POS, 2), "300.75");
}

#[test]
fn interior_groups_pad_to_four_digits() {
    // 12345678.0001 → groups [1234, 5678, 1], weight 1, dscale 4. The middle
    // group must render "5678" and the fraction group must pad to "0001".
    assert_eq!(decode(&[1234, 5678, 1], 1, NUMERIC_POS, 4), "12345678.0001");
}

#[test]
fn missing_trailing_integer_groups_are_implicit_zeros() {
    // 10000 → one stored group [1] at weight 1; group 0 is implicit.
    assert_eq!(decode(&[1], 1, NUMERIC_POS, 0), "10000");
}

#[test]
fn negative_weight_prints_leading_zero_padding() {
    // 0.00001 → one group [1000] at weight -2, dscale 5: an entire implicit
    // zero group sits between the decimal point and the stored group.
    assert_eq!(decode(&[1000], -2, NUMERIC_POS, 5), "0.00001");
}

#[test]
fn negative_sign_prefixes_the_rendering() {
    assert_eq!(decode(&[42], 0, NUMERIC_NEG, 0), "-42");
}

#[test]
fn dscale_truncates_inside_the_final_group() {
    // 1.5 stores its fraction as the group 5000; dscale 1 keeps one digit.
    assert_eq!(decode(&[1, 5000], 0, NUMERIC_POS, 1), "1.5");
}

#[test]
fn dscale_pads_past_the_last_stored_group() {
    // 1.5 with dscale 6 renders the stored group then implicit zeros.
    assert_eq!(decode(&[1, 5000], 0, NUMERIC_POS, 6), "1.500000");
}

#[test]
fn specials_render_as_postgres_prints_them() {
    assert_eq!(decode(&[], 0, NUMERIC_NAN, 0), "NaN");
    assert_eq!(decode(&[], 0, NUMERIC_PINF, 0), "Infinity");
    assert_eq!(decode(&[], 0, NUMERIC_NINF, 0), "-Infinity");
}

#[test]
fn short_buffer_is_an_error_not_a_guess() {
    assert!(decode_numeric_text(&[0, 0, 0]).is_err());
}

#[test]
fn digit_count_mismatch_is_an_error() {
    // Header declares 2 groups, body carries 1.
    let mut buf = wire(&[1234], 0, NUMERIC_POS, 0);
    buf[1] = 2;
    assert!(decode_numeric_text(&buf).is_err());
}

#[test]
fn unrecognised_sign_word_is_an_error() {
    assert!(decode_numeric_text(&wire(&[1], 0, 0x1234, 0)).is_err());
}

#[test]
fn digit_group_beyond_base_10000_is_an_error() {
    assert!(decode_numeric_text(&wire(&[10_000], 0, NUMERIC_POS, 0)).is_err());
}

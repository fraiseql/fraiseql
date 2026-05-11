#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
use super::*;

#[test]
fn test_read_cstr() {
    let mut data = Bytes::from_static(b"hello\0world\0");
    assert_eq!(data.read_cstr().unwrap(), "hello");
    assert_eq!(data.read_cstr().unwrap(), "world");
}

#[test]
fn test_read_i32() {
    let mut data = Bytes::from_static(&[0x00, 0x00, 0x01, 0x00]);
    assert_eq!(data.read_i32_be().unwrap(), 256);
}

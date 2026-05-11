#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
use super::*;

#[test]
fn test_encode_query() {
    let msg = FrontendMessage::Query("SELECT 1".to_string());
    let buf = encode_message(&msg).unwrap();

    assert_eq!(buf[0], b'Q');
    let len = i32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]);
    assert_eq!(len, (buf.len() - 1) as i32);
}

#[test]
fn test_encode_terminate() {
    let msg = FrontendMessage::Terminate;
    let buf = encode_message(&msg).unwrap();

    assert_eq!(buf[0], b'X');
    assert_eq!(buf.len(), 5);
}

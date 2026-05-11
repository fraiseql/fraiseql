use super::*;

#[test]
fn test_constant_time_eq_equal() {
    assert!(constant_time_eq(b"test", b"test"));
    assert!(constant_time_eq(b"", b""));
}

#[test]
fn test_constant_time_eq_not_equal() {
    assert!(!constant_time_eq(b"test", b"fail"));
    assert!(!constant_time_eq(b"test", b"tes"));
    assert!(!constant_time_eq(b"test", b""));
}

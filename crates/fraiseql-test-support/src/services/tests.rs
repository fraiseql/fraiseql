use super::normalize;

#[test]
fn normalize_drops_empty() {
    assert_eq!(normalize(Some(String::new())), None);
}

#[test]
fn normalize_drops_whitespace_only() {
    assert_eq!(normalize(Some("   \t ".to_string())), None);
}

#[test]
fn normalize_keeps_real_value() {
    assert_eq!(
        normalize(Some("postgresql://x".to_string())),
        Some("postgresql://x".to_string())
    );
}

#[test]
fn normalize_passes_through_none() {
    assert_eq!(normalize(None), None);
}

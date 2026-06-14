use super::*;

#[test]
fn test_to_snake_case() {
    assert_eq!(to_snake_case("id"), "id");
    assert_eq!(to_snake_case("firstName"), "first_name");
    assert_eq!(to_snake_case("createdAt"), "created_at");
    assert_eq!(to_snake_case("userId"), "user_id");
    assert_eq!(to_snake_case("updatedAtTimestamp"), "updated_at_timestamp");
    assert_eq!(to_snake_case("ipAddress"), "ip_address");
}

#[test]
fn test_to_snake_case_idempotent() {
    assert_eq!(to_snake_case("ip_address"), "ip_address");
    assert_eq!(to_snake_case("first_name"), "first_name");
    assert_eq!(to_snake_case("id"), "id");
}

#[test]
fn test_to_snake_case_digit_boundaries() {
    // Letter→digit boundary: a digit segment is its own word (the inverse of
    // to_camel_case collapsing `phone_1` → `phone1`).
    assert_eq!(to_snake_case("phone1"), "phone_1");
    assert_eq!(to_snake_case("phone2"), "phone_2");
    assert_eq!(to_snake_case("address2"), "address_2");
    assert_eq!(to_snake_case("line2Content"), "line_2_content");
    // Digit→uppercase-word boundary.
    assert_eq!(to_snake_case("dns1Id"), "dns_1_id");
    assert_eq!(to_snake_case("backup10Id"), "backup_10_id");
    // Consecutive digits stay one word.
    assert_eq!(to_snake_case("phone12"), "phone_12");
    // v1 convention: acronym-looking digit identifiers split (caveat — see docs).
    assert_eq!(to_snake_case("oauth2"), "oauth_2");
    assert_eq!(to_snake_case("ipv4"), "ipv_4");
}

#[test]
fn test_to_snake_case_digit_idempotent() {
    // Already-snake digit fields must round-trip unchanged.
    assert_eq!(to_snake_case("phone_1"), "phone_1");
    assert_eq!(to_snake_case("address_2"), "address_2");
    assert_eq!(to_snake_case("dns_1_id"), "dns_1_id");
}

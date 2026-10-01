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
    // Letter→digit boundary on a NON-acronym: a digit segment is its own word (the
    // inverse of to_camel_case collapsing `phone_1` → `phone1`).
    assert_eq!(to_snake_case("phone1"), "phone_1");
    assert_eq!(to_snake_case("phone2"), "phone_2");
    assert_eq!(to_snake_case("address2"), "address_2");
    assert_eq!(to_snake_case("line2Content"), "line_2_content");
    assert_eq!(to_snake_case("foo3"), "foo_3"); // not registered → splits
    // Digit→uppercase-word boundary.
    assert_eq!(to_snake_case("dns1Id"), "dns_1_id");
    assert_eq!(to_snake_case("backup10Id"), "backup_10_id");
    // Consecutive digits stay one word.
    assert_eq!(to_snake_case("phone12"), "phone_12");
}

#[test]
fn test_to_snake_case_acronyms_stay_whole() {
    // Built-in acronyms (`<word><digit>`) keep their internal digit attached.
    assert_eq!(to_snake_case("s3"), "s3");
    assert_eq!(to_snake_case("ec2"), "ec2");
    assert_eq!(to_snake_case("ipv4"), "ipv4");
    assert_eq!(to_snake_case("ipv6"), "ipv6");
    assert_eq!(to_snake_case("oauth2"), "oauth2");
    assert_eq!(to_snake_case("sha256"), "sha256");
    assert_eq!(to_snake_case("md5"), "md5");
    // An acronym at the start of a compound stays whole; the next word boundary
    // still splits.
    assert_eq!(to_snake_case("s3Bucket"), "s3_bucket");
    // After a camelCase hump the digit is already protected by the uppercase
    // boundary (no acronym lookup needed).
    assert_eq!(to_snake_case("awsS3Bucket"), "aws_s3_bucket");
}

#[test]
fn test_to_snake_case_digit_idempotent() {
    // Already-snake digit fields must round-trip unchanged.
    assert_eq!(to_snake_case("phone_1"), "phone_1");
    assert_eq!(to_snake_case("address_2"), "address_2");
    assert_eq!(to_snake_case("dns_1_id"), "dns_1_id");
    assert_eq!(to_snake_case("s3"), "s3"); // acronym idempotent
}

#[test]
fn test_set_runtime_acronyms_adds_to_defaults() {
    // A unique term not registered anywhere else, so this test's global mutation
    // (OnceLock is process-wide) cannot affect another test's expectations.
    set_runtime_acronyms(&["acmewidget7".to_string()]);
    assert_eq!(to_snake_case("acmewidget7"), "acmewidget7");
    // Built-in defaults still apply alongside the project addition.
    assert_eq!(to_snake_case("s3"), "s3");
    assert_eq!(to_snake_case("ipv4"), "ipv4");
    // A non-registered name still splits.
    assert_eq!(to_snake_case("phone1"), "phone_1");
}

/// #1372: a registered acronym is recognised at any word position, not only the
/// first. The word's candidate starts at the capital that opened it (`Ec2`), not at
/// its first lowercase letter (`c2`), so `emissionEc2Kg` reaches the stored key
/// `emission_ec2_kg`.
#[test]
fn test_to_snake_case_registered_acronym_after_the_first_word() {
    let cases = [
        // After the first word — the defect.
        ("emissionEc2Kg", "emission_ec2_kg"),
        ("emissionEc2", "emission_ec2"),
        ("hostIpv4", "host_ipv4"),
        ("authOauth2Token", "auth_oauth2_token"),
        // First word, camelCase and PascalCase — must keep working.
        ("ec2Emission", "ec2_emission"),
        ("Ec2Emission", "ec2_emission"),
        ("EmissionEc2Kg", "emission_ec2_kg"),
        // Already snake: idempotent.
        ("emission_ec2_kg", "emission_ec2_kg"),
    ];
    for (input, expected) in cases {
        assert_eq!(to_snake_case(input), expected, "input `{input}`");
    }
}

/// #1372, the other half: an unregistered `<word><digit>` still splits at every
/// position, so the fix widens nothing it should not.
#[test]
fn test_to_snake_case_unregistered_word_digit_still_splits_after_the_first_word() {
    let cases = [
        ("emissionCo2Kg", "emission_co_2_kg"),
        ("emissionCo2", "emission_co_2"),
        ("co2Emission", "co_2_emission"),
        ("hostFoo4", "host_foo_4"),
    ];
    for (input, expected) in cases {
        assert_eq!(to_snake_case(input), expected, "input `{input}`");
    }
}

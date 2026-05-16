use super::*;

#[test]
fn mask_password_with_credentials() {
    let url = "postgres://user:password@localhost:5432/db";
    let masked = mask_password(url);
    assert!(masked.contains("***"));
    assert!(!masked.contains("password"));
}

#[test]
fn mask_password_without_credentials() {
    let url = "postgres://localhost:5432/db";
    let masked = mask_password(url);
    assert_eq!(masked, url);
}

#[test]
fn helpers_version_constant_exists() {
    assert_eq!(HELPERS_VERSION, "2.2.0");
}

#[test]
fn mutation_response_sql_content_exists() {
    assert!(MUTATION_RESPONSE_SQL.contains("fraiseql.library_version"));
    assert!(MUTATION_RESPONSE_SQL.contains("fraiseql.mutation_ok"));
    assert!(MUTATION_RESPONSE_SQL.contains("fraiseql.mutation_err"));
}

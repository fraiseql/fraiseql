#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code — panics are acceptable

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

#[test]
fn changelog_contract_sql_content_exists() {
    // The vendored contract installs the table the mutation outbox writes (#569).
    assert!(CHANGELOG_CONTRACT_SQL.contains("core.tb_entity_change_log"));
    assert!(CHANGELOG_CONTRACT_SQL.contains("CREATE TABLE IF NOT EXISTS"));
}

/// #569 anti-drift guard (mandatory, gate #3). The CLI's vendored change-log contract DDL
/// must stay **byte-identical** to the observers migration that OWNS the contract. If this
/// fails, re-copy `crates/fraiseql-observers/migrations/08_create_entity_change_log_contract.sql`
/// into `crates/fraiseql-cli/sql/helpers/entity_change_log_contract.sql`.
#[test]
fn changelog_contract_matches_observers_migration() {
    let migration_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../fraiseql-observers/migrations/08_create_entity_change_log_contract.sql");
    let migration = std::fs::read_to_string(&migration_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", migration_path.display()));
    assert_eq!(
        CHANGELOG_CONTRACT_SQL, migration,
        "CLI change-log contract DDL drifted from observers migration 08 — re-copy it"
    );
}

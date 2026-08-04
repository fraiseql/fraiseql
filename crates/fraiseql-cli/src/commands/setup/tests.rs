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

/// #390 lockstep: the change-log CHECK constraint must name **exactly** the
/// serialized tokens of [`fraiseql_core::security::ActorType`]. Adding an enum
/// variant without extending `chk_entity_change_log_actor_type` in migration 08
/// (and re-copying the vendored helper) makes this red — otherwise the database
/// would refuse rows the runtime legitimately stamps.
///
/// This tests the vendored copy; [`changelog_contract_matches_observers_migration`]
/// pins it byte-identical to the owning migration, so the check is transitive.
#[test]
fn actor_constraint_covers_every_actor_type_token() {
    let constraint_start = CHANGELOG_CONTRACT_SQL
        .find("chk_entity_change_log_actor_type CHECK")
        .expect("contract DDL carries the actor-type CHECK constraint");
    let constraint = &CHANGELOG_CONTRACT_SQL[constraint_start..];
    let constraint = &constraint[..constraint.find(';').expect("constraint terminated")];

    for actor in fraiseql_core::security::ActorType::ALL {
        let quoted = format!("'{}'", actor.as_str());
        assert!(
            constraint.contains(&quoted),
            "CHECK constraint is missing ActorType token {quoted} — extend migration 08"
        );
    }
    // And nothing beyond the enum: every quoted token in the IN (…) list parses.
    let in_list_start = constraint.find("IN (").expect("constraint uses IN (…)");
    let in_list = &constraint[in_list_start..];
    let in_list = &in_list[..in_list.find(')').expect("IN list closed")];
    for token in in_list.split('\'').skip(1).step_by(2) {
        assert!(
            fraiseql_core::security::ActorType::from_token(token).is_some(),
            "constraint names '{token}', which is not an ActorType — remove it or add the variant"
        );
    }
}

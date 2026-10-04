//! The CLI refuses a PostgreSQL server below 18 when it connects, not later on an
//! obscure SQL error.
//!
//! Two connection paths exist outside the `PostgresAdapter` (which carries its own
//! check, tested in `fraiseql-db`): the shared catalogue pool every introspecting
//! command opens, and `generate-views --validate`'s single client. Each is driven
//! against PostgreSQL 17.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `BELOW_FLOOR_DATABASE_URL`
//! (PostgreSQL 17)
//!
//! `--all-features` compiles this file into the database-free test leg too, so it skips
//! where no `DATABASE_URL` is bound, like every other suite here. Where one is bound, the
//! below-floor server must be too: a rig that lost it fails here instead of going green.

#![cfg(feature = "test-postgres")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::{io::Write, process::Command};

use fraiseql_cli::schema::pg_catalog::PgCatalog;
use fraiseql_core::schema::{CompiledSchema, FieldDefinition, FieldType, TypeDefinition};
use fraiseql_test_support::{below_floor_database_url, try_database_url};
use tempfile::Builder;

/// `None` in the database-free leg; otherwise the PostgreSQL 17 URL, which must be bound.
fn below_floor_url(test: &str) -> Option<String> {
    if try_database_url().is_none() {
        eprintln!("SKIP {test}: no DATABASE_URL (database-free leg)");
        return None;
    }
    Some(below_floor_database_url())
}

fn assert_names_the_floor(message: &str) {
    for needle in ["PostgreSQL 17", "PostgreSQL 18 or newer", "pg_upgrade"] {
        assert!(message.contains(needle), "missing {needle:?}: {message}");
    }
}

#[tokio::test]
async fn the_catalogue_pool_refuses_postgresql_17() {
    let Some(url) = below_floor_url("the_catalogue_pool_refuses_postgresql_17") else {
        return;
    };
    let Err(err) =
        PgCatalog::connect(&url, &fraiseql_db::postgres::PostgresTlsConfig::default()).await
    else {
        panic!("PgCatalog connected to PostgreSQL 17");
    };
    assert_names_the_floor(&format!("{err:#}"));
}

#[test]
fn generate_views_validate_refuses_postgresql_17() {
    let Some(url) = below_floor_url("generate_views_validate_refuses_postgresql_17") else {
        return;
    };
    let mut schema = CompiledSchema::new();
    let mut thing = TypeDefinition::new("Thing", "v_floor_thing");
    thing.jsonb_column = "data".to_string();
    thing.fields.push(FieldDefinition::new("id", FieldType::Id));
    schema.types.push(thing);
    let mut file = Builder::new().suffix(".json").tempfile().unwrap();
    file.write_all(schema.to_json().unwrap().as_bytes()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args([
            "generate-views",
            "-s",
            file.path().to_str().unwrap(),
            "--entity",
            "Thing",
            "--view",
            "va_floor_thing",
            "--validate",
        ])
        .env("DATABASE_URL", url)
        .output()
        .expect("spawn fraiseql-cli");

    assert!(!output.status.success(), "--validate must fail on PostgreSQL 17");
    assert_names_the_floor(&String::from_utf8_lossy(&output.stderr));
}

use super::*;

#[test]
fn the_lock_is_the_scripts_first_statement() {
    let script = lock_prefixed("CREATE TABLE IF NOT EXISTS t (a INT);");

    // First, not merely present: a lock taken after the DDL it guards is taken after
    // the race it is meant to prevent.
    let first = script.split_once(';').expect("the script is statement-terminated").0;
    assert_eq!(first, format!("SELECT pg_advisory_xact_lock({MIGRATION_LOCK_KEY})"));
    assert!(script.ends_with("CREATE TABLE IF NOT EXISTS t (a INT);"));
}

#[test]
fn a_locked_statement_takes_the_lock_before_its_ddl_in_one_block() {
    let block =
        locked_statement("CREATE SCHEMA IF NOT EXISTS tenant_acme").expect("no tag in the DDL");

    assert_eq!(
        block,
        format!(
            "DO $fraiseql_ddl$ BEGIN PERFORM pg_advisory_xact_lock({MIGRATION_LOCK_KEY}); \
             CREATE SCHEMA IF NOT EXISTS tenant_acme; END $fraiseql_ddl$"
        )
    );
}

#[test]
fn a_statement_carrying_the_tag_is_refused() {
    let closing = "CREATE SCHEMA a; END $fraiseql_ddl$; DROP SCHEMA b; DO $fraiseql_ddl$ BEGIN";

    assert_eq!(locked_statement(closing), Err(TagInStatement));
}

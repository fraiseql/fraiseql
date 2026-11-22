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

#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
//! The connection-string rule: both PostgreSQL forms in, removed engines out, and
//! no password in any message.

use super::require_postgres;

#[test]
fn a_postgres_url_is_accepted_in_either_scheme() {
    require_postgres("postgres://localhost/db").unwrap();
    require_postgres("postgresql://u:p@h:5432/db").unwrap();
}

/// The form `compile` skipped the mutation-contract check for (#1403).
#[test]
fn a_libpq_key_value_string_is_accepted() {
    require_postgres("host=db dbname=app user=app").unwrap();
}

#[test]
fn a_removed_engine_is_refused_by_the_postgresql_only_rule() {
    for url in [
        "mysql://localhost/db",
        "sqlite://./x.db",
        "mssql://h/db",
        "app.db",
    ] {
        let message = require_postgres(url).unwrap_err().to_string();
        assert!(message.contains("PostgreSQL-only"), "{url}: {message}");
    }
}

#[test]
fn an_unparseable_string_is_refused_naming_both_forms() {
    let message = require_postgres("not-a-url").unwrap_err().to_string();
    assert!(
        message.contains("postgresql://") && message.contains("key=value"),
        "the refusal must name the accepted forms; got: {message}"
    );
}

/// A refusal must never carry the connection string back out: it is printed to
/// the terminal and to CI logs, and the string can hold a password.
#[test]
fn a_refusal_does_not_echo_the_password() {
    for url in [
        "postgres://u:hunter2@h:notaport/db",
        "host=h password=hunter2 port=notaport",
        "mysql://u:hunter2@h",
    ] {
        let message = format!("{:#}", require_postgres(url).unwrap_err());
        assert!(!message.contains("hunter2"), "{url}: {message}");
    }
}

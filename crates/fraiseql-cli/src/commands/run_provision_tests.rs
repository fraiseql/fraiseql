//! `fraiseql run --admin-database-url`: reading the URLs it provisions from.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code

use super::*;

#[test]
fn a_url_s_place_defaults_its_port_and_database() {
    let parsed = login("postgres://alice:pw@db", "TEST").unwrap();
    assert_eq!(parsed.user, "alice");
    assert_eq!(parsed.password.as_deref(), Some("pw"));
    assert_eq!(
        parsed.place,
        Place {
            hosts:    vec!["db".to_string()],
            ports:    vec![5432],
            database: "alice".to_string(),
        }
    );
}

#[test]
fn a_url_that_is_not_one_is_refused_without_echoing_it() {
    let Err(error) = login("postgres://user:s3cret@host:notaport/db", "THE_VAR") else {
        panic!("refused");
    };
    let message = error.to_string();
    assert!(message.contains("THE_VAR"), "{message}");
    assert!(!message.contains("s3cret"), "{message}");
}

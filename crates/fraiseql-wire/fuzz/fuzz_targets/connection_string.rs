#![no_main]

//! Property target for #817: the connection-string parser must not let a query
//! parameter bleed into the host or the database name.
//!
//! `parse_tcp` used to split userinfo at `rfind('@')` and take everything after
//! the first `/` as the database, with no `?` handling at all. So every standard
//! libpq parameter (`sslmode`, `application_name`, …) was appended to the
//! database name, and an `@` inside a parameter *value* was taken as the userinfo
//! delimiter — the host was then parsed out of the tail of the query string.
//!
//! Both failures are silent: you get a `ConnectionInfo` that connects somewhere
//! other than where the string said, which is why a wrong host matters more than
//! a wrong database. `sslmode` in particular decides whether the connection is
//! encrypted, so folding it into the database name is a security-relevant loss.
//!
//! The properties, for any string the parser accepts:
//!
//! 1. no component contains a `?` — a component holding one is a component the
//!    query string bled into;
//! 2. the host never contains `&`, `=` or `/` — those cannot occur in a hostname
//!    and their presence means the host was parsed out of the query string;
//! 3. the database name is a single path segment.

use fraiseql_wire::client::connection_string::ConnectionInfo;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    // Keep the fuzzer on the shapes the parser is meant to accept; a random blob
    // is rejected long before it can exercise the component split.
    if !data.starts_with("postgres://") && !data.starts_with("postgresql://") {
        return;
    }

    let Ok(info) = ConnectionInfo::parse(data) else {
        return;
    };

    if let Some(host) = &info.host {
        assert!(
            !host.contains('?'),
            "query string bled into the host: {host:?} (from {data:?})"
        );
        assert!(
            !host.contains('&') && !host.contains('=') && !host.contains('/'),
            "host was parsed out of the query string: {host:?} (from {data:?})"
        );
    }

    if let Some(db) = &info.database {
        assert!(
            !db.contains('?'),
            "query string was folded into the database name: {db:?} (from {data:?})"
        );
        assert!(
            !db.contains('/'),
            "database name is not a single path segment: {db:?} (from {data:?})"
        );
    }

    if let Some(user) = &info.user {
        assert!(
            !user.contains('?'),
            "query string bled into the user: {user:?} (from {data:?})"
        );
    }
});

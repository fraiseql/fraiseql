use super::*;

#[test]
fn accepts_postgresql_scheme() {
    assert_eq!(
        parse_database_url("postgresql://user@localhost/db").expect("accepted"),
        DatabaseScheme::Postgres,
    );
}

#[test]
fn accepts_postgres_alias() {
    assert_eq!(
        parse_database_url("postgres://user@localhost/db").expect("accepted"),
        DatabaseScheme::Postgres,
    );
}

#[test]
fn accepts_postgresql_with_query_string() {
    assert_eq!(
        parse_database_url("postgresql://user:pw@host:5432/db?sslmode=require")
            .expect("query-string parameters must not affect scheme parsing"),
        DatabaseScheme::Postgres,
    );
}

#[test]
fn rejects_unknown_scheme_with_clear_message() {
    let err = parse_database_url("redis://localhost:6379")
        .expect_err("redis:// is not a supported database scheme")
        .to_string();
    assert!(
        err.starts_with(GUARD_MESSAGE_PREFIX),
        "diagnostic must start with the operator-facing prefix: {err}"
    );
    assert!(err.contains("\"redis\""), "missing observed-scheme reproduction: {err}");
}

#[test]
fn rejects_empty_string() {
    let err = parse_database_url("").expect_err("empty URL must be rejected").to_string();
    assert!(err.starts_with(GUARD_MESSAGE_PREFIX), "{err}");
}

#[test]
fn rejects_url_without_scheme() {
    let err = parse_database_url("localhost:5432")
        .expect_err("URL without a scheme must be rejected")
        .to_string();
    // Before #731 the whole string was reported as the "observed scheme", because
    // `split("://").next()` returns the input when there is no separator. That is
    // also why a bare `"postgres"` was *accepted*. The refusal now names the real
    // fault: there is no scheme at all.
    assert!(err.contains("no scheme"), "{err}");
}

// ── #731: a scheme-less string is not a valid database URL ───────────────────

/// `"postgres"` has no `://`, so `split("://").next()` handed back the whole
/// string and it matched the `"postgres"` arm — the guard accepted it and the
/// failure resurfaced later as the opaque driver error the guard exists to
/// replace.
#[test]
fn a_scheme_less_string_is_refused() {
    for input in ["postgres", "postgresql", "mysql", "sqlite", "sqlserver"] {
        let err = super::parse_database_url(input)
            .expect_err("#731: a string with no `://` is not a database URL");
        assert!(
            err.to_string().contains("no scheme"),
            "the refusal must say the scheme is missing, for {input:?}: {err}"
        );
    }
}

/// Counterweight: the supported schemes with a separator still parse.
#[test]
fn the_supported_schemes_still_parse_with_a_separator() {
    assert!(super::parse_database_url("postgres://host/db").is_ok());
    assert!(super::parse_database_url("postgresql://host/db").is_ok());
}

// --- G2 de-scope (#374/#721/#799): removed schemes are refused LOUDLY ---
//
// FraiseQL is PostgreSQL-only. A URL for a removed engine must produce an
// explanatory error naming the removal and pointing at PostgreSQL — not
// "unknown scheme", and never a fall-through to an opaque driver error.

#[test]
fn mysql_scheme_is_refused_with_removal_notice() {
    let err = parse_database_url("mysql://user:pw@localhost:3306/mydb")
        .expect_err("mysql:// must be refused")
        .to_string();
    assert!(err.starts_with(GUARD_MESSAGE_PREFIX), "grep-able prefix, got: {err}");
    assert!(err.contains("mysql"), "must name the observed scheme, got: {err}");
    assert!(err.contains("removed"), "must say the support was removed, got: {err}");
    assert!(err.contains("PostgreSQL-only"), "must state the posture, got: {err}");
    assert!(err.contains("postgresql://"), "must point at the supported scheme, got: {err}");
}

#[test]
fn sqlite_scheme_is_refused_with_removal_notice() {
    let err = parse_database_url("sqlite://./mydb.db")
        .expect_err("sqlite:// must be refused")
        .to_string();
    assert!(err.starts_with(GUARD_MESSAGE_PREFIX), "grep-able prefix, got: {err}");
    assert!(err.contains("sqlite"), "must name the observed scheme, got: {err}");
    assert!(err.contains("removed"), "must say the support was removed, got: {err}");
    assert!(err.contains("PostgreSQL-only"), "must state the posture, got: {err}");
}

#[test]
fn sqlserver_scheme_is_refused_with_removal_notice() {
    let err = parse_database_url("sqlserver://localhost:1433")
        .expect_err("sqlserver:// must be refused")
        .to_string();
    assert!(err.starts_with(GUARD_MESSAGE_PREFIX), "grep-able prefix, got: {err}");
    assert!(err.contains("sqlserver"), "must name the observed scheme, got: {err}");
    assert!(err.contains("removed"), "must say the support was removed, got: {err}");
    assert!(err.contains("PostgreSQL-only"), "must state the posture, got: {err}");
}

#[test]
fn removed_scheme_error_differs_from_unknown_scheme_error() {
    // An operator pointing at mysql:// made a *supported-in-the-past* choice;
    // the diagnostic must say "removed", while a genuinely unknown scheme
    // ("oracle://") keeps the generic unsupported-scheme message.
    let removed = parse_database_url("mysql://h/db").expect_err("refused").to_string();
    let unknown = parse_database_url("oracle://h/db").expect_err("refused").to_string();
    assert!(removed.contains("removed"), "removed-scheme message: {removed}");
    assert!(
        !unknown.contains("removed"),
        "unknown-scheme message must not claim removal: {unknown}"
    );
}

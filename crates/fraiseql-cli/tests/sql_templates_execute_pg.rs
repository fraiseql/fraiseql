//! The compiled rich-filter SQL templates are EXECUTED against live PostgreSQL.
//!
//! `sql_templates.rs` is a hardcoded table of SQL fragments compiled into every
//! schema. Three audit passes found entries that are not valid SQL or compute
//! the wrong answer, because nothing had ever run them (#721) — the existing
//! tests assert only that a template *exists* for an operator, which a wrong
//! template satisfies exactly as well as a right one.
//!
//! This suite substitutes the placeholders and asks PostgreSQL for the answer.
//!
//! Self-skips when no `DATABASE_URL` is set, so it is inert in the
//! database-free leg.
//!
//! **Execution engine:** `PostgreSQL`
//! **Infrastructure:** `DATABASE_URL`
#![cfg(feature = "test-postgres")]
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use fraiseql_cli::schema::sql_templates::extract_template_for_operator;
use tokio_postgres::NoTls;

async fn connect() -> Option<tokio_postgres::Client> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let (client, connection) = tokio_postgres::connect(&url, NoTls).await.ok()?;
    tokio::spawn(async move { connection.await.ok() });
    Some(client)
}

/// Render a template into a runnable `SELECT` by substituting `$field` with a
/// literal and evaluating the predicate against `$1`.
async fn eval_predicate(
    client: &tokio_postgres::Client,
    template: &str,
    field: &str,
    arg: &str,
) -> bool {
    let predicate = template.replace("$field", &format!("'{field}'"));
    let sql = format!("SELECT ({predicate}) AS matched");
    let row = client.query_one(&sql, &[&arg]).await;
    assert!(row.is_ok(), "template must be valid SQL: {sql}\nerror: {row:?}");
    row.unwrap().get::<_, Option<bool>>("matched").unwrap_or(false)
}

/// #721 — `tldEq` must extract the TLD, i.e. everything after the **last** dot.
///
/// The template split on the FIRST dot and kept the dot, so `example.com`
/// yielded `.com` (never equal to the `com` a client sends) and
/// `mail.example.com` yielded `.example.com`. Every `tldEq` filter therefore
/// matched nothing, silently.
#[tokio::test]
async fn tld_eq_extracts_the_last_label_for_any_domain_depth() {
    let Some(client) = connect().await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let template = extract_template_for_operator("postgres", "tldEq")
        .expect("postgres tldEq template must exist");

    for domain in ["example.com", "mail.example.com", "a.b.c.example.com"] {
        assert!(
            eval_predicate(&client, &template, domain, "com").await,
            "{domain} has TLD 'com'; template: {template}"
        );
        assert!(
            !eval_predicate(&client, &template, domain, "example").await,
            "{domain} must NOT match TLD 'example' — that is a label, not the TLD"
        );
    }

    // A different TLD must not match, or the predicate is vacuously true.
    assert!(
        !eval_predicate(&client, &template, "example.com", "org").await,
        "example.com must not match TLD 'org'"
    );
    // A domain with no dot is its own last label.
    assert!(
        eval_predicate(&client, &template, "localhost", "localhost").await,
        "a dotless host is its own TLD"
    );
}

/// The `IN`-list variant shares the extraction and must agree with `tldEq`.
#[tokio::test]
async fn tld_in_uses_the_same_extraction_as_tld_eq() {
    let Some(client) = connect().await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let template = extract_template_for_operator("postgres", "tldIn")
        .expect("postgres tldIn template must exist");
    // `$params` is the compiler's placeholder for the expanded bind list; a
    // single-element list is `$1`.
    let template = template.replace("$params", "$1");

    for domain in ["example.com", "mail.example.com"] {
        assert!(
            eval_predicate(&client, &template, domain, "com").await,
            "{domain} has TLD 'com'; template: {template}"
        );
    }
    assert!(!eval_predicate(&client, &template, "example.com", "net").await);
}

/// The email-domain templates are the neighbouring extraction family and are
/// exercised here so a future edit to one cannot silently break the others.
#[tokio::test]
async fn email_domain_templates_extract_the_part_after_the_at_sign() {
    let Some(client) = connect().await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let template = extract_template_for_operator("postgres", "domainEq")
        .expect("postgres domainEq template must exist");

    assert!(eval_predicate(&client, &template, "alice@example.com", "example.com").await);
    assert!(!eval_predicate(&client, &template, "alice@example.com", "other.com").await);
}

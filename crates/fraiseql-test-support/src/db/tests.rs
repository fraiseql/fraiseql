#![allow(clippy::unwrap_used)] // Reason: test code

use super::resolve_or_panic;

/// The loud-failure contract: a missing DB URL must abort, not silently default.
/// If this ever returns instead of panicking, DB-backed integration tests would
/// pass vacuously whenever CI fails to inject `DATABASE_URL`.
#[test]
#[should_panic(expected = "DATABASE_URL is not set")]
fn resolve_or_panic_is_loud_when_unset() {
    let _ = resolve_or_panic("DATABASE_URL", "postgresql://...", None);
}

#[test]
#[should_panic(expected = "SOME_URL is not set")]
fn resolve_or_panic_names_the_missing_var() {
    let _ = resolve_or_panic("SOME_URL", "scheme://...", None);
}

#[test]
fn resolve_or_panic_returns_the_set_url() {
    assert_eq!(
        resolve_or_panic(
            "DATABASE_URL",
            "postgresql://...",
            Some("postgresql://localhost/test".to_string())
        ),
        "postgresql://localhost/test"
    );
}

/// Runs one of this binary's `#[ignore]`d probes in a child process, with `env` set (or
/// removed, for `None`) and `FRAISEQL_TEST_REQUIRE_POSTGRES` as `require` says. A child is
/// how the real getter meets a real environment: this crate cannot change its own.
fn probe(name: &str, require: bool, database_url: Option<&str>) -> (bool, String) {
    let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
    cmd.args([
        "--exact",
        name,
        "--ignored",
        "--test-threads=1",
        "--nocapture",
    ]);
    match database_url {
        Some(url) => cmd.env("DATABASE_URL", url),
        None => cmd.env_remove("DATABASE_URL"),
    };
    if require {
        cmd.env(super::REQUIRE_POSTGRES_ENV, "1");
    } else {
        cmd.env_remove(super::REQUIRE_POSTGRES_ENV);
    }
    let out = cmd.output().unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

/// Nothing listens on port 1, so this names a database that cannot be reached.
const UNREACHABLE: &str = "postgresql://fraiseql@127.0.0.1:1/none";

#[test]
#[ignore = "a probe: run by the tests below, in a child with the environment they set"]
fn probe_try_database_url() {
    let _ = super::try_database_url();
}

#[test]
#[ignore = "a probe: run by the tests below, in a child with the environment they set"]
fn probe_postgres() {
    // Multi-threaded: with `local-testcontainers`, an unset URL spawns a container, whose
    // drop needs one.
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
    let _ = runtime.block_on(crate::services::postgres());
}

#[test]
fn try_database_url_fails_where_a_database_is_required_and_unset() {
    let (ok, out) = probe("db::tests::probe_try_database_url", true, None);
    assert!(!ok, "the child returned instead of failing:\n{out}");
    assert!(
        out.contains("DATABASE_URL is not set, but FRAISEQL_TEST_REQUIRE_POSTGRES"),
        "{out}"
    );
}

/// Without `local-testcontainers` an unset URL finds no database. With it, the getter
/// spawns one where Docker runs, which is a database and not a skip, so this holds only in
/// the build that cannot spawn.
#[cfg(not(feature = "local-testcontainers"))]
#[test]
fn postgres_fails_where_a_database_is_required_and_unset() {
    let (ok, out) = probe("db::tests::probe_postgres", true, None);
    assert!(!ok, "the child returned instead of failing:\n{out}");
    assert!(
        out.contains("DATABASE_URL is not set, but FRAISEQL_TEST_REQUIRE_POSTGRES"),
        "{out}"
    );
}

#[test]
fn postgres_fails_where_a_database_is_required_and_unreachable() {
    let (ok, out) = probe("db::tests::probe_postgres", true, Some(UNREACHABLE));
    assert!(!ok, "the child returned instead of failing:\n{out}");
    assert!(out.contains("set but unreachable, but FRAISEQL_TEST_REQUIRE_POSTGRES"), "{out}");
}

/// Without the flag, a run with no database still skips: a developer's checkout is not a
/// leg that promised one.
#[test]
fn the_getters_still_skip_where_no_database_is_required() {
    let mut cases = vec![
        ("db::tests::probe_try_database_url", None),
        ("db::tests::probe_postgres", Some(UNREACHABLE)),
    ];
    // An unset URL spawns a container under `local-testcontainers`; see above.
    if cfg!(not(feature = "local-testcontainers")) {
        cases.push(("db::tests::probe_postgres", None));
    }
    for (name, url) in cases {
        let (ok, out) = probe(name, false, url);
        assert!(ok, "{name} with DATABASE_URL={url:?} failed without the flag:\n{out}");
        assert!(out.contains("1 passed"), "{name} did not run in the child:\n{out}");
    }
}

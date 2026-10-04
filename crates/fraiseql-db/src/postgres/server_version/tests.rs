use fraiseql_error::FraiseQLError;

use super::{MINIMUM_SERVER_VERSION_NUM, check_server_version};

#[test]
fn postgresql_18_0_is_supported() {
    check_server_version(180_000, "18.0").expect("18.0 is the floor");
}

#[test]
fn a_later_release_is_supported() {
    check_server_version(190_002, "19.2").expect("newer than the floor");
}

#[test]
fn the_last_17_release_is_refused() {
    let err = check_server_version(179_999, "17.99").expect_err("below the floor");
    assert!(matches!(err, FraiseQLError::Unsupported { .. }), "{err:?}");
}

#[test]
fn the_refusal_names_the_version_found_the_floor_and_the_upgrade_path() {
    let message = check_server_version(160_004, "16.4 (Debian 16.4-1.pgdg120+2)")
        .expect_err("below the floor")
        .to_string();
    for needle in [
        "16.4 (Debian 16.4-1.pgdg120+2)",
        "PostgreSQL 18",
        "pg_upgrade",
    ] {
        assert!(message.contains(needle), "missing {needle:?}: {message}");
    }
}

#[test]
fn the_floor_is_postgresql_18_0() {
    assert_eq!(MINIMUM_SERVER_VERSION_NUM, 180_000);
}

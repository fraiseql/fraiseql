//! Tests for the Redis URL a security section names.

use super::*;

#[test]
fn user_info_and_password_parameters_are_credentials() {
    for url in [
        "redis://:pw@redis:6379/0",
        "redis://app:pw@redis:6379",
        "redis://app@redis",
        "rediss://app:pw@redis:6380",
        "redis://redis:6379/0?password=pw",
        "redis+unix:///run/redis.sock?db=0&pass=pw",
        "redis+unix:///run/redis.sock?PASSWORD=pw",
    ] {
        assert!(holds_credential(url), "{url}");
    }
}

#[test]
fn a_host_path_and_database_are_not_credentials() {
    for url in [
        "redis://redis:6379/0",
        "rediss://redis.internal:6380",
        "redis://redis/0?protocol=resp3",
        "redis+unix:///run/redis.sock?db=0",
        "redis://redis:6379/0#pw@x",
    ] {
        assert!(!holds_credential(url), "{url}");
    }
}

#[test]
fn the_host_drops_user_info_path_and_query() {
    assert_eq!(host_of("redis://app:pw@redis:6379/0?password=pw"), "redis://redis:6379");
    assert_eq!(host_of("rediss://:pw@redis"), "rediss://redis");
    assert_eq!(host_of("redis+unix:///run/redis.sock?pass=pw"), "redis+unix:///run/redis.sock");
    assert_eq!(host_of("pw-but-no-scheme"), "(not a URL)");
}

#[test]
fn an_environment_name_is_an_identifier() {
    for name in ["REDIS_URL", "_R", "r1"] {
        assert!(is_env_name(name), "{name}");
    }
    for name in ["", "1R", "R URL", "redis://redis", "R-URL"] {
        assert!(!is_env_name(name), "{name}");
    }
}

#[test]
fn the_check_refuses_by_key_and_never_shows_the_url() {
    let refusal = check("pkce", Some("redis://:pw-SENTINEL@redis"), None).expect_err("refused");
    assert!(refusal.contains("[security.pkce] redis_url"), "{refusal}");
    assert!(refusal.contains("redis_url_env"), "{refusal}");
    assert!(!refusal.contains("pw-SENTINEL"), "{refusal}");

    assert!(
        check("pkce", Some("redis://redis"), Some("R"))
            .expect_err("refused")
            .contains("both")
    );
    assert!(
        check("pkce", None, Some("1R"))
            .expect_err("refused")
            .contains("environment variable")
    );
    assert_eq!(check("pkce", Some("redis://redis"), None), Ok(()));
    assert_eq!(check("pkce", None, Some("R")), Ok(()));
    assert_eq!(check("pkce", None, None), Ok(()));
}

#[test]
fn resolution_reads_the_named_variable() {
    let env = |name: &str| match name {
        "SET" => Some("redis://:pw@redis".to_owned()),
        "BLANK" => Some(" ".to_owned()),
        _ => None,
    };
    assert_eq!(resolve("pkce", None, Some("SET"), env), Ok(Some("redis://:pw@redis".into())));
    assert_eq!(resolve("pkce", Some("redis://r"), None, env), Ok(Some("redis://r".into())));
    assert_eq!(resolve("pkce", None, None, env), Ok(None));
    let unset = resolve("pkce", None, Some("UNSET"), env).expect_err("refused");
    assert!(unset.contains("`UNSET`") && unset.contains("not set"), "{unset}");
    assert!(
        resolve("pkce", None, Some("BLANK"), env)
            .expect_err("refused")
            .contains("empty")
    );
    assert!(
        resolve("pkce", Some("redis://r"), Some("SET"), env)
            .expect_err("refused")
            .contains("both")
    );
}

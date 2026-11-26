//! Tests for the NATS URL `[observers]` names.

use super::*;

#[test]
fn user_info_on_any_listed_server_is_a_credential() {
    for url in [
        "nats://app:pw@nats:4222",
        "tls://token@nats:4222",
        "nats://nats-1:4222,nats://app:pw@nats-2:4222",
        "nats://nats-1:4222, tls://token@nats-2:4222",
    ] {
        assert!(holds_credential(url), "{url}");
    }
}

#[test]
fn hosts_and_ports_are_not_credentials() {
    for url in [
        "nats://nats:4222",
        "tls://nats.internal:4443",
        "nats://nats-1:4222,nats://nats-2",
    ] {
        assert!(!holds_credential(url), "{url}");
    }
}

#[test]
fn each_listed_server_is_named_by_its_host() {
    assert_eq!(host_of("tls://app:pw@nats:4443"), "tls://nats:4443");
    assert_eq!(
        host_of("nats://nats-1:4222, nats://token@nats-2:4222"),
        "nats://nats-1:4222,nats://nats-2:4222"
    );
}

#[test]
fn a_written_url_with_a_credential_is_refused_naming_the_key_only() {
    let refusal = check(Some("nats://app:pw-SENTINEL@nats:4222"), None).expect_err("refused");
    assert!(refusal.contains("nats_url_env"), "{refusal}");
    assert!(!refusal.contains("pw-SENTINEL"), "{refusal}");
}

#[test]
fn both_keys_or_a_url_in_the_variable_name_are_refused() {
    assert!(check(Some("nats://nats:4222"), Some("NATS_URL")).is_err());
    assert!(check(None, Some("nats://nats:4222")).is_err());
}

#[test]
fn a_plain_url_or_a_variable_name_is_accepted() {
    assert_eq!(check(Some("tls://nats:4443"), None), Ok(()));
    assert_eq!(check(None, Some("FRAISEQL_OBSERVERS_NATS_URL")), Ok(()));
    assert_eq!(check(None, None), Ok(()));
}

#[test]
fn the_variable_is_read_and_must_hold_a_value() {
    let env = |name: &str| match name {
        "SET" => Some("tls://app:pw@nats:4443".to_owned()),
        "BLANK" => Some("  ".to_owned()),
        _ => None,
    };
    assert_eq!(resolve(None, Some("SET"), env), Ok(Some("tls://app:pw@nats:4443".to_owned())));
    assert!(resolve(None, Some("BLANK"), env).expect_err("refused").contains("empty"));
    assert!(resolve(None, Some("UNSET"), env).expect_err("refused").contains("not set"));
    assert_eq!(
        resolve(Some("tls://nats:4443"), None, env),
        Ok(Some("tls://nats:4443".to_owned()))
    );
    assert_eq!(resolve(None, None, env), Ok(None));
}

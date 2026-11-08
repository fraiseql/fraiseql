//! The URL redaction's own tests: every shape of credential, and input it cannot parse.

use super::{MASK, url, url_opt};

#[test]
fn a_url_without_credentials_is_unchanged() {
    for raw in [
        "postgres://db.example.com:5432/app",
        "https://hooks.example.com/notify",
        "nats://[::1]:4222",
        "redis://cache",
    ] {
        assert_eq!(url(raw), raw);
    }
}

#[test]
fn a_user_without_a_password_is_kept() {
    assert_eq!(url("postgres://app@db:5432/app"), "postgres://app@db:5432/app");
}

#[test]
fn the_password_is_masked_and_the_user_kept() {
    assert_eq!(url("postgres://app:s3cret@db:5432/app"), "postgres://app:***@db:5432/app");
    assert_eq!(url("redis://:s3cret@cache:6379"), "redis://:***@cache:6379");
}

#[test]
fn a_percent_encoded_password_is_masked_whole() {
    assert_eq!(url("postgres://app:p%40ss%3Aw0rd@db/app"), "postgres://app:***@db/app");
}

#[test]
fn an_unencoded_at_sign_in_the_password_is_masked_with_it() {
    let rendered = url("postgres://app:p@ss@db/app");
    assert_eq!(rendered, "postgres://app:***@db/app");
}

#[test]
fn a_delimiter_in_an_unencoded_password_does_not_end_the_host_early() {
    for (raw, rendered) in [
        ("postgres://app:pa/ss@db/app", "postgres://app:***@db/app"),
        ("postgres://app:pa?ss@db/app", "postgres://app:***@db/app"),
        ("postgres://app:pa#ss@db/app", "postgres://app:***@db/app"),
    ] {
        assert_eq!(url(raw), rendered, "{raw:?}");
    }
}

#[test]
fn an_at_sign_after_the_host_masks_everything_before_it() {
    let rendered = url("https://hooks.example.com/x?token=abc@def");
    assert!(!rendered.contains("abc"), "{rendered}");
    assert_eq!(rendered, "https://***@def");
}

#[test]
fn query_values_are_masked_and_keys_kept() {
    assert_eq!(
        url("https://hooks.example.com/x?token=abc&v=2&flag"),
        "https://hooks.example.com/x?token=***&v=***&***"
    );
}

#[test]
fn a_fragment_is_masked() {
    assert_eq!(
        url("https://app.example.com/cb#access_token=abc"),
        "https://app.example.com/cb#***"
    );
}

#[test]
fn input_without_the_url_shape_is_masked_whole() {
    for raw in [
        "",
        "not a url",
        "app:s3cret@db:5432",
        "data:text/plain,s3cret",
        "http://app:s3cret@exa mple.com/",
        "1http://app:s3cret@db/",
        "postgres://app:s3cret@db/\napp",
    ] {
        assert_eq!(url(raw), MASK, "{raw:?}");
    }
}

#[test]
fn an_optional_url_is_rendered_when_present() {
    assert_eq!(url_opt(Some("redis://u:p@h")), Some("redis://u:***@h".to_string()));
    assert_eq!(url_opt(None), None);
}

#[test]
fn the_display_wrapper_renders_the_same() {
    assert_eq!(
        super::Url("postgres://app:s3cret@db/app").to_string(),
        url("postgres://app:s3cret@db/app")
    );
}

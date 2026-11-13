use super::{DEFAULT_KEY_ENCRYPTION_KEY_ENV, IssuerConfig};
use crate::server_config::ServerConfig;

fn issuer(url: &str) -> IssuerConfig {
    IssuerConfig {
        url:                    url.to_string(),
        audience:               "my-api".to_string(),
        key_encryption_key_env: DEFAULT_KEY_ENCRYPTION_KEY_ENV.to_string(),
    }
}

fn refusal(config: &IssuerConfig) -> String {
    config.validate().expect_err("refused")
}

#[test]
fn the_section_parses_and_the_key_encryption_key_env_has_a_default() {
    let config: ServerConfig =
        toml::from_str("[auth_issuer]\nurl = \"https://api.example.com\"\naudience = \"my-api\"\n")
            .expect("parses");
    assert_eq!(config.auth_issuer, Some(issuer("https://api.example.com")));
}

#[cfg(feature = "auth")]
#[test]
fn the_default_key_encryption_key_env_is_the_one_the_key_ring_documents() {
    assert_eq!(DEFAULT_KEY_ENCRYPTION_KEY_ENV, fraiseql_auth::signing_key::DEFAULT_KEK_ENV);
}

#[test]
fn the_section_and_the_external_issuers_issuer_key_live_side_by_side() {
    let config: ServerConfig = toml::from_str(
        "[auth]\nissuer = \"https://idp.example.com\"\naudience = \"idp-aud\"\n\n\
         [auth_issuer]\nurl = \"https://api.example.com\"\naudience = \"my-api\"\n",
    )
    .expect("both parse");
    assert_eq!(
        config.auth.and_then(|auth| auth.issuer).as_deref(),
        Some("https://idp.example.com")
    );
    assert_eq!(config.auth_issuer.map(|i| i.url).as_deref(), Some("https://api.example.com"));
}

#[test]
fn an_https_url_is_accepted() {
    issuer("https://api.example.com").validate().expect("accepted");
    issuer("https://api.example.com/tenant/7")
        .validate()
        .expect("a path is accepted");
}

#[test]
fn plain_http_is_accepted_only_on_a_loopback_address() {
    issuer("http://127.0.0.1:8080").validate().expect("IPv4 loopback");
    issuer("http://[::1]:8080").validate().expect("IPv6 loopback");
    issuer("http://localhost:8080").validate().expect("localhost");
    issuer("http://localhost.:8080").validate().expect("localhost, absolute form");
    issuer("http://auth.localhost:8080").validate().expect("a name under localhost");
    assert!(refusal(&issuer("http://api.example.com")).contains("url"));
    assert!(refusal(&issuer("http://10.0.0.7")).contains("url"));
    assert!(refusal(&issuer("http://localhost.example.com")).contains("url"));
    assert!(refusal(&issuer("http://localhost.localdomain:8080")).contains("url"));
}

#[test]
fn a_url_that_is_not_an_absolute_https_url_is_refused() {
    for bad in ["", "api.example.com", "/relative", "ftp://api.example.com"] {
        assert!(refusal(&issuer(bad)).contains("url"), "{bad:?}");
    }
}

#[test]
fn a_url_with_a_query_or_fragment_is_refused() {
    assert!(refusal(&issuer("https://api.example.com/?a=b")).contains("url"));
    assert!(refusal(&issuer("https://api.example.com/#x")).contains("url"));
}

#[test]
fn an_empty_audience_is_refused() {
    let config = IssuerConfig {
        audience: " ".to_string(),
        ..issuer("https://api.example.com")
    };
    assert!(refusal(&config).contains("audience"));
}

#[test]
fn an_empty_key_encryption_key_env_is_refused() {
    let config = IssuerConfig {
        key_encryption_key_env: String::new(),
        ..issuer("https://api.example.com")
    };
    assert!(refusal(&config).contains("key_encryption_key_env"));
}

#[test]
fn the_issuer_and_hs256_together_are_refused() {
    let config: ServerConfig = toml::from_str(
        "[auth_hs256]\nsecret_env = \"S\"\naudience = \"a\"\n\n\
         [auth_issuer]\nurl = \"https://api.example.com\"\naudience = \"my-api\"\n",
    )
    .expect("parses");
    let message = config.validate().expect_err("refused");
    assert!(
        message.contains("[auth_issuer]") && message.contains("[auth_hs256]"),
        "{message}"
    );
}

#[test]
fn the_server_config_refuses_an_issuer_section_that_does_not_validate() {
    let config: ServerConfig =
        toml::from_str("[auth_issuer]\nurl = \"http://api.example.com\"\naudience = \"my-api\"\n")
            .expect("parses");
    let message = config.validate().expect_err("refused");
    assert!(message.contains("auth_issuer: `url`"), "{message}");
}

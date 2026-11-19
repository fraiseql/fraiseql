#![allow(clippy::unwrap_used)] // Reason: test code

use super::*;

fn config(url: &str) -> ServerConfig {
    ServerConfig {
        database_url: url.to_string(),
        ..ServerConfig::default()
    }
}

#[tokio::test]
async fn the_application_pool_keeps_no_connection_idle() {
    let pool = open_application_pool(&config("postgres://u@127.0.0.1:1/app")).unwrap();
    assert_eq!(pool.options().get_min_connections(), 0);
    assert_eq!(pool.options().get_max_connections(), 5);
    assert_eq!(pool.options().get_acquire_timeout(), Duration::from_secs(10));
    assert_eq!(pool.size(), 0, "building the pool opens no connection");
}

#[tokio::test]
async fn the_application_pool_takes_its_sizing_from_its_section() {
    let pool = open_application_pool(&ServerConfig {
        application_pool: ApplicationPoolConfig {
            max_connections:      9,
            acquire_timeout_secs: 3,
        },
        ..config("postgres://u@127.0.0.1:1/app")
    })
    .unwrap();
    assert_eq!(pool.options().get_max_connections(), 9);
    assert_eq!(pool.options().get_acquire_timeout(), Duration::from_secs(3));
}

#[test]
fn database_tls_applies_to_the_application_pool() {
    let options = connect_options(&ServerConfig {
        database_tls: Some(toml::from_str("postgres_ssl_mode = \"verify-full\"").unwrap()),
        ..config("postgres://u@db.example.com/app?sslmode=disable")
    })
    .unwrap();
    assert!(matches!(options.get_ssl_mode(), PgSslMode::VerifyFull));
}

#[test]
fn an_unset_tls_mode_leaves_the_url_to_decide() {
    let options =
        connect_options(&config("postgres://u@db.example.com/app?sslmode=require")).unwrap();
    assert!(matches!(options.get_ssl_mode(), PgSslMode::Require));
}

#[test]
fn a_zero_bound_is_refused_naming_the_field() {
    for (section, field) in [
        (
            ApplicationPoolConfig {
                max_connections: 0,
                ..ApplicationPoolConfig::default()
            },
            "max_connections",
        ),
        (
            ApplicationPoolConfig {
                acquire_timeout_secs: 0,
                ..ApplicationPoolConfig::default()
            },
            "acquire_timeout_secs",
        ),
    ] {
        let message = section.validate().unwrap_err();
        assert!(message.contains(field), "{message}");
    }
}

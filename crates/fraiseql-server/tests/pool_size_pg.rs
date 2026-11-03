//! The pool a server opens is the size its configuration and environment ask for (#1558).
//!
//! Self-skips when no `DATABASE_URL` is set.

#![cfg(feature = "cli")] // `ServerArgs` is the `cli` feature's (on by default)
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code

use fraiseql_server::{
    ServerArgs,
    server_config::{ServerConfig, build_postgres_adapter},
};
use fraiseql_test_support::try_database_url;

/// Held by every test that sets a variable `ServerArgs::from_env` reads.
static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[tokio::test]
async fn fraiseql_pool_max_size_sizes_the_pool_and_a_bad_value_is_refused() {
    let Some(url) = try_database_url() else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let env = ENV.lock().unwrap();

    std::env::set_var("FRAISEQL_POOL_MAX_SIZE", "7");
    let args = ServerArgs::from_env();
    std::env::set_var("FRAISEQL_POOL_MAX_SIZE", "seven");
    let refused = ServerArgs::from_env();
    std::env::remove_var("FRAISEQL_POOL_MAX_SIZE");
    drop(env);

    let mut config = ServerConfig {
        database_url: url,
        ..ServerConfig::default()
    };
    args.unwrap().apply_to_config(&mut config);
    let adapter = build_postgres_adapter(&config).await.unwrap();
    assert_eq!(adapter.pool().status().max_size, 7, "the pool the server opened");

    let message = refused.unwrap_err();
    assert!(message.contains("FRAISEQL_POOL_MAX_SIZE"), "{message}");
}

/// #1879: the pool the server opens retries a mutation's transaction as
/// `[transaction_retry]` says.
#[tokio::test]
async fn the_pool_retries_as_transaction_retry_says() {
    let Some(url) = try_database_url() else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let config: ServerConfig =
        toml::from_str("[transaction_retry]\nmax_attempts = 4\nbase_ms = 5\nmax_ms = 50\n")
            .unwrap();
    let config = ServerConfig {
        database_url: url,
        ..config
    };
    let adapter = build_postgres_adapter(&config).await.unwrap();
    assert_eq!(adapter.transaction_retry().max_attempts(), 4, "the pool the server opened");
}

/// #1879: `FRAISEQL_TRANSACTION_RETRY_MAX_ATTEMPTS` wins over `[transaction_retry]` in the
/// pool the server opens, and a bad value is refused.
#[tokio::test]
async fn fraiseql_transaction_retry_max_attempts_wins_and_a_bad_value_is_refused() {
    let Some(url) = try_database_url() else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let env = ENV.lock().unwrap();
    std::env::set_var("FRAISEQL_TRANSACTION_RETRY_MAX_ATTEMPTS", "6");
    let args = ServerArgs::from_env();
    std::env::set_var("FRAISEQL_TRANSACTION_RETRY_MAX_ATTEMPTS", "six");
    let refused = ServerArgs::from_env();
    std::env::remove_var("FRAISEQL_TRANSACTION_RETRY_MAX_ATTEMPTS");
    drop(env);

    let config: ServerConfig = toml::from_str("[transaction_retry]\nmax_attempts = 4\n").unwrap();
    let mut config = ServerConfig {
        database_url: url,
        ..config
    };
    args.unwrap().apply_to_config(&mut config);
    let adapter = build_postgres_adapter(&config).await.unwrap();
    assert_eq!(adapter.transaction_retry().max_attempts(), 6, "the pool the server opened");

    let message = refused.unwrap_err();
    assert!(message.contains("FRAISEQL_TRANSACTION_RETRY_MAX_ATTEMPTS"), "{message}");
}

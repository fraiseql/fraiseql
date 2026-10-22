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

/// One test, so nothing else in this binary reads the variable while it is set.
#[tokio::test]
async fn fraiseql_pool_max_size_sizes_the_pool_and_a_bad_value_is_refused() {
    let Some(url) = try_database_url() else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    std::env::set_var("FRAISEQL_POOL_MAX_SIZE", "7");
    let args = ServerArgs::from_env();
    std::env::set_var("FRAISEQL_POOL_MAX_SIZE", "seven");
    let refused = ServerArgs::from_env();
    std::env::remove_var("FRAISEQL_POOL_MAX_SIZE");

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

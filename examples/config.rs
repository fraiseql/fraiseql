//! Connection Configuration Example
//!
//! This example demonstrates how to use fraiseql-wire's connection configuration
//! features including timeouts, keepalive, and application name settings.
//!
//! Run with:
//!   cargo run --example config
//!
//! Configuration via environment variables:
//!   POSTGRES_HOST     - Database host (default: localhost)
//!   POSTGRES_PORT     - Database port (default: 5432)
//!   POSTGRES_USER     - Database user (default: postgres)
//!   POSTGRES_PASSWORD - Database password (default: postgres)
//!   POSTGRES_DB       - Database name (default: postgres)

use fraiseql_wire::{connection::ConnectionConfig, FraiseClient};
use std::env;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for debug output
    tracing_subscriber::fmt()
        .with_max_level(
            env::var("RUST_LOG")
                .ok()
                .and_then(|l| l.parse().ok())
                .unwrap_or(tracing::Level::INFO),
        )
        .init();

    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║  fraiseql-wire: Phase 8.3 Connection Configuration Example     ║");
    println!("║                                                                ║");
    println!("║  Demonstrates timeout, keepalive, and application name         ║");
    println!("║  configuration options for database connections                ║");
    println!("╚════════════════════════════════════════════════════════════════╝");
    println!();

    // Read configuration from environment variables
    let host = env::var("POSTGRES_HOST").unwrap_or_else(|_| "localhost".to_string());
    let port = env::var("POSTGRES_PORT").unwrap_or_else(|_| "5432".to_string());
    let user = env::var("POSTGRES_USER").unwrap_or_else(|_| "postgres".to_string());
    let password = env::var("POSTGRES_PASSWORD").unwrap_or_else(|_| "postgres".to_string());
    let database = env::var("POSTGRES_DB").unwrap_or_else(|_| "postgres".to_string());

    let connection_string = format!("postgres://{}:{}/{}", host, port, database);

    println!("📊 Configuration Examples\n");
    println!("Connection: {}@{}:{}/{}", user, host, port, database);
    println!();

    // Example 1: Simple configuration with default settings
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Example 1: Basic Connection (no special configuration)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
    println!("Code:");
    println!("  let client = FraiseClient::connect(&connection_string).await?;");
    println!();

    match FraiseClient::connect(&connection_string).await {
        Ok(_client) => println!("✓ Connected successfully with default configuration"),
        Err(e) => println!("✗ Connection failed: {}", e),
    }
    println!();

    // Example 2: Configuration with statement timeout
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Example 2: Configuration with Statement Timeout");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
    println!("Code:");
    println!("  let config = ConnectionConfig::builder(&database, &user)");
    println!("      .password(&password)");
    println!("      .statement_timeout(Duration::from_secs(30))");
    println!("      .build();");
    println!("  let client = FraiseClient::connect_with_config(&conn_string, config).await?;");
    println!();

    let config = ConnectionConfig::builder(&database, &user)
        .password(&password)
        .statement_timeout(Duration::from_secs(30))
        .build();

    println!("Configuration details:");
    println!("  - statement_timeout: 30 seconds");
    println!("    (Queries will be terminated if they exceed 30 seconds)");
    println!();

    match FraiseClient::connect_with_config(&connection_string, config).await {
        Ok(_client) => {
            println!("✓ Connected successfully with statement timeout configuration");
        }
        Err(e) => {
            println!("✗ Connection failed: {}", e);
        }
    }
    println!();

    // Example 3: Full configuration with multiple options
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Example 3: Full Configuration (All Options)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
    println!("Code:");
    println!("  let config = ConnectionConfig::builder(&database, &user)");
    println!("      .password(&password)");
    println!("      .statement_timeout(Duration::from_secs(60))");
    println!("      .keepalive_idle(Duration::from_secs(300))");
    println!("      .application_name(\"fraiseql_example\")");
    println!("      .extra_float_digits(2)");
    println!("      .build();");
    println!();

    let full_config = ConnectionConfig::builder(&database, &user)
        .password(&password)
        .statement_timeout(Duration::from_secs(60))
        .keepalive_idle(Duration::from_secs(300))
        .application_name("fraiseql_example")
        .extra_float_digits(2)
        .build();

    println!("Configuration details:");
    println!("  - statement_timeout: 60 seconds");
    println!("    (Queries terminate after 60 seconds of execution)");
    println!("  - keepalive_idle: 300 seconds (5 minutes)");
    println!("    (TCP keepalive probes sent every 5 minutes during idle)");
    println!("  - application_name: \"fraiseql_example\"");
    println!("    (Visible in PostgreSQL logs and pg_stat_activity)");
    println!("  - extra_float_digits: 2");
    println!("    (Increased precision for floating point values)");
    println!();

    match FraiseClient::connect_with_config(&connection_string, full_config).await {
        Ok(_client) => {
            println!("✓ Connected successfully with full configuration");
        }
        Err(e) => {
            println!("✗ Connection failed: {}", e);
        }
    }
    println!();

    // Example 4: Show configuration builder pattern
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Example 4: Builder Pattern (Fluent API)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
    println!("The ConnectionConfigBuilder uses a fluent API for easy chaining:");
    println!();

    let builder_config = ConnectionConfig::builder("mydb", "user")
        .password("secret")
        .statement_timeout(Duration::from_secs(45))
        .keepalive_idle(Duration::from_secs(600))
        .application_name("fluent_example")
        .param("timezone", "UTC")
        .build();

    println!("Code:");
    println!("  ConnectionConfig::builder(\"mydb\", \"user\")");
    println!("      .password(\"secret\")");
    println!("      .statement_timeout(Duration::from_secs(45))");
    println!("      .keepalive_idle(Duration::from_secs(600))");
    println!("      .application_name(\"fluent_example\")");
    println!("      .param(\"timezone\", \"UTC\")");
    println!("      .build()");
    println!();

    println!("Built configuration:");
    println!("  - database: {}", builder_config.database);
    println!("  - user: {}", builder_config.user);
    println!(
        "  - statement_timeout: {:?}",
        builder_config.statement_timeout
    );
    println!("  - keepalive_idle: {:?}", builder_config.keepalive_idle);
    println!(
        "  - application_name: {:?}",
        builder_config.application_name
    );
    println!(
        "  - custom param (timezone): {:?}",
        builder_config.params.get("timezone")
    );
    println!();

    // Example 5: Show timeout value conversions
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Example 5: Timeout Value Conversions");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
    println!("Timeouts are converted to milliseconds for PostgreSQL:");
    println!();

    let examples = vec![
        (Duration::from_secs(1), "1 second"),
        (Duration::from_millis(500), "500 milliseconds"),
        (
            Duration::from_secs(5) + Duration::from_millis(250),
            "5.25 seconds",
        ),
        (Duration::from_secs(30), "30 seconds"),
        (Duration::from_secs(300), "5 minutes"),
    ];

    for (duration, description) in examples {
        println!(
            "  {} ({:?}) → {} ms",
            description,
            duration,
            duration.as_millis()
        );
    }
    println!();

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Example 6: Using connect_with_config_and_tls");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
    println!("You can also combine configuration with TLS encryption:");
    println!();
    println!("Code:");
    println!("  let config = ConnectionConfig::builder(&database, &user)");
    println!("      .password(&password)");
    println!("      .statement_timeout(Duration::from_secs(30))");
    println!("      .build();");
    println!();
    println!("  let tls = TlsConfig::builder()");
    println!("      .verify_hostname(true)");
    println!("      .build()?;");
    println!();
    println!("  let client = FraiseClient::connect_with_config_and_tls(");
    println!("      connection_string,");
    println!("      config,");
    println!("      tls");
    println!("  ).await?;");
    println!();

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
    println!("✨ Configuration examples complete!");
    println!();
    println!("Key takeaways:");
    println!("  - Use ConnectionConfig::builder() for advanced options");
    println!("  - statement_timeout: Limits query execution time");
    println!("  - keepalive_idle: Prevents connection timeout on idle");
    println!("  - application_name: Identifies your app in PostgreSQL logs");
    println!("  - extra_float_digits: Controls floating point precision");
    println!("  - All options are optional; defaults work for most cases");
    println!();

    Ok(())
}

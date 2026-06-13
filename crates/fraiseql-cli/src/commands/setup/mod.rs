//! `fraiseql setup` - Install FraiseQL helpers to a PostgreSQL database.
//!
//! Installs SQL helper functions (`fraiseql.mutation_ok`, `fraiseql.mutation_err`,
//! etc.) to the target database. These helpers reduce boilerplate when writing
//! mutation functions under the v2.2.0 protocol.

use anyhow::{Context, Result};
use deadpool_postgres::{Config, ManagerConfig, RecyclingMethod, Runtime};
use tokio_postgres::NoTls;
use tracing::info;

use crate::output::OutputFormatter;

/// SQL helper library version (must match sql/helpers/mutation_response.sql)
const HELPERS_VERSION: &str = "2.2.0";

/// The SQL helper library content embedded as a const
const MUTATION_RESPONSE_SQL: &str = include_str!("../../../sql/helpers/mutation_response.sql");

/// Run the setup command to install helpers to a database.
///
/// # Errors
///
/// Returns an error if database connection fails, SQL execution fails,
/// or the URL cannot be resolved.
pub async fn run(
    database_url: Option<&str>,
    dry_run: bool,
    formatter: &OutputFormatter,
) -> Result<()> {
    if dry_run {
        // For dry-run, use provided URL or a placeholder
        let db_url = database_url.unwrap_or("postgres://user:pass@localhost/db");
        print_dry_run(db_url, formatter);
        return Ok(());
    }

    // Resolve database URL for actual execution
    let db_url = super::migrate::resolve_database_url(database_url)
        .context("Failed to resolve database URL")?;

    formatter.progress(&format!(
        "🔧 Installing FraiseQL mutation helpers (v{}) to database...",
        HELPERS_VERSION
    ));

    // Connect to database and get a pool
    let pool = connect_to_database(&db_url).await.context("Failed to connect to database")?;

    // Apply the SQL helpers
    apply_helpers(&pool, formatter).await.context("Failed to apply helpers")?;

    // Report success
    formatter.progress(&format!(
        "✅ FraiseQL mutation helpers v{} installed successfully",
        HELPERS_VERSION
    ));

    formatter.progress("Installed functions:");
    formatter.progress("  - fraiseql.library_version()");
    formatter.progress("  - fraiseql.mutation_ok(...)");
    formatter.progress("  - fraiseql.mutation_err(...)");

    Ok(())
}

/// Print what would be done (dry run mode)
fn print_dry_run(db_url: &str, formatter: &OutputFormatter) {
    formatter.progress("📋 DRY RUN MODE (no changes will be made)");
    formatter.progress("");
    formatter.progress(&format!("Database URL: {}", mask_password(db_url)));
    formatter.progress("");
    formatter.progress("The following SQL will be executed:");
    formatter.progress("");
    formatter.progress(MUTATION_RESPONSE_SQL);
    formatter.progress("");
    formatter.progress("To apply these changes, run without --dry-run:");
    formatter.progress(&format!("  fraiseql setup --database '{}'", mask_password(db_url)));
}

/// Mask sensitive parts of database URL for display
fn mask_password(url: &str) -> String {
    if let Some(at_pos) = url.rfind('@') {
        if let Some(colon_pos) = url[..at_pos].rfind(':') {
            let before = &url[..=colon_pos];
            let after = &url[at_pos..];
            format!("{}***{}", before, after)
        } else {
            url.to_string()
        }
    } else {
        url.to_string()
    }
}

/// Connect to the database using a deadpool connection pool
async fn connect_to_database(db_url: &str) -> Result<deadpool_postgres::Pool> {
    // Create deadpool config
    let mut cfg = Config::new();
    cfg.url = Some(db_url.to_string());
    cfg.manager = Some(ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    });
    cfg.pool = Some(deadpool_postgres::PoolConfig::new(2));

    // Create connection pool
    let pool = cfg
        .create_pool(Some(Runtime::Tokio1), NoTls)
        .context("Failed to create database pool")?;

    // Test connection
    let _client = pool.get().await.context("Failed to acquire database connection")?;

    info!("Connected to database");

    Ok(pool)
}

/// Apply the SQL helpers to the database
async fn apply_helpers(pool: &deadpool_postgres::Pool, formatter: &OutputFormatter) -> Result<()> {
    formatter.progress("📝 Applying SQL helpers...");

    // Get a client from the pool
    let client = pool.get().await.context("Failed to acquire database connection")?;

    // Execute the whole helper library as a single simple-query batch. The file
    // defines dollar-quoted PL/pgSQL function bodies (`$$ … ; … $$`) and a
    // trailing `DO`-block self-test, so it CANNOT be split on `;` — doing so
    // shreds the function bodies and installs nothing (#426). `batch_execute`
    // uses the simple-query protocol, which understands dollar-quoting and
    // multi-statement scripts (the same way `psql -f` runs the file).
    client
        .batch_execute(MUTATION_RESPONSE_SQL)
        .await
        .context("Failed to install FraiseQL helper library")?;

    formatter.progress("✓ SQL helpers applied");

    // Verify installation
    let version: String = client
        .query_one("SELECT fraiseql.library_version() AS version", &[])
        .await
        .context("Failed to verify helper installation")?
        .get("version");

    if version == HELPERS_VERSION {
        info!("Helper version verified: {}", version);
    } else {
        // This is a soft warning, not a hard failure
        formatter.progress(&format!(
            "⚠️  Version mismatch: expected {}, got {}",
            HELPERS_VERSION, version
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests;

//! `fraiseql run --admin-database-url`: create and grant the server's roles in development.
//!
//! The engine connects as the user its database URL names, and first-party auth as the user
//! the variable `[auth.database] url_env` names. On a fresh development database neither role
//! exists. With an admin URL, `fraiseql run` creates each one before the server starts:
//!
//! - a role that does not exist is created `LOGIN NOSUPERUSER`, with its URL's password;
//! - a role that exists keeps every attribute it has: only its password is set, from its URL;
//! - each role is granted `CONNECT` and `CREATE` on the database, and the engine's role `CREATE` on
//!   schema `public`, which PostgreSQL 15 and later no longer grant to every role;
//! - a URL whose user is the admin URL's user is left alone, so a development setup that runs
//!   everything as one superuser still works.
//!
//! Only in development: provisioning a role from a URL is a convenience no production
//! deployment should depend on, so outside `FRAISEQL_ENV=development` the flag is refused.
//! No URL and no password is printed, logged or carried in an error.

use anyhow::{Context, Result};
use fraiseql_core::schema::CompiledSchema;
use fraiseql_db::postgres::PostgresTlsConfig;
use tokio_postgres::config::Host;

/// The flag, as refusals name it.
pub const FLAG: &str = "--admin-database-url";

/// The environment variable the flag falls back to.
pub const ENV_VAR: &str = "FRAISEQL_ADMIN_DATABASE_URL";

/// The admin URL from the flag, or else from [`ENV_VAR`]; `None` when neither sets one.
pub fn admin_url(flag: Option<String>) -> Option<String> {
    flag.or_else(|| std::env::var(ENV_VAR).ok()).filter(|url| !url.is_empty())
}

/// Refuse the flag outside development, before anything connects.
///
/// # Errors
///
/// Returns an error naming the flag and `FRAISEQL_ENV` when the process is not declared a
/// development one.
pub fn refuse_outside_development() -> Result<()> {
    if fraiseql_server::ServerConfig::is_production_mode() {
        anyhow::bail!(
            "{FLAG} ({ENV_VAR}) creates database roles, and only in development: set \
             FRAISEQL_ENV=development, or create the roles yourself (see docs/auth/database.md)"
        );
    }
    Ok(())
}

/// A database a URL names: its hosts, ports and database.
#[derive(Debug, PartialEq, Eq)]
struct Place {
    hosts:    Vec<String>,
    ports:    Vec<u16>,
    database: String,
}

/// The user and password a URL names, and where.
struct Login {
    user:     String,
    password: Option<String>,
    place:    Place,
}

/// Parse `url`, which `source` names in any refusal. The value itself is never echoed.
fn login(url: &str, source: &str) -> Result<Login> {
    let config: tokio_postgres::Config = url
        .parse()
        .map_err(|_| anyhow::anyhow!("{source} is not a PostgreSQL connection string"))?;
    let user = config
        .get_user()
        .filter(|user| !user.is_empty())
        .ok_or_else(|| anyhow::anyhow!("{source} names no user, so {FLAG} has no role to create"))?
        .to_string();
    let password = config
        .get_password()
        .map(|bytes| {
            String::from_utf8(bytes.to_vec())
                .map_err(|_| anyhow::anyhow!("{source}'s password is not UTF-8"))
        })
        .transpose()?;
    let hosts = config
        .get_hosts()
        .iter()
        .map(|host| match host {
            Host::Tcp(name) => name.clone(),
            Host::Unix(path) => path.display().to_string(),
        })
        .collect();
    let ports = match config.get_ports() {
        [] => vec![5432],
        ports => ports.to_vec(),
    };
    // libpq's default: the database named like the user.
    let database = config.get_dbname().unwrap_or(&user).to_string();
    Ok(Login {
        user,
        password,
        place: Place {
            hosts,
            ports,
            database,
        },
    })
}

/// One role to provision, and what it is for.
struct Target {
    login:  Login,
    source: String,
    engine: bool,
}

/// Create and grant the engine's role, from `engine_url`, and auth's role, from the variable
/// `schema`'s `[auth.database] url_env` names, connected as `admin_url`.
///
/// `engine_source` names where the engine URL came from, for refusals.
///
/// # Errors
///
/// Refuses an admin URL that names another database than a target, two URLs naming one user
/// with different passwords, and a URL it cannot read; returns any error the database
/// raises while provisioning.
pub async fn provision(
    admin_url: &str,
    engine_url: &str,
    engine_source: &str,
    schema: &CompiledSchema,
) -> Result<()> {
    let admin = login(admin_url, FLAG)?;
    let mut targets = vec![Target {
        login:  login(engine_url, engine_source)?,
        source: engine_source.to_string(),
        engine: true,
    }];
    let auth_env = schema
        .auth
        .as_ref()
        .and_then(|auth| auth.database.as_ref())
        .map(|db| &db.url_env);
    if let Some(name) = auth_env {
        // Unset or empty, the server's boot refuses it, naming the variable.
        if let Some(url) = std::env::var(name).ok().filter(|url| !url.is_empty()) {
            targets.push(Target {
                login:  login(&url, name)?,
                source: name.clone(),
                engine: false,
            });
        }
    }
    for target in &targets {
        if target.login.place != admin.place {
            anyhow::bail!(
                "{FLAG} must name the same database as {}: host, port and database. It creates \
                 roles for the database the server connects to, and no other",
                target.source
            );
        }
    }
    let targets = merge_same_user(targets)?;

    let pool = crate::connection::postgres_pool(
        admin_url,
        "provisioning roles (--admin-database-url)",
        &PostgresTlsConfig::default(),
    )
    .await?;
    let client = pool.get().await.context("no connection for provisioning roles")?;
    for target in targets.iter().filter(|target| target.login.user != admin.user) {
        provision_role(&client, target, &admin.place.database).await?;
        let what = if target.engine {
            "the engine"
        } else {
            "first-party auth"
        };
        println!("   Role {} ready for {what}", target.login.user);
    }
    Ok(())
}

/// One target per user. The engine's and auth's URL may name one role (the single-role
/// layout); it is provisioned once, with the engine's grants.
fn merge_same_user(targets: Vec<Target>) -> Result<Vec<Target>> {
    let mut merged: Vec<Target> = Vec::new();
    for target in targets {
        if let Some(seen) = merged.iter_mut().find(|seen| seen.login.user == target.login.user) {
            if seen.login.password != target.login.password {
                anyhow::bail!(
                    "{} and {} name the same user with different passwords",
                    seen.source,
                    target.source
                );
            }
            seen.engine |= target.engine;
        } else {
            merged.push(target);
        }
    }
    Ok(merged)
}

/// Create `target`'s role when it is absent, or set only its password when it exists, then
/// grant it what the server needs on `database`.
async fn provision_role(
    client: &deadpool_postgres::Object,
    target: &Target,
    database: &str,
) -> Result<()> {
    let role = target.login.user.as_str();
    let password = target.login.password.as_deref();
    let exists: bool = client
        .query_one("SELECT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = $1)", &[&role])
        .await
        .context("could not read pg_roles")?
        .get(0);
    // PostgreSQL quotes the role, the password and the database itself (`format` with `%I` and
    // `%L`): role statements take no bind parameters.
    let mut statements = Vec::new();
    if !exists {
        statements.push(
            quoted(client, "SELECT format('CREATE ROLE %I LOGIN NOSUPERUSER', $1::text)", &[&role])
                .await?,
        );
    }
    if let Some(password) = password {
        statements.push(
            quoted(
                client,
                "SELECT format('ALTER ROLE %I PASSWORD %L', $1::text, $2::text)",
                &[&role, &password],
            )
            .await?,
        );
    }
    statements.push(
        quoted(
            client,
            "SELECT format('GRANT CONNECT, CREATE ON DATABASE %I TO %I', $1::text, $2::text)",
            &[&database, &role],
        )
        .await?,
    );
    if target.engine {
        statements.push(
            quoted(
                client,
                "SELECT format('GRANT CREATE ON SCHEMA public TO %I', $1::text)",
                &[&role],
            )
            .await?,
        );
    }
    for statement in statements {
        // The error names the role, never the statement: it may carry the password.
        client
            .batch_execute(&statement)
            .await
            .map_err(|e| anyhow::anyhow!("could not provision role {role}: {}", db_message(&e)))?;
    }
    Ok(())
}

/// A statement PostgreSQL built from `format`, with its arguments quoted.
async fn quoted(
    client: &deadpool_postgres::Object,
    format: &str,
    args: &[&(dyn tokio_postgres::types::ToSql + Sync)],
) -> Result<String> {
    Ok(client
        .query_one(format, args)
        .await
        .map_err(|e| anyhow::anyhow!("could not build a role statement: {}", db_message(&e)))?
        .get(0))
}

/// The database's own message, without the statement it refused.
fn db_message(error: &tokio_postgres::Error) -> String {
    error
        .as_db_error()
        .map_or_else(|| error.to_string(), |db| db.message().to_string())
}

#[cfg(test)]
#[path = "run_provision_tests.rs"]
mod tests;

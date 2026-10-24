//! The boot provisioning table: every DDL script a configured subsystem needs in place
//! before the router mounts it, in one ordered list.
//!
//! Each subsystem used to provision itself with its own `run_migration` call and its own
//! error mapping, so the session schema ran up to five times per boot and a database that
//! refused the DDL read as a configuration mistake. The table names each step once, runs
//! a script the first time a step needs it, and maps every failure to
//! [`ServerError::Database`].

use std::collections::HashSet;

use tracing::info;

use super::{Result, Server, ServerError};
use crate::capability::REGISTRY;

/// What a capability reads to decide which provisioning steps it needs.
#[allow(dead_code)] // Reason: read by the impls of compiled-in capabilities; a build with none reads none
pub struct BootCx<'a> {
    /// The server about to be served.
    pub server: &'a Server,
}

/// One boot provisioning step: the scripts one configured subsystem needs.
pub struct ProvisionStep {
    /// The subsystem, as the boot log and the error name it.
    pub(super) name:    &'static str,
    /// The pool the scripts run against.
    pub(super) pool:    sqlx::PgPool,
    /// The DDL, in order. Each script is idempotent.
    pub(super) scripts: Vec<&'static str>,
}

impl ProvisionStep {
    /// A step running `scripts` against `pool`. The boot-DDL gate accepts a schema
    /// constant here and in a `run_migration` argument, nowhere else.
    pub fn new(name: &'static str, pool: &sqlx::PgPool, scripts: &[&'static str]) -> Self {
        Self {
            name,
            pool: pool.clone(),
            scripts: scripts.to_vec(),
        }
    }
}

/// Drop the scripts an earlier step already runs, keeping each step's order.
///
/// Every step's pool is a clone of the one pool the server was constructed with, so a
/// script that ran once has run for every step that names it.
pub(super) fn without_repeats(steps: Vec<ProvisionStep>) -> Vec<ProvisionStep> {
    let mut seen = HashSet::new();
    steps
        .into_iter()
        .map(|mut step| {
            step.scripts.retain(|script| seen.insert(*script));
            step
        })
        .filter(|step| !step.scripts.is_empty())
        .collect()
}

/// Run each step under the migration lock, in order.
///
/// # Errors
///
/// Returns [`ServerError::Database`] naming the first step whose DDL fails.
pub(super) async fn run(steps: Vec<ProvisionStep>) -> Result<()> {
    for step in without_repeats(steps) {
        crate::migration_lock::run_migrations(&step.pool, &step.scripts)
            .await
            .map_err(|e| {
                ServerError::Database(format!("Failed to provision the {} schema: {e}", step.name))
            })?;
        info!(step = step.name, scripts = step.scripts.len(), "Boot schema ready");
    }
    Ok(())
}

impl Server {
    /// The provisioning steps this server's configuration needs, in boot order.
    pub(super) fn provision_steps(&self) -> Vec<ProvisionStep> {
        let mut steps = Vec::new();

        // Role assignments (#1632): before the router mounts a writer of them (the RBAC
        // API, SCIM group sync) or a request reads them.
        if let Some(rbac) = self.rbac.as_ref() {
            steps.push(ProvisionStep::new(
                "RBAC",
                rbac.pool(),
                &[crate::api::rbac_management::db_backend::RBAC_SCHEMA_SQL],
            ));
        }

        // Tenant admin credentials (#1089): the table the admin gate reads.
        if let Some(tokens) = self.admin_tokens.as_ref() {
            steps.push(ProvisionStep::new(
                "admin-token",
                tokens.pool(),
                &[crate::api::admin_principal::SCHEMA_SQL],
            ));
        }

        // Postgres-stored API keys (#627): a configured store whose DDL cannot run must
        // refuse to serve, not authenticate nothing.
        if let Some(store) = self.api_key_authenticator.as_ref().and_then(|a| a.postgres_store()) {
            steps.push(ProvisionStep::new(
                "API-key",
                store.pool(),
                &[crate::api_key::postgres::PG_API_KEY_SCHEMA_SQL],
            ));
        }

        #[cfg(feature = "auth")]
        self.push_auth_steps(&mut steps);

        // Durable async operations (#391): the table submissions are written to.
        if let (Some(_), Some(pool)) = (self.async_operations.as_ref(), self.db_pool.as_ref()) {
            steps.push(ProvisionStep::new(
                "async operations",
                pool,
                &[crate::async_operations::store::ASYNC_OPERATIONS_SCHEMA_SQL],
            ));
        }

        // Then every compiled-in capability's own steps, in registry order.
        let cx = BootCx { server: self };
        for registration in REGISTRY.iter().filter(|registration| registration.compiled_in) {
            steps.extend(registration.capability.provision(&cx));
        }

        steps
    }

    /// The identity steps: every login surface mints a Postgres session and resolves an
    /// account, so a missing table would 500 the first login instead of failing the boot.
    #[cfg(feature = "auth")]
    fn push_auth_steps(&self, steps: &mut Vec<ProvisionStep>) {
        use fraiseql_auth::{account_linking, session_postgres::SESSIONS_SCHEMA_SQL};

        let Some(pool) = self.enrichment_pool.as_ref() else {
            return;
        };

        // /auth/saml/acs (#381).
        #[cfg(feature = "auth-saml")]
        if self.saml_state.is_some() {
            steps.push(ProvisionStep::new(
                "SAML login",
                pool,
                &[SESSIONS_SCHEMA_SQL, account_linking::SCHEMA_SQL],
            ));
        }

        // SCIM provisioning (#946). Deactivation revokes through the session store, so the
        // session table is needed even where no login route is mounted: otherwise only the
        // "block new sessions" half of offboarding works.
        if self.config.scim.as_ref().is_some_and(|scim| scim.enabled) {
            steps.push(ProvisionStep::new(
                "SCIM",
                pool,
                &[
                    account_linking::SCHEMA_SQL,
                    SESSIONS_SCHEMA_SQL,
                    fraiseql_auth::scim::PG_SCIM_SCHEMA_SQL,
                ],
            ));
        }

        // [auth.local] (#367).
        if self.otp_state.is_some() {
            steps.push(ProvisionStep::new(
                "[auth.local] OTP",
                pool,
                &[
                    SESSIONS_SCHEMA_SQL,
                    account_linking::SCHEMA_SQL,
                    fraiseql_auth::PG_OTP_SCHEMA_SQL,
                ],
            ));
        }
        if self.mfa_state.is_some() {
            steps.push(ProvisionStep::new(
                "[auth.local] MFA",
                pool,
                &[
                    SESSIONS_SCHEMA_SQL,
                    account_linking::SCHEMA_SQL,
                    fraiseql_auth::PG_MFA_SCHEMA_SQL,
                ],
            ));
        }
        if self.local_password_state.is_some() {
            // The DDL `LocalPasswordAuthenticator::init` would run unlocked.
            steps.push(ProvisionStep::new(
                "[auth.local] password",
                pool,
                &[
                    account_linking::SCHEMA_SQL,
                    fraiseql_auth::PASSWORD_SCHEMA_SQL,
                    fraiseql_auth::PASSWORD_RESET_SCHEMA_SQL,
                    fraiseql_auth::EMAIL_VERIFICATION_SCHEMA_SQL,
                    SESSIONS_SCHEMA_SQL,
                ],
            ));
        }
        if self.anon_signup_state.is_some() {
            steps.push(ProvisionStep::new("[auth.local] anonymous", pool, &[SESSIONS_SCHEMA_SQL]));
        }

        // /auth/v1/callback (#368).
        if self.social_login.is_some() {
            steps.push(ProvisionStep::new(
                "social login",
                pool,
                &[SESSIONS_SCHEMA_SQL, account_linking::SCHEMA_SQL],
            ));
        }
    }
}

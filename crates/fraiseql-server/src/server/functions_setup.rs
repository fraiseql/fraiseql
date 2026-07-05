//! Serve-time functions-runtime preparation.
//!
//! Loads the compiled schema's function modules, registers the runtimes, attaches
//! the `send_email` wiring, and stores the before-mutation hooks so
//! `build_app_state` mounts them and after:mutation functions actually fire. Runs
//! once at serve time (async, fail-loud) — the counterpart to the RBAC/inbound
//! schema init already in the serve path.

use std::sync::Arc;

use fraiseql_core::db::traits::DatabaseAdapter;

use super::{Server, ServerError};
use crate::{schema::loader::CompiledSchemaLoader, subsystems::loader::build_functions_subsystem};

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> Server<A> {
    /// Prepare functions-runtime dispatch from the compiled schema.
    ///
    /// Loads the extended schema's functions config; when it declares functions,
    /// builds the subsystem (modules loaded from `module_dir`, runtimes
    /// registered), attaches the `send_email` wiring, and stores the resulting
    /// hooks. A no-op (hooks stay `None`) when no functions are declared.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::ConfigError`] if the schema cannot be loaded or a
    /// declared function's module is missing/unreadable (fail-loud: a declared
    /// function that can never run is a misconfiguration).
    pub(super) async fn prepare_functions_runtime(&mut self) -> Result<(), ServerError> {
        let extended = CompiledSchemaLoader::new(&self.config.schema_path)
            .load_extended()
            .await
            .map_err(|error| {
                ServerError::ConfigError(format!("failed to load functions config: {error}"))
            })?;

        let Some(functions_config) = extended.functions else {
            return Ok(()); // no `functions` in the compiled schema
        };
        if functions_config.definitions.is_empty() {
            return Ok(());
        }

        let subsystem = build_functions_subsystem(functions_config).map_err(|error| {
            ServerError::ConfigError(format!("functions-runtime setup failed: {error}"))
        })?;

        // Sign the per-dispatch idempotency token when an HMAC secret is
        // configured; unsigned digest otherwise (zero-config default).
        let idempotency_key = self.build_idempotency_key();
        if idempotency_key.is_some() {
            tracing::info!("idempotency tokens are HMAC-signed (VERP-ready send-ids)");
        }
        let mut hooks =
            subsystem.into_before_mutation_hooks().with_idempotency_key(idempotency_key);

        if let Some((resolver, transport)) = self.build_send_email_wiring() {
            hooks = hooks.with_email(resolver, transport);
            tracing::info!(
                "send_email host op enabled (host-owned from, per-connected-account SMTP)"
            );
        }

        let function_count = hooks.module_registry.len();
        self.functions_hooks = Some(Arc::new(hooks));
        tracing::info!(functions = function_count, "functions-runtime dispatch enabled");
        Ok(())
    }

    /// Derive the idempotency-token HMAC subkey from the configured server HMAC
    /// secret (`hmac_secret_env` names an env var). `None` → the token stays an
    /// unsigned digest (the zero-config default); a signed token is required before
    /// it is exposed externally as a VERP Return-Path (P04b). A configured-but-empty
    /// secret is a misconfiguration surfaced loudly, not silently signed with "".
    fn build_idempotency_key(&self) -> Option<Arc<[u8]>> {
        let env_name = self.config.hmac_secret_env.as_deref()?;
        if let Some(secret) = std::env::var(env_name).ok().filter(|secret| !secret.is_empty()) {
            let subkey = fraiseql_observers::derive_idempotency_subkey(secret.as_bytes());
            Some(Arc::from(subkey.as_slice()))
        } else {
            tracing::warn!(
                env = env_name,
                "hmac_secret_env is set but the environment variable is empty/unset — \
                 idempotency tokens stay unsigned and VERP send-correlation is disabled"
            );
            None
        }
    }

    /// Build the `send_email` wiring — sender-identity resolver + SMTP transport —
    /// from config. Returns `None` when no SMTP mailbox is configured, leaving
    /// `send_email` fail-loud.
    fn build_send_email_wiring(
        &self,
    ) -> Option<(
        Arc<dyn fraiseql_functions::SenderIdentityResolver>,
        Arc<dyn fraiseql_functions::EmailTransport>,
    )> {
        #[cfg(feature = "inbound-email")]
        {
            let transport =
                crate::inbound::email::build_email_transport(&self.config.mailbox, |name| {
                    std::env::var(name).ok()
                })?;
            Some((self.build_sender_resolver(), transport))
        }
        #[cfg(not(feature = "inbound-email"))]
        {
            None
        }
    }

    /// The sender-identity resolver: DB-backed on the shared identity primitive
    /// when `[identity.sender]` is enabled, else the login-email default.
    #[cfg(feature = "inbound-email")]
    fn build_sender_resolver(&self) -> Arc<dyn fraiseql_functions::SenderIdentityResolver> {
        #[cfg(feature = "auth")]
        if let Some(sender) =
            self.config.identity.as_ref().and_then(|identity| identity.sender.as_ref())
        {
            if sender.enabled {
                if let Some(pool) = self.enrichment_pool.as_ref() {
                    let resolver = crate::identity::resolver::IdentityResolver::postgres(
                        sender.clone(),
                        pool.clone(),
                    );
                    return Arc::new(crate::identity::sender::DbSenderIdentityResolver::new(
                        resolver,
                        SENDING_ADDRESS_FIELD,
                        Some(DISPLAY_NAME_FIELD.to_string()),
                    ));
                }
                tracing::warn!(
                    "[identity.sender] is enabled but no auth database pool is available — \
                     send_email falls back to the login-email sender"
                );
            }
        }
        Arc::new(fraiseql_functions::LoginEmailSender)
    }
}

/// The resolved sender query's field holding the verified from-address (convention,
/// matching `docs/architecture/enriched-identity-rls.md`).
#[cfg(all(feature = "inbound-email", feature = "auth"))]
const SENDING_ADDRESS_FIELD: &str = "sending_address";

/// The resolved sender query's field holding the sender display name.
#[cfg(all(feature = "inbound-email", feature = "auth"))]
const DISPLAY_NAME_FIELD: &str = "display_name";

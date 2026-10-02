//! Tenant key resolution from HTTP request context.
//!
//! An authenticated request is served the tenant its token is bound to
//! (`SecurityContext::tenant_id`); client headers may only agree with it. An anonymous
//! request is addressed by `X-Tenant-ID`, then `Host` (via `DomainRegistry`).
//!
//! The resolver only **extracts and validates** the key format. It does NOT check
//! whether the key is registered — that validation happens in
//! [`TenantExecutorRegistry::executor_for`](super::tenant_registry::TenantExecutorRegistry::executor_for).

use axum::http::HeaderMap;
use dashmap::DashMap;
use fraiseql_core::security::SecurityContext;
use fraiseql_error::{FraiseQLError, Result};
use tracing::warn;

/// Maximum length of a tenant key accepted from the `X-Tenant-ID` header.
///
/// Derived from the schema-isolation limit so the header validator and the
/// schema-mode DDL helpers agree on a single cap (#333): a schema-mode tenant
/// produces the PostgreSQL identifier `tenant_{key}`, which must fit in
/// [`MAX_PG_IDENTIFIER_LEN`](crate::tenancy::schema_isolation::MAX_PG_IDENTIFIER_LEN)
/// (63), leaving `63 - "tenant_".len()` = 56 usable characters.
pub(crate) const MAX_TENANT_KEY_LEN: usize = crate::tenancy::schema_isolation::MAX_PG_IDENTIFIER_LEN
    - crate::tenancy::schema_isolation::TENANT_SCHEMA_PREFIX.len();

/// Resolves the tenant key from an incoming HTTP request.
pub struct TenantKeyResolver;

impl TenantKeyResolver {
    /// Resolve and validate a tenant key from request context.
    ///
    /// **An authenticated request is served its token's tenant**
    /// (`SecurityContext::tenant_id`, derived from the configured tenant claim). The
    /// `X-Tenant-ID` and `Host` headers are client-controlled: for an authenticated caller
    /// they may only agree with the token. One that names a different tenant is refused,
    /// and so is one sent with a token that binds no tenant at all — otherwise a header
    /// would choose which tenant's executor a principal runs against
    /// (GHSA-24pq-hx78-766q). With no header, a token binding no tenant is served by the
    /// default executor.
    ///
    /// **An anonymous request** is addressed by its client hints alone: `X-Tenant-ID`,
    /// then `Host` through the domain registry (public per-tenant surfaces). When
    /// `strict` is true, two hints that disagree are refused.
    ///
    /// # Errors
    ///
    /// - `FraiseQLError::Authorization` when an authenticated request's header or `Host` names a
    ///   tenant its token is not bound to.
    /// - `FraiseQLError::Validation` if the `X-Tenant-ID` header contains invalid characters or
    ///   exceeds `MAX_TENANT_KEY_LEN`, or if `strict` is true and an anonymous request's hints
    ///   conflict.
    #[doc(hidden)] // Internal-pub: dispatched by GraphQL handler/subscription routes; downstream tenancy goes through TenancyConfig, not this fn.
    pub fn resolve(
        security_context: Option<&SecurityContext>,
        headers: &HeaderMap,
        domain_registry: Option<&DomainRegistry>,
        strict: bool,
    ) -> Result<Option<String>> {
        let hints = Self::client_hints(headers, domain_registry)?;

        if let Some(ctx) = security_context {
            let bound = ctx.tenant_id.as_ref().map(|t| t.0.as_str());
            if let Some((source, named)) =
                hints.iter().find(|(_, named)| Some(named.as_str()) != bound)
            {
                warn!(
                    source,
                    named, "authenticated request names a tenant its token does not bind"
                );
                return Err(FraiseQLError::unauthorized(format!(
                    "the request names tenant '{named}' ({source}), which the caller's token \
                     is not bound to"
                )));
            }
            return Ok(bound.map(str::to_string));
        }

        if let [(first, a), rest @ ..] = hints.as_slice() {
            if let Some((second, b)) = rest.iter().find(|(_, b)| b != a) {
                warn!("Tenant source conflict detected: {first}: {a}, {second}: {b}");
                if strict {
                    return Err(FraiseQLError::Validation {
                        message: format!(
                            "Conflicting tenant values from sources: {first}: {a}, {second}: {b}"
                        ),
                        path:    None,
                    });
                }
            }
        }
        Ok(hints.into_iter().next().map(|(_, key)| key))
    }

    /// The tenants a request's client-controlled headers name, in priority order:
    /// `X-Tenant-ID` (format-validated), then `Host` through the domain registry.
    fn client_hints(
        headers: &HeaderMap,
        domain_registry: Option<&DomainRegistry>,
    ) -> Result<Vec<(&'static str, String)>> {
        let mut hints = Vec::new();
        if let Some(value) = headers.get("X-Tenant-ID").and_then(|v| v.to_str().ok()) {
            validate_tenant_key(value)?;
            hints.push(("X-Tenant-ID", value.to_string()));
        }
        if let (Some(registry), Some(host)) =
            (domain_registry, headers.get("Host").and_then(|v| v.to_str().ok()))
        {
            if let Some(key) = registry.lookup(host) {
                hints.push(("Host", key));
            }
        }
        Ok(hints)
    }
}

/// Validate that a tenant key from the `X-Tenant-ID` header is safe.
///
/// The accepted alphabet (`[a-zA-Z0-9_]`) and length cap ([`MAX_TENANT_KEY_LEN`])
/// match the schema-mode DDL helpers in
/// [`crate::tenancy::schema_isolation`] so a key accepted here is also usable
/// for schema-mode provisioning (#333). Hyphens, previously accepted, are now
/// rejected because PostgreSQL schema identifiers cannot contain them.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if the key is too long or contains
/// characters outside `[a-zA-Z0-9_]`.
pub(crate) fn validate_tenant_key(key: &str) -> Result<()> {
    if key.len() > MAX_TENANT_KEY_LEN {
        return Err(FraiseQLError::validation(format!(
            "X-Tenant-ID exceeds maximum length of {MAX_TENANT_KEY_LEN} characters"
        )));
    }
    if !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return Err(FraiseQLError::validation(
            "X-Tenant-ID contains invalid characters (allowed: a-zA-Z0-9_)",
        ));
    }
    Ok(())
}

/// Maps custom domains to tenant keys.
///
/// Thread-safe via `DashMap` — concurrent reads and writes without external locking.
pub struct DomainRegistry {
    domains: DashMap<String, String>,
}

impl DomainRegistry {
    /// Create an empty domain registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            domains: DashMap::new(),
        }
    }

    /// Register a domain → tenant key mapping.
    pub fn register(&self, domain: impl Into<String>, tenant_key: impl Into<String>) {
        self.domains.insert(domain.into(), tenant_key.into());
    }

    /// Remove a domain mapping. Returns `true` if the domain was registered.
    #[must_use]
    pub fn remove(&self, domain: &str) -> bool {
        self.domains.remove(domain).is_some()
    }

    /// Lookup tenant key by domain.
    ///
    /// Strips the port from the `Host` header value before lookup
    /// (e.g. `"api.acme.com:8080"` → `"api.acme.com"`).
    #[must_use]
    pub fn lookup(&self, host: &str) -> Option<String> {
        let domain = host.split(':').next().unwrap_or(host);
        self.domains.get(domain).map(|v| v.clone())
    }

    /// List all registered domain → tenant key mappings.
    #[must_use]
    pub fn domains(&self) -> Vec<(String, String)> {
        self.domains.iter().map(|e| (e.key().clone(), e.value().clone())).collect()
    }

    /// Number of registered domains.
    #[must_use]
    pub fn len(&self) -> usize {
        self.domains.len()
    }

    /// Whether the registry has no domains.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.domains.is_empty()
    }
}

impl Default for DomainRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;

//! Tenant key resolution from HTTP request context.
//!
//! Resolves tenant key from three sources in priority order:
//! 1. JWT `tenant_id` claim (via `SecurityContext`)
//! 2. `X-Tenant-ID` header
//! 3. `Host` header (via `DomainRegistry`)
//!
//! The resolver only **extracts and validates** the key format. It does NOT check
//! whether the key is registered — that validation happens in
//! [`TenantExecutorRegistry::executor_for`](super::tenant_registry::TenantExecutorRegistry::executor_for).

use axum::http::HeaderMap;
use dashmap::DashMap;
use fraiseql_core::security::SecurityContext;
use fraiseql_error::{FraiseQLError, Result};
use std::collections::HashSet;
use tracing::warn;

/// Maximum length for a tenant key from the `X-Tenant-ID` header.
pub(crate) const MAX_TENANT_KEY_LEN: usize = 128;

/// Resolves the tenant key from an incoming HTTP request.
pub struct TenantKeyResolver;

impl TenantKeyResolver {
    /// Resolve and validate a tenant key from request context.
    ///
    /// Priority: JWT `tenant_id` > `X-Tenant-ID` header > `Host` header.
    ///
    /// JWT values are trusted (already validated by token verification).
    /// `X-Tenant-ID` header values are validated for format safety.
    /// Cross-validates all available sources for consistency when `strict` is true.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if the `X-Tenant-ID` header value
    /// contains invalid characters, exceeds [`MAX_TENANT_KEY_LEN`], or if
    /// `strict` is true and multiple sources provide conflicting tenant values.
    pub fn resolve(
        security_context: Option<&SecurityContext>,
        headers: &HeaderMap,
        domain_registry: Option<&DomainRegistry>,
        strict: bool,
    ) -> Result<Option<String>> {
        let mut sources = Vec::new();
        let mut resolved_value = None;

        // 1. JWT tenant_id (highest priority, trusted)
        if let Some(ctx) = security_context {
            if let Some(ref tid) = ctx.tenant_id {
                resolved_value = Some(tid.0.clone());
                sources.push(("JWT".to_string(), tid.0.clone()));
            }
        }

        // 2. X-Tenant-ID header (untrusted, must validate)
        if let Some(val) = headers.get("X-Tenant-ID") {
            if let Ok(s) = val.to_str() {
                validate_tenant_key(s)?;
                let header_value = s.to_string();
                sources.push(("X-Tenant-ID".to_string(), header_value.clone()));
                if resolved_value.is_none() {
                    resolved_value = Some(header_value);
                }
            }
        }

        // 3. Host header → domain registry lookup
        if let Some(registry) = domain_registry {
            if let Some(val) = headers.get("Host") {
                if let Ok(host) = val.to_str() {
                    if let Some(key) = registry.lookup(host) {
                        sources.push(("Host".to_string(), key.clone()));
                        if resolved_value.is_none() {
                            resolved_value = Some(key);
                        }
                    }
                }
            }
        }

        // Cross-validate sources
        if sources.len() > 1 {
            let unique_values: HashSet<_> = sources.iter().map(|(_, v)| v).collect();
            if unique_values.len() > 1 {
                let conflicts: Vec<String> = sources.iter().map(|(src, val)| format!("{}: {}", src, val)).collect();
                warn!("Tenant source conflict detected: {}", conflicts.join(", "));
                if strict {
                    return Err(FraiseQLError::Validation {
                        message: format!("Conflicting tenant values from sources: {}", conflicts.join(", ")),
                        path: None,
                    });
                }
            }
        }

        Ok(resolved_value)
    }
}

/// Validate that a tenant key from the `X-Tenant-ID` header is safe.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if the key is too long or contains
/// characters outside `[a-zA-Z0-9_-]`.
fn validate_tenant_key(key: &str) -> Result<()> {
    if key.len() > MAX_TENANT_KEY_LEN {
        return Err(FraiseQLError::validation(format!(
            "X-Tenant-ID exceeds maximum length of {MAX_TENANT_KEY_LEN} characters"
        )));
    }
    if !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
        return Err(FraiseQLError::validation(
            "X-Tenant-ID contains invalid characters (allowed: a-zA-Z0-9_-)",
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

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

/// Maximum length for a tenant key from the `X-Tenant-ID` header.
const MAX_TENANT_KEY_LEN: usize = 128;

/// Resolves the tenant key from an incoming HTTP request.
pub struct TenantKeyResolver;

impl TenantKeyResolver {
    /// Resolve and validate a tenant key from request context.
    ///
    /// Priority: JWT `tenant_id` > `X-Tenant-ID` header > `Host` header.
    ///
    /// JWT values are trusted (already validated by token verification).
    /// `X-Tenant-ID` header values are validated for format safety.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if the `X-Tenant-ID` header value
    /// contains invalid characters or exceeds [`MAX_TENANT_KEY_LEN`].
    pub fn resolve(
        security_context: Option<&SecurityContext>,
        headers: &HeaderMap,
        domain_registry: &DomainRegistry,
    ) -> Result<Option<String>> {
        // 1. JWT tenant_id (highest priority, trusted)
        if let Some(ctx) = security_context {
            if let Some(ref tid) = ctx.tenant_id {
                return Ok(Some(tid.clone()));
            }
        }

        // 2. X-Tenant-ID header (untrusted, must validate)
        if let Some(val) = headers.get("X-Tenant-ID") {
            if let Ok(s) = val.to_str() {
                validate_tenant_key(s)?;
                return Ok(Some(s.to_string()));
            }
        }

        // 3. Host header → domain registry lookup
        if let Some(val) = headers.get("Host") {
            if let Ok(host) = val.to_str() {
                if let Some(key) = domain_registry.lookup(host) {
                    return Ok(Some(key));
                }
            }
        }

        Ok(None)
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
    if !key
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
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
        self.domains
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect()
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
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code
    #![allow(clippy::missing_panics_doc)] // Reason: test code
    #![allow(missing_docs)] // Reason: test code

    use axum::http::HeaderValue;

    use super::*;

    fn headers_with_tenant_id(value: &str) -> HeaderMap {
        let mut map = HeaderMap::new();
        map.insert("X-Tenant-ID", HeaderValue::from_str(value).unwrap());
        map
    }

    fn headers_with_host(value: &str) -> HeaderMap {
        let mut map = HeaderMap::new();
        map.insert("Host", HeaderValue::from_str(value).unwrap());
        map
    }

    fn ctx_with_tenant(tenant_id: &str) -> SecurityContext {
        use chrono::Utc;
        use std::collections::HashMap;

        SecurityContext {
            user_id: "test-user".to_string(),
            roles: vec![],
            tenant_id: Some(tenant_id.to_string()),
            scopes: vec![],
            attributes: HashMap::new(),
            request_id: "test-req".to_string(),
            ip_address: None,
            authenticated_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::hours(1),
            issuer: None,
            audience: None,
        }
    }

    // ── Priority tests ───────────────────────────────────────────────────

    #[test]
    fn test_resolve_from_jwt_takes_priority() {
        let ctx = ctx_with_tenant("from-jwt");
        let headers = headers_with_tenant_id("from-header");
        let registry = DomainRegistry::new();
        let key = TenantKeyResolver::resolve(Some(&ctx), &headers, &registry).unwrap();
        assert_eq!(key, Some("from-jwt".to_string()));
    }

    #[test]
    fn test_resolve_from_header_when_no_jwt() {
        let headers = headers_with_tenant_id("from-header");
        let registry = DomainRegistry::new();
        let key = TenantKeyResolver::resolve(None, &headers, &registry).unwrap();
        assert_eq!(key, Some("from-header".to_string()));
    }

    #[test]
    fn test_resolve_from_host_header() {
        let headers = headers_with_host("api.theirclient.com");
        let registry = DomainRegistry::new();
        registry.register("api.theirclient.com", "tenant-abc");
        let key = TenantKeyResolver::resolve(None, &headers, &registry).unwrap();
        assert_eq!(key, Some("tenant-abc".to_string()));
    }

    #[test]
    fn test_resolve_returns_none_when_no_tenant() {
        let headers = HeaderMap::new();
        let registry = DomainRegistry::new();
        let key = TenantKeyResolver::resolve(None, &headers, &registry).unwrap();
        assert_eq!(key, None);
    }

    // ── Validation tests ─────────────────────────────────────────────────

    #[test]
    fn test_resolve_rejects_invalid_header_chars() {
        let headers = headers_with_tenant_id("../../../etc/passwd");
        let registry = DomainRegistry::new();
        let result = TenantKeyResolver::resolve(None, &headers, &registry);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, FraiseQLError::Validation { .. }),
            "Expected Validation error, got: {err:?}"
        );
    }

    #[test]
    fn test_resolve_rejects_oversized_header() {
        let headers = headers_with_tenant_id(&"a".repeat(200));
        let registry = DomainRegistry::new();
        let result = TenantKeyResolver::resolve(None, &headers, &registry);
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_accepts_valid_header() {
        let headers = headers_with_tenant_id("valid-tenant_123");
        let registry = DomainRegistry::new();
        let key = TenantKeyResolver::resolve(None, &headers, &registry).unwrap();
        assert_eq!(key, Some("valid-tenant_123".to_string()));
    }

    // ── Domain registry tests ────────────────────────────────────────────

    #[test]
    fn test_domain_registry_lookup() {
        let reg = DomainRegistry::new();
        reg.register("api.acme.com", "tenant-acme");
        assert_eq!(reg.lookup("api.acme.com"), Some("tenant-acme".to_string()));
        assert_eq!(reg.lookup("api.other.com"), None);
    }

    #[test]
    fn test_domain_registry_strips_port() {
        let reg = DomainRegistry::new();
        reg.register("api.acme.com", "tenant-acme");
        assert_eq!(
            reg.lookup("api.acme.com:8080"),
            Some("tenant-acme".to_string())
        );
    }

    #[test]
    fn test_domain_registry_remove() {
        let reg = DomainRegistry::new();
        reg.register("api.acme.com", "tenant-acme");
        assert!(reg.remove("api.acme.com"));
        assert_eq!(reg.lookup("api.acme.com"), None);
        assert!(!reg.remove("api.acme.com"));
    }

    #[test]
    fn test_domain_registry_len() {
        let reg = DomainRegistry::new();
        assert!(reg.is_empty());
        reg.register("a.com", "t-a");
        reg.register("b.com", "t-b");
        assert_eq!(reg.len(), 2);
    }

    #[test]
    fn test_host_header_unregistered_domain_returns_none() {
        let headers = headers_with_host("unknown-domain.com");
        let registry = DomainRegistry::new();
        let key = TenantKeyResolver::resolve(None, &headers, &registry).unwrap();
        assert_eq!(key, None);
    }
}

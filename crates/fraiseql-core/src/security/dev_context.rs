//! Synthetic `SecurityContext` from dev-mode default claims.
//!
//! Provides the bridge between `DevConfig.default_claims` (a flat JSON map)
//! and the structured `SecurityContext` used by the executor for RLS,
//! field-level authorization, and `inject_params`.

use std::collections::HashMap;

use chrono::Utc;

use crate::{schema::DevConfig, security::SecurityContext};

/// Create a synthetic [`SecurityContext`] from dev-mode default claims.
///
/// Claim keys are mapped as follows:
/// - `"sub"` → `user_id` (defaults to `"dev-user"` if absent)
/// - `"tenant_id"` / `"org_id"` → `tenant_id`
/// - `"roles"` → `roles` (JSON array of strings)
/// - `"scopes"` / `"scope"` → `scopes` (space-delimited string or JSON array)
/// - all other keys → `attributes`
///
/// # Errors
///
/// This function is infallible — missing keys fall back to sensible defaults.
#[must_use]
#[allow(clippy::implicit_hasher)] // Reason: DevConfig.default_claims uses default HashMap
pub fn security_context_from_dev_claims(
    claims: &HashMap<String, serde_json::Value>,
    request_id: String,
) -> SecurityContext {
    let user_id = claims.get("sub").and_then(|v| v.as_str()).unwrap_or("dev-user").to_string();

    let tenant_id = claims
        .get("tenant_id")
        .or_else(|| claims.get("org_id"))
        .and_then(|v| v.as_str())
        .map(ToString::to_string);

    let roles = claims
        .get("roles")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let scopes = claims
        .get("scopes")
        .or_else(|| claims.get("scope"))
        .and_then(|v| {
            v.as_str()
                .map(|s| s.split_whitespace().map(String::from).collect())
                .or_else(|| {
                    v.as_array().map(|arr| {
                        arr.iter().filter_map(|v| v.as_str().map(String::from)).collect()
                    })
                })
        })
        .unwrap_or_default();

    let reserved = ["sub", "tenant_id", "org_id", "roles", "scopes", "scope"];
    let attributes: HashMap<String, serde_json::Value> = claims
        .iter()
        .filter(|(k, _)| !reserved.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let now = Utc::now();

    SecurityContext {
        user_id,
        tenant_id,
        roles,
        scopes,
        attributes,
        request_id,
        ip_address: None,
        authenticated_at: now,
        expires_at: now + chrono::Duration::hours(24),
        issuer: Some("fraiseql-dev-mode".to_string()),
        audience: None,
    }
}

/// Check whether dev mode is active based on config and environment.
///
/// Returns `true` only when:
/// 1. `dev_config` is `Some` with `enabled = true`
/// 2. The environment is NOT production (`FRAISEQL_ENV != production`)
#[must_use]
pub fn is_dev_mode_active(dev_config: Option<&DevConfig>) -> bool {
    let Some(config) = dev_config else {
        return false;
    };
    if !config.enabled {
        return false;
    }
    // Forcibly disable in production
    !is_production_env()
}

/// Returns `true` if `FRAISEQL_ENV` or `NODE_ENV` is set to `"production"`.
fn is_production_env() -> bool {
    std::env::var("FRAISEQL_ENV").is_ok_and(|v| v.eq_ignore_ascii_case("production"))
        || std::env::var("NODE_ENV").is_ok_and(|v| v.eq_ignore_ascii_case("production"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code
mod tests {
    use super::*;

    #[test]
    fn basic_claims_mapping() {
        let mut claims = HashMap::new();
        claims.insert("sub".to_string(), serde_json::json!("u1"));
        claims.insert("tenant_id".to_string(), serde_json::json!("t1"));
        claims.insert("custom".to_string(), serde_json::json!("val"));

        let ctx = security_context_from_dev_claims(&claims, "req-1".to_string());

        assert_eq!(ctx.user_id, "u1");
        assert_eq!(ctx.tenant_id, Some("t1".to_string()));
        assert_eq!(ctx.attributes.get("custom"), Some(&serde_json::json!("val")));
        assert_eq!(ctx.issuer, Some("fraiseql-dev-mode".to_string()));
    }

    #[test]
    fn default_user_id_when_sub_missing() {
        let claims = HashMap::new();
        let ctx = security_context_from_dev_claims(&claims, "req-2".to_string());
        assert_eq!(ctx.user_id, "dev-user");
    }

    #[test]
    fn org_id_falls_back_to_tenant_id() {
        let mut claims = HashMap::new();
        claims.insert("org_id".to_string(), serde_json::json!("org-1"));

        let ctx = security_context_from_dev_claims(&claims, "req-3".to_string());
        assert_eq!(ctx.tenant_id, Some("org-1".to_string()));
    }

    #[test]
    fn roles_from_array() {
        let mut claims = HashMap::new();
        claims.insert("roles".to_string(), serde_json::json!(["admin", "viewer"]));

        let ctx = security_context_from_dev_claims(&claims, "req-4".to_string());
        assert_eq!(ctx.roles, vec!["admin", "viewer"]);
    }

    #[test]
    fn scopes_from_space_delimited_string() {
        let mut claims = HashMap::new();
        claims.insert("scope".to_string(), serde_json::json!("read:user write:post"));

        let ctx = security_context_from_dev_claims(&claims, "req-5".to_string());
        assert_eq!(ctx.scopes, vec!["read:user", "write:post"]);
    }

    #[test]
    fn scopes_from_array() {
        let mut claims = HashMap::new();
        claims.insert("scopes".to_string(), serde_json::json!(["read:user", "write:post"]));

        let ctx = security_context_from_dev_claims(&claims, "req-6".to_string());
        assert_eq!(ctx.scopes, vec!["read:user", "write:post"]);
    }

    #[test]
    fn reserved_keys_excluded_from_attributes() {
        let mut claims = HashMap::new();
        claims.insert("sub".to_string(), serde_json::json!("u1"));
        claims.insert("roles".to_string(), serde_json::json!(["admin"]));
        claims.insert("extra".to_string(), serde_json::json!(42));

        let ctx = security_context_from_dev_claims(&claims, "req-7".to_string());

        assert!(ctx.attributes.contains_key("extra"));
        assert!(!ctx.attributes.contains_key("sub"));
        assert!(!ctx.attributes.contains_key("roles"));
    }

    #[test]
    fn expires_24h_from_now() {
        let claims = HashMap::new();
        let ctx = security_context_from_dev_claims(&claims, "req-8".to_string());

        let ttl = (ctx.expires_at - ctx.authenticated_at).num_hours();
        assert_eq!(ttl, 24);
    }
}

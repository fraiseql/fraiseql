//! Federation context tracking for subscriptions
//!
//! Ensures subscriptions are bound to their originating subgraph and prevents
//! cross-subgraph subscription attempts in a federated Apollo environment.

/// Federation context for subscription isolation
///
/// In Apollo Federation 2.0, each subgraph is an independent service.
/// This context tracks which subgraph a subscription belongs to and validates
/// that subscription requests don't cross service boundaries.
#[derive(Debug, Clone)]
pub struct FederationContext {
    /// Subgraph ID/name that owns this subscription
    pub federation_id: Option<String>,
    /// Service name in federation
    pub service_name: Option<String>,
}

impl FederationContext {
    /// Create new federation context with both IDs
    #[must_use]
    pub const fn new(federation_id: Option<String>, service_name: Option<String>) -> Self {
        Self {
            federation_id,
            service_name,
        }
    }

    /// Create federation context with `federation_id` only
    #[must_use]
    pub const fn with_id(federation_id: String) -> Self {
        Self {
            federation_id: Some(federation_id),
            service_name: None,
        }
    }

    /// Create federation context with service name only
    #[must_use]
    pub const fn with_service(service_name: String) -> Self {
        Self {
            federation_id: None,
            service_name: Some(service_name),
        }
    }

    /// Create federation context with both
    #[must_use]
    pub const fn with_both(federation_id: String, service_name: String) -> Self {
        Self {
            federation_id: Some(federation_id),
            service_name: Some(service_name),
        }
    }

    /// Create federation context for non-federated environment
    #[must_use]
    pub const fn standalone() -> Self {
        Self {
            federation_id: None,
            service_name: None,
        }
    }

    /// Check if federation context is enabled
    #[must_use]
    pub const fn is_federated(&self) -> bool {
        self.federation_id.is_some() || self.service_name.is_some()
    }

    /// Check if subscription context matches this federation context
    ///
    /// Returns true if:
    /// - Neither context has federation ID (non-federated)
    /// - Both have same federation ID
    /// - Both have same service name
    /// - At least one field matches if both are set
    #[must_use]
    pub fn matches(&self, other: &Self) -> bool {
        // Non-federated environment: any subscription allowed
        if !self.is_federated() && !other.is_federated() {
            return true;
        }

        // Federated environment: must match
        if self.is_federated() && other.is_federated() {
            // Check federation_id match
            if let (Some(self_id), Some(other_id)) = (&self.federation_id, &other.federation_id) {
                if self_id == other_id {
                    return true;
                }
            }

            // Check service_name match
            if let (Some(self_svc), Some(other_svc)) = (&self.service_name, &other.service_name) {
                if self_svc == other_svc {
                    return true;
                }
            }

            return false;
        }

        // One is federated, other is not: mismatch
        false
    }

    /// Check if subscription should be rejected (opposite of matches)
    #[must_use]
    pub fn should_reject(&self, other: &Self) -> bool {
        !self.matches(other)
    }

    /// Get description of federation context for logging
    #[must_use]
    pub fn describe(&self) -> String {
        match (&self.federation_id, &self.service_name) {
            (None, None) => "Standalone (no federation)".to_string(),
            (Some(id), None) => format!("Federation ID: {id}"),
            (None, Some(svc)) => format!("Service: {svc}"),
            (Some(id), Some(svc)) => format!("Federation ID: {id}, Service: {svc}"),
        }
    }

    /// Get identifier for federation (`federation_id` if available, else `service_name`)
    #[must_use]
    pub fn identifier(&self) -> Option<String> {
        self.federation_id
            .clone()
            .or_else(|| self.service_name.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_federation_context_creation() {
        let ctx = FederationContext::new(Some("subgraph-1".to_string()), Some("users".to_string()));
        assert_eq!(ctx.federation_id, Some("subgraph-1".to_string()));
        assert_eq!(ctx.service_name, Some("users".to_string()));
        assert!(ctx.is_federated());
    }

    #[test]
    fn test_federation_context_with_id() {
        let ctx = FederationContext::with_id("subgraph-1".to_string());
        assert_eq!(ctx.federation_id, Some("subgraph-1".to_string()));
        assert_eq!(ctx.service_name, None);
        assert!(ctx.is_federated());
    }

    #[test]
    fn test_federation_context_with_service() {
        let ctx = FederationContext::with_service("orders".to_string());
        assert_eq!(ctx.federation_id, None);
        assert_eq!(ctx.service_name, Some("orders".to_string()));
        assert!(ctx.is_federated());
    }

    #[test]
    fn test_federation_context_standalone() {
        let ctx = FederationContext::standalone();
        assert_eq!(ctx.federation_id, None);
        assert_eq!(ctx.service_name, None);
        assert!(!ctx.is_federated());
    }

    #[test]
    fn test_federation_context_matches_standalone() {
        let ctx1 = FederationContext::standalone();
        let ctx2 = FederationContext::standalone();
        assert!(ctx1.matches(&ctx2));
    }

    #[test]
    fn test_federation_context_matches_same_id() {
        let ctx1 = FederationContext::with_id("subgraph-1".to_string());
        let ctx2 = FederationContext::with_id("subgraph-1".to_string());
        assert!(ctx1.matches(&ctx2));
    }

    #[test]
    fn test_federation_context_matches_different_id() {
        let ctx1 = FederationContext::with_id("subgraph-1".to_string());
        let ctx2 = FederationContext::with_id("subgraph-2".to_string());
        assert!(!ctx1.matches(&ctx2));
    }

    #[test]
    fn test_federation_context_matches_same_service() {
        let ctx1 = FederationContext::with_service("orders".to_string());
        let ctx2 = FederationContext::with_service("orders".to_string());
        assert!(ctx1.matches(&ctx2));
    }

    #[test]
    fn test_federation_context_matches_different_service() {
        let ctx1 = FederationContext::with_service("orders".to_string());
        let ctx2 = FederationContext::with_service("users".to_string());
        assert!(!ctx1.matches(&ctx2));
    }

    #[test]
    fn test_federation_context_matches_both_same() {
        let ctx1 = FederationContext::with_both("subgraph-1".to_string(), "orders".to_string());
        let ctx2 = FederationContext::with_both("subgraph-1".to_string(), "orders".to_string());
        assert!(ctx1.matches(&ctx2));
    }

    #[test]
    fn test_federation_context_matches_both_id_same() {
        let ctx1 = FederationContext::with_both("subgraph-1".to_string(), "orders".to_string());
        let ctx2 = FederationContext::with_both("subgraph-1".to_string(), "users".to_string());
        assert!(ctx1.matches(&ctx2)); // ID matches, so allowed
    }

    #[test]
    fn test_federation_context_matches_both_different() {
        let ctx1 = FederationContext::with_both("subgraph-1".to_string(), "orders".to_string());
        let ctx2 = FederationContext::with_both("subgraph-2".to_string(), "users".to_string());
        assert!(!ctx1.matches(&ctx2));
    }

    #[test]
    fn test_federation_context_matches_federated_vs_standalone() {
        let ctx1 = FederationContext::standalone();
        let ctx2 = FederationContext::with_id("subgraph-1".to_string());
        assert!(!ctx1.matches(&ctx2));
        assert!(!ctx2.matches(&ctx1));
    }

    #[test]
    fn test_federation_context_should_reject() {
        let ctx1 = FederationContext::with_id("subgraph-1".to_string());
        let ctx2 = FederationContext::with_id("subgraph-2".to_string());
        assert!(ctx1.should_reject(&ctx2));
        assert!(ctx2.should_reject(&ctx1));
    }

    #[test]
    fn test_federation_context_describe() {
        assert_eq!(
            FederationContext::standalone().describe(),
            "Standalone (no federation)"
        );
        assert_eq!(
            FederationContext::with_id("subgraph-1".to_string()).describe(),
            "Federation ID: subgraph-1"
        );
        assert_eq!(
            FederationContext::with_service("orders".to_string()).describe(),
            "Service: orders"
        );
        assert_eq!(
            FederationContext::with_both("subgraph-1".to_string(), "orders".to_string()).describe(),
            "Federation ID: subgraph-1, Service: orders"
        );
    }

    #[test]
    fn test_federation_context_identifier() {
        let ctx1 = FederationContext::with_id("subgraph-1".to_string());
        assert_eq!(ctx1.identifier(), Some("subgraph-1".to_string()));

        let ctx2 = FederationContext::with_service("orders".to_string());
        assert_eq!(ctx2.identifier(), Some("orders".to_string()));

        let ctx3 = FederationContext::standalone();
        assert_eq!(ctx3.identifier(), None);

        let ctx4 = FederationContext::with_both("subgraph-1".to_string(), "orders".to_string());
        assert_eq!(ctx4.identifier(), Some("subgraph-1".to_string())); // ID takes precedence
    }
}

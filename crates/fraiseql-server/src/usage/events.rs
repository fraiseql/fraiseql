//! Mutation audit event type.

/// A single mutation audit event, captured from the `fraiseql::mutation_audit`
/// tracing target and normalised before aggregation.
///
/// The `period` field is a UTC calendar month bucket in `"YYYY-MM"` format,
/// assigned at the moment the event is recorded by
/// [`MutationAuditLayer`](super::layer::MutationAuditLayer).
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct MutationAuditEvent {
    /// GraphQL mutation field name (e.g. `"create_user"`).
    pub mutation_name: String,
    /// Return-type entity name (e.g. `"User"`).
    pub entity_type:   String,
    /// Mutation operation kind (`"create"`, `"update"`, `"delete"`, `"custom"`).
    pub operation:     String,
    /// Tenant identifier extracted from the security context; empty string when
    /// no tenant is present (single-tenant deployments).
    pub tenant_id:     String,
    /// UTC calendar month in `"YYYY-MM"` format (e.g. `"2026-05"`).
    pub period:        String,
}

impl MutationAuditEvent {
    /// Create a new mutation audit event.
    pub fn new(
        mutation_name: impl Into<String>,
        entity_type: impl Into<String>,
        operation: impl Into<String>,
        tenant_id: impl Into<String>,
        period: impl Into<String>,
    ) -> Self {
        Self {
            mutation_name: mutation_name.into(),
            entity_type: entity_type.into(),
            operation: operation.into(),
            tenant_id: tenant_id.into(),
            period: period.into(),
        }
    }
}

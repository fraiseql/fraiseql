//! A page of a Relay query read without a document (a REST relay route) passes the gates a
//! connection passes through a document: the operation gate, the row scope, the client's
//! filter, field-level RBAC, the gated-field refusal and the request's cost ceiling. Each
//! refusal is asserted before the database is reached.

use super::*;
use crate::security::RequestBudget;

/// The match a REST relay route hands the engine: its fields, and its window as arguments.
fn page_match(executor: &Executor) -> crate::runtime::QueryMatch {
    let mut qm = direct_match(executor, "{ users { id } }");
    qm.arguments.insert("first".to_string(), serde_json::json!(2));
    qm
}

fn untouched(adapter: &CapturingMockAdapter) -> bool {
    adapter.captured_where().is_none() && adapter.captured_limit().is_none()
}

#[tokio::test]
async fn a_relay_page_is_hidden_from_a_principal_without_the_role() {
    let (executor, adapter) = relay_executor(relay_schema(admins_only), RuntimeConfig::default());
    let err = executor
        .execute_relay_direct(&page_match(&executor), Some(&principal()), None)
        .await
        .expect_err("a viewer is not an admin");
    assert!(err.to_string().contains("not found"), "{err}");
    assert!(untouched(&adapter));
}

#[tokio::test]
async fn a_relay_page_refuses_a_principal_of_the_wrong_actor_type() {
    let (executor, adapter) =
        relay_executor(relay_schema(service_accounts_only), RuntimeConfig::default());
    let err = executor
        .execute_relay_direct(&page_match(&executor), Some(&principal()), None)
        .await
        .expect_err("a human user is not a service account");
    assert!(is_refusal(&err), "{err}");
    assert!(untouched(&adapter));
}

#[tokio::test]
async fn a_relay_page_refuses_an_expired_principal() {
    let (executor, adapter) = relay_executor(relay_schema(|_| {}), RuntimeConfig::default());
    let mut expired = principal();
    expired.expires_at =
        fraiseql_time::Timestamp::now() - fraiseql_time::jiff::SignedDuration::from_mins(1);
    let err = executor
        .execute_relay_direct(&page_match(&executor), Some(&expired), None)
        .await
        .expect_err("an expired principal reads nothing");
    assert!(err.to_string().contains("expired"), "{err}");
    assert!(untouched(&adapter));
}

#[tokio::test]
async fn a_relay_page_is_put_to_the_authorizer_as_a_read_of_its_query() {
    struct DenyUsers;
    impl Authorizer for DenyUsers {
        fn authorize(&self, req: &AuthzRequest<'_>) -> Result<AuthzDecision> {
            Ok(if req.target_type == Some("User") {
                AuthzDecision::Deny {
                    reason: "no users".to_string(),
                }
            } else {
                AuthzDecision::Allow
            })
        }
    }
    let config = RuntimeConfig::default().with_authorizer(Arc::new(DenyUsers));
    let (executor, adapter) = relay_executor(relay_schema(|_| {}), config);
    let err = executor
        .execute_relay_direct(&page_match(&executor), Some(&principal()), None)
        .await
        .expect_err("a read of User is denied");
    assert!(matches!(err, FraiseQLError::Authorization { .. }), "{err}");
    assert!(untouched(&adapter));
}

#[tokio::test]
async fn a_relay_page_composes_the_tenant_filter() {
    let (executor, adapter) = relay_executor(relay_schema(tenant_scoped), RuntimeConfig::default());
    executor
        .execute_relay_direct(&page_match(&executor), Some(&principal()), None)
        .await
        .expect("a tenant-scoped page");
    assert!(where_text(&adapter).contains("tenant-abc"), "{}", where_text(&adapter));
}

#[tokio::test]
async fn a_relay_page_refuses_a_tenant_scoped_query_without_a_principal() {
    let (executor, adapter) = relay_executor(relay_schema(tenant_scoped), RuntimeConfig::default());
    executor
        .execute_relay_direct(&page_match(&executor), None, None)
        .await
        .expect_err("no principal, no tenant to scope to");
    assert!(untouched(&adapter));
}

#[tokio::test]
async fn a_relay_page_composes_the_row_policy() {
    let config = RuntimeConfig::default().with_rls_policy(Arc::new(OwnerPolicy));
    let (executor, adapter) = relay_executor(relay_schema(|_| {}), config);
    executor
        .execute_relay_direct(&page_match(&executor), Some(&principal()), None)
        .await
        .expect("a policy-scoped page");
    assert!(where_text(&adapter).contains("owner"), "{}", where_text(&adapter));
}

#[tokio::test]
async fn a_relay_page_composes_the_clients_filter() {
    let (executor, adapter) = relay_executor(relay_schema(|_| {}), RuntimeConfig::default());
    let mut qm = page_match(&executor);
    qm.arguments
        .insert("where".to_string(), serde_json::json!({ "name": { "eq": "Alice" } }));
    executor
        .execute_relay_direct(&qm, Some(&principal()), None)
        .await
        .expect("a filtered page");
    assert!(where_text(&adapter).contains("Alice"), "{}", where_text(&adapter));
}

#[tokio::test]
async fn a_relay_page_over_a_type_with_a_gated_field_is_refused() {
    let mut schema = relay_schema(|_| {});
    schema.types[0].fields[1].authorize = true;
    let (executor, adapter) = relay_executor(schema, RuntimeConfig::default());
    executor
        .execute_relay_direct(&page_match(&executor), Some(&principal()), None)
        .await
        .expect_err("the Relay path does not adjudicate gated fields");
    assert!(untouched(&adapter));
}

#[tokio::test]
async fn a_relay_page_selecting_a_field_whose_scope_rejects_is_refused() {
    let mut schema = relay_schema(|_| {});
    schema.types[0].fields[2].on_deny = crate::schema::FieldDenyPolicy::Reject;
    let (executor, adapter) = relay_executor(schema, RuntimeConfig::default());
    let qm = direct_match(&executor, "{ users { id secret } }");
    executor
        .execute_relay_direct(&qm, Some(&principal()), None)
        .await
        .expect_err("the principal lacks `read:secret`");
    assert!(untouched(&adapter));
}

#[tokio::test]
async fn a_relay_page_over_the_cost_ceiling_is_refused_before_the_read() {
    let (executor, adapter) = relay_executor(relay_schema(|_| {}), RuntimeConfig::default());
    let budget = RequestBudget::new(None, Some(1));
    let err = executor
        .execute_relay_direct(&page_match(&executor), Some(&principal()), Some(&budget))
        .await
        .expect_err("one field over a page of two costs more than one");
    assert!(matches!(err, FraiseQLError::CostExceeded { .. }), "{err}");
    assert!(untouched(&adapter));
}

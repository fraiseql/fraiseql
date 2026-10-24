//! One test per guard site of a root read (#1646).
//!
//! A root read reaches the database through one of six doors: the authenticated and the
//! anonymous GraphQL document, the direct read (REST, gRPC, exports, embedding), the count,
//! the Relay connection and the `node(id:)` lookup. Each door applies the same gates — the
//! operation's own (`requires_role`, `requires_actor`, an expired token), the row predicate
//! (RLS, `inject_params`, the server's parent scoping), the client's filter, the page caps,
//! the field masks and the response ceiling.
//!
//! A shared rule's own tests say nothing about whether each door consults it, so these
//! tests are per door: each one fails if exactly that door stops applying exactly that
//! gate. They hold the behaviour of every door constant while the doors are folded onto
//! one root-read pipeline.

use std::{collections::HashMap, sync::Arc};

use chrono::Utc;

use crate::{
    backend::types::JsonbValue,
    error::{FraiseQLError, Result},
    runtime::{
        RuntimeConfig,
        executor::{Executor, test_support::CapturingMockAdapter},
    },
    schema::{
        AutoParams, CompiledSchema, CursorType, FieldDefinition, FieldDenyPolicy, FieldType,
        InjectedParamSource, PaginationOrder, QueryDefinition, TypeDefinition,
    },
    security::{
        ActorType, Authorizer, AuthzDecision, AuthzRequest, RLSPolicy, RlsWhereClause,
        SecurityContext, rls_policy::RlsTarget,
    },
};

// ── fixtures ─────────────────────────────────────────────────────────────────────────

/// A `users` list query over `v_user`, every auto-param on, ordered by identity, with a
/// `User` type of `id`, `name` and a `secret` that needs `read:secret` and masks.
fn schema_with(edit: impl FnOnce(&mut QueryDefinition)) -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut query =
        QueryDefinition::new("users", "User").returning_list().with_sql_source("v_user");
    query.auto_params = AutoParams::all();
    query.pagination_order = Some(PaginationOrder::Identity);
    edit(&mut query);
    schema.queries.push(query);
    let mut user = TypeDefinition::new("User", "v_user");
    let mut secret = FieldDefinition::new("secret", FieldType::String);
    secret.requires_scope = Some("read:secret".to_string());
    secret.on_deny = FieldDenyPolicy::Mask;
    user.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::new("name", FieldType::String),
        secret,
    ];
    schema.types.push(user);
    schema
}

fn plain_schema() -> CompiledSchema {
    schema_with(|_| {})
}

fn rows() -> Vec<JsonbValue> {
    vec![
        JsonbValue::new(serde_json::json!({"id": "1", "name": "Alice", "secret": "s1"})),
        JsonbValue::new(serde_json::json!({"id": "2", "name": "Bob", "secret": "s2"})),
    ]
}

fn principal() -> SecurityContext {
    SecurityContext {
        user_id:          "user-42".into(),
        roles:            vec!["viewer".to_string()],
        tenant_id:        Some("tenant-abc".into()),
        scopes:           Vec::new(),
        attributes:       HashMap::from([(
            "tenant_id".to_string(),
            serde_json::json!("tenant-abc"),
        )]),
        request_id:       "req-guard".to_string(),
        ip_address:       None,
        expires_at:       Utc::now() + chrono::Duration::hours(1),
        authenticated_at: Utc::now(),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

fn tenant_scoped(query: &mut QueryDefinition) {
    query
        .inject_params
        .insert("tenant_id".to_string(), InjectedParamSource::Jwt("tenant_id".to_string()));
}

fn service_accounts_only(query: &mut QueryDefinition) {
    query.requires_actor = vec![ActorType::ServiceAccount];
}

fn admins_only(query: &mut QueryDefinition) {
    query.requires_role = Some("admin".to_string());
}

/// A policy that scopes every read to the principal's own rows: `owner = <user id>`.
struct OwnerPolicy;

impl RLSPolicy for OwnerPolicy {
    fn evaluate(
        &self,
        context: &SecurityContext,
        _target: &RlsTarget<'_>,
    ) -> Result<Option<RlsWhereClause>> {
        Ok(Some(RlsWhereClause::new(crate::backend::WhereClause::Field {
            path:     vec!["owner".to_string()],
            operator: crate::backend::where_clause::WhereOperator::Eq,
            value:    serde_json::Value::String(context.user_id.to_string()),
        })))
    }
}

fn executor(
    schema: CompiledSchema,
    config: RuntimeConfig,
) -> (Executor, Arc<CapturingMockAdapter>) {
    let adapter = Arc::new(CapturingMockAdapter::new(rows()));
    (Executor::with_config(schema, adapter.clone(), config), adapter)
}

fn relay_executor(
    schema: CompiledSchema,
    config: RuntimeConfig,
) -> (Executor, Arc<CapturingMockAdapter>) {
    let adapter = Arc::new(CapturingMockAdapter::new(rows()));
    (Executor::with_config_and_relay(schema, adapter.clone(), config), adapter)
}

fn relay_schema(edit: impl FnOnce(&mut QueryDefinition)) -> CompiledSchema {
    schema_with(|q| {
        q.relay = true;
        q.relay_cursor_column = Some("pk_user".to_string());
        q.relay_cursor_type = CursorType::Int64;
        edit(q);
    })
}

/// The query match a direct read is handed: what the REST resolver builds.
fn direct_match(executor: &Executor, document: &str) -> crate::runtime::QueryMatch {
    executor
        .matcher()
        .match_query(document, None)
        .expect("the fixture document matches")
}

fn where_text(adapter: &CapturingMockAdapter) -> String {
    format!("{:?}", adapter.captured_where())
}

fn is_refusal(err: &FraiseQLError) -> bool {
    matches!(err, FraiseQLError::Validation { .. } | FraiseQLError::Authorization { .. })
}

// ── the authenticated GraphQL document ────────────────────────────────────────────────

#[tokio::test]
async fn a_document_read_refuses_an_expired_principal() {
    let (executor, _) = executor(plain_schema(), RuntimeConfig::default());
    let mut expired = principal();
    expired.expires_at = Utc::now() - chrono::Duration::minutes(1);
    let err = executor
        .execute_with_security("{ users { id } }", None, &expired)
        .await
        .expect_err("an expired token reads nothing");
    assert!(err.to_string().contains("expired"), "{err}");
}

#[tokio::test]
async fn a_document_read_refuses_a_principal_of_the_wrong_actor_type() {
    let (executor, adapter) =
        executor(schema_with(service_accounts_only), RuntimeConfig::default());
    let err = executor
        .execute_with_security("{ users { id } }", None, &principal())
        .await
        .expect_err("a human user is not a service account");
    assert!(is_refusal(&err), "{err}");
    assert!(adapter.captured_where().is_none() && adapter.captured_limit().is_none());
}

#[tokio::test]
async fn a_document_read_composes_the_clients_filter() {
    let (executor, adapter) = executor(plain_schema(), RuntimeConfig::default());
    let vars = serde_json::json!({ "where": { "name": { "eq": "Alice" } } });
    executor
        .execute_with_security(
            "query($where: UserWhereInput) { users(where: $where) { id } }",
            Some(&vars),
            &principal(),
        )
        .await
        .expect("a filtered read");
    assert!(where_text(&adapter).contains("Alice"), "{}", where_text(&adapter));
}

#[tokio::test]
async fn a_document_read_holds_the_limit_to_the_page_ceiling() {
    let config = RuntimeConfig {
        max_page_size: Some(10),
        ..RuntimeConfig::default()
    };
    let (executor, _) = executor(plain_schema(), config);
    let err = executor
        .execute_with_security("{ users(limit: 100) { id } }", None, &principal())
        .await
        .expect_err("a page over the ceiling");
    assert!(err.to_string().contains("limit"), "{err}");
}

#[tokio::test]
async fn a_document_read_holds_the_offset_to_its_ceiling() {
    let config = RuntimeConfig {
        max_offset: Some(10),
        ..RuntimeConfig::default()
    };
    let (executor, _) = executor(plain_schema(), config);
    let err = executor
        .execute_with_security("{ users(offset: 100) { id } }", None, &principal())
        .await
        .expect_err("an offset over the ceiling");
    assert!(err.to_string().contains("offset"), "{err}");
}

#[tokio::test]
async fn a_document_read_masks_a_field_the_principal_lacks_the_scope_for() {
    let (executor, _) = executor(plain_schema(), RuntimeConfig::default());
    let response = executor
        .execute_with_security("{ users { id secret } }", None, &principal())
        .await
        .expect("a masked field is served as null");
    assert_eq!(response["data"]["users"][0]["secret"], serde_json::Value::Null, "{response}");
}

#[tokio::test]
async fn a_document_read_over_the_response_ceiling_is_refused() {
    let config = RuntimeConfig {
        max_response_bytes: Some(8),
        ..RuntimeConfig::default()
    };
    let (executor, _) = executor(plain_schema(), config);
    executor
        .execute_with_security("{ users { id name } }", None, &principal())
        .await
        .expect_err("two rows weigh more than eight bytes");
}

// ── the anonymous GraphQL document ────────────────────────────────────────────────────

#[tokio::test]
async fn an_anonymous_document_read_of_an_actor_restricted_query_is_refused() {
    let (executor, adapter) =
        executor(schema_with(service_accounts_only), RuntimeConfig::default());
    let err = executor
        .execute("{ users { id } }", None)
        .await
        .expect_err("an anonymous request has no actor class");
    assert!(is_refusal(&err), "{err}");
    assert!(adapter.captured_limit().is_none());
}

#[tokio::test]
async fn an_anonymous_document_read_holds_the_offset_to_its_ceiling() {
    let config = RuntimeConfig {
        max_offset: Some(10),
        ..RuntimeConfig::default()
    };
    let (executor, _) = executor(plain_schema(), config);
    let err = executor
        .execute("{ users(offset: 100) { id } }", None)
        .await
        .expect_err("an offset over the ceiling");
    assert!(err.to_string().contains("offset"), "{err}");
}

#[tokio::test]
async fn an_anonymous_document_read_masks_a_scoped_field() {
    let (executor, _) = executor(plain_schema(), RuntimeConfig::default());
    let response = executor
        .execute("{ users { id secret } }", None)
        .await
        .expect("a masked field is served as null");
    assert_eq!(response["data"]["users"][0]["secret"], serde_json::Value::Null, "{response}");
}

// ── the direct read ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_direct_read_refuses_a_principal_of_the_wrong_actor_type() {
    let (executor, adapter) =
        executor(schema_with(service_accounts_only), RuntimeConfig::default());
    let qm = direct_match(&executor, "{ users { id } }");
    let err = executor
        .execute_query_direct(&qm, None, Some(&principal()), None)
        .await
        .expect_err("a human user is not a service account");
    assert!(is_refusal(&err), "{err}");
    assert!(adapter.captured_limit().is_none());
}

#[tokio::test]
async fn a_direct_read_composes_the_tenant_filter() {
    let (executor, adapter) = executor(schema_with(tenant_scoped), RuntimeConfig::default());
    let qm = direct_match(&executor, "{ users { id } }");
    executor
        .execute_query_direct(&qm, None, Some(&principal()), None)
        .await
        .expect("a tenant-scoped read");
    assert!(where_text(&adapter).contains("tenant-abc"), "{}", where_text(&adapter));
}

#[tokio::test]
async fn a_direct_read_refuses_a_tenant_scoped_query_without_a_principal() {
    let (executor, adapter) = executor(schema_with(tenant_scoped), RuntimeConfig::default());
    let qm = direct_match(&executor, "{ users { id } }");
    executor
        .execute_query_direct(&qm, None, None, None)
        .await
        .expect_err("no principal, no tenant to scope to");
    assert!(adapter.captured_limit().is_none() && adapter.captured_where().is_none());
}

#[tokio::test]
async fn a_direct_read_composes_the_servers_parent_scoping() {
    let (executor, adapter) = executor(plain_schema(), RuntimeConfig::default());
    let mut qm = direct_match(&executor, "{ users { id } }");
    qm.scope_where = Some(serde_json::json!({ "name": { "eq": "parent-scope" } }));
    executor
        .execute_query_direct(&qm, None, Some(&principal()), None)
        .await
        .expect("a scoped read");
    assert!(where_text(&adapter).contains("parent-scope"), "{}", where_text(&adapter));
}

#[tokio::test]
async fn a_direct_read_holds_the_offset_to_its_ceiling() {
    let config = RuntimeConfig {
        max_offset: Some(10),
        ..RuntimeConfig::default()
    };
    let (executor, _) = executor(plain_schema(), config);
    let mut qm = direct_match(&executor, "{ users { id } }");
    qm.arguments.insert("offset".to_string(), serde_json::json!(100));
    let err = executor
        .execute_query_direct(&qm, None, Some(&principal()), None)
        .await
        .expect_err("an offset over the ceiling");
    assert!(err.to_string().contains("offset"), "{err}");
}

#[tokio::test]
async fn a_direct_read_masks_a_field_the_principal_lacks_the_scope_for() {
    let (executor, _) = executor(plain_schema(), RuntimeConfig::default());
    let qm = direct_match(&executor, "{ users { id secret } }");
    let response = executor
        .execute_query_direct(&qm, None, Some(&principal()), None)
        .await
        .expect("a masked field is served as null");
    assert_eq!(response["data"]["users"][0]["secret"], serde_json::Value::Null, "{response}");
}

#[tokio::test]
async fn a_direct_read_selecting_a_policy_gated_field_is_refused() {
    let mut schema = plain_schema();
    schema.types[0].fields[1].authorize = true;
    let (executor, adapter) = executor(schema, RuntimeConfig::default());
    let qm = direct_match(&executor, "{ users { id name } }");
    executor
        .execute_query_direct(&qm, None, Some(&principal()), None)
        .await
        .expect_err("the JSON direct read cannot adjudicate a gated field");
    assert!(adapter.captured_limit().is_none() && adapter.captured_where().is_none());
}

// ── the count ─────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_count_is_hidden_from_a_principal_without_the_role() {
    let (executor, _) = executor(schema_with(admins_only), RuntimeConfig::default());
    let qm = direct_match(&executor, "{ users { id } }");
    let err = executor
        .count_rows(&qm, None, Some(&principal()))
        .await
        .expect_err("a viewer is not an admin");
    assert!(err.to_string().contains("not found"), "{err}");
}

#[tokio::test]
async fn a_count_refuses_a_principal_of_the_wrong_actor_type() {
    let (executor, _) = executor(schema_with(service_accounts_only), RuntimeConfig::default());
    let qm = direct_match(&executor, "{ users { id } }");
    let err = executor
        .count_rows(&qm, None, Some(&principal()))
        .await
        .expect_err("a human user is not a service account");
    assert!(is_refusal(&err), "{err}");
}

#[tokio::test]
async fn a_count_composes_the_row_policy() {
    let config = RuntimeConfig::default().with_rls_policy(Arc::new(OwnerPolicy));
    let (executor, adapter) = executor(plain_schema(), config);
    let qm = direct_match(&executor, "{ users { id } }");
    executor.count_rows(&qm, None, Some(&principal())).await.expect("a count");
    assert!(where_text(&adapter).contains("owner"), "{}", where_text(&adapter));
}

#[tokio::test]
async fn a_count_composes_the_tenant_filter() {
    let (executor, adapter) = executor(schema_with(tenant_scoped), RuntimeConfig::default());
    let qm = direct_match(&executor, "{ users { id } }");
    executor.count_rows(&qm, None, Some(&principal())).await.expect("a count");
    assert!(where_text(&adapter).contains("tenant-abc"), "{}", where_text(&adapter));
}

#[tokio::test]
async fn a_count_refuses_a_tenant_scoped_query_without_a_principal() {
    let (executor, adapter) = executor(schema_with(tenant_scoped), RuntimeConfig::default());
    let qm = direct_match(&executor, "{ users { id } }");
    executor
        .count_rows(&qm, None, None)
        .await
        .expect_err("no tenant to count within");
    assert!(adapter.captured_where().is_none());
}

#[tokio::test]
async fn a_count_composes_the_servers_parent_scoping() {
    let (executor, adapter) = executor(plain_schema(), RuntimeConfig::default());
    let mut qm = direct_match(&executor, "{ users { id } }");
    qm.scope_where = Some(serde_json::json!({ "name": { "eq": "parent-scope" } }));
    executor.count_rows(&qm, None, Some(&principal())).await.expect("a count");
    assert!(where_text(&adapter).contains("parent-scope"), "{}", where_text(&adapter));
}

#[tokio::test]
async fn a_count_composes_the_clients_filter() {
    let (executor, adapter) = executor(plain_schema(), RuntimeConfig::default());
    let mut qm = direct_match(&executor, "{ users { id } }");
    qm.arguments
        .insert("where".to_string(), serde_json::json!({ "name": { "eq": "Alice" } }));
    executor.count_rows(&qm, None, Some(&principal())).await.expect("a count");
    assert!(where_text(&adapter).contains("Alice"), "{}", where_text(&adapter));
}

// ── the Relay connection ──────────────────────────────────────────────────────────────

const CONNECTION: &str = "{ users(first: 2) { edges { node { id } } } }";

#[tokio::test]
async fn a_connection_over_a_type_with_a_gated_field_is_refused() {
    let mut schema = relay_schema(|_| {});
    schema.types[0].fields[1].authorize = true;
    let (executor, adapter) = relay_executor(schema, RuntimeConfig::default());
    executor
        .execute_with_security(CONNECTION, None, &principal())
        .await
        .expect_err("the Relay path does not adjudicate gated fields");
    assert!(adapter.captured_where().is_none());
}

#[tokio::test]
async fn a_connection_composes_the_row_policy() {
    let config = RuntimeConfig::default().with_rls_policy(Arc::new(OwnerPolicy));
    let (executor, adapter) = relay_executor(relay_schema(|_| {}), config);
    executor
        .execute_with_security(CONNECTION, None, &principal())
        .await
        .expect("a policy-scoped page");
    assert!(where_text(&adapter).contains("owner"), "{}", where_text(&adapter));
}

#[tokio::test]
async fn an_anonymous_connection_under_a_row_policy_is_refused() {
    let config = RuntimeConfig::default().with_rls_policy(Arc::new(OwnerPolicy));
    let (executor, adapter) = relay_executor(relay_schema(|_| {}), config);
    executor
        .execute(CONNECTION, None)
        .await
        .expect_err("the policy cannot be evaluated without a principal");
    assert!(adapter.captured_where().is_none());
}

#[tokio::test]
async fn a_connection_composes_the_tenant_filter() {
    let (executor, adapter) = relay_executor(relay_schema(tenant_scoped), RuntimeConfig::default());
    executor
        .execute_with_security(CONNECTION, None, &principal())
        .await
        .expect("a tenant-scoped page");
    assert!(where_text(&adapter).contains("tenant-abc"), "{}", where_text(&adapter));
}

/// A query both injected and under a row policy is refused for its missing context, not
/// hidden: every door answered the inject refusal before the policy's.
#[tokio::test]
async fn an_anonymous_connection_to_an_injected_query_under_a_row_policy_names_the_missing_context()
{
    let config = RuntimeConfig::default().with_rls_policy(Arc::new(OwnerPolicy));
    let (executor, adapter) = relay_executor(relay_schema(tenant_scoped), config);
    let err = executor
        .execute(CONNECTION, None)
        .await
        .expect_err("neither the policy nor the inject params can be resolved anonymously");
    assert!(err.to_string().contains("security context"), "{err}");
    assert!(adapter.captured_where().is_none());
}

// ── the node lookup ───────────────────────────────────────────────────────────────────

fn node_document() -> String {
    use base64::Engine as _;
    let id = base64::engine::general_purpose::STANDARD
        .encode("User:00000000-0000-0000-0000-000000000001");
    format!(r#"{{ node(id: "{id}") {{ ... on User {{ id }} }} }}"#)
}

#[tokio::test]
async fn a_node_lookup_refuses_a_principal_of_the_wrong_actor_type() {
    let (executor, adapter) =
        relay_executor(relay_schema(service_accounts_only), RuntimeConfig::default());
    let err = executor
        .execute_with_security(&node_document(), None, &principal())
        .await
        .expect_err("a human user is not a service account");
    assert!(is_refusal(&err), "{err}");
    assert!(adapter.captured_where().is_none());
}

#[tokio::test]
async fn a_node_lookup_is_put_to_the_authorizer_as_a_read_of_its_type() {
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
        .execute_with_security(&node_document(), None, &principal())
        .await
        .expect_err("a read of User is denied once the id names it");
    assert!(matches!(err, FraiseQLError::Authorization { .. }), "{err}");
    assert!(adapter.captured_where().is_none());
}

// ── what a door inherits from the one pipeline ────────────────────────────────────────

/// A `<name>Count` sibling inherits its list's explicit arguments because they lower into
/// the same `WHERE` (`QueryDefinition::count_sibling`). The count used to resolve its read
/// in a copy that composed the row scope, the parent scoping and the client filter but not
/// the explicit arguments, so `usersCount(name: "Alice")` counted every user.
#[tokio::test]
async fn a_count_sibling_counts_only_the_rows_its_explicit_arguments_select() {
    let mut schema = schema_with(|q| {
        q.arguments
            .push(crate::schema::ArgumentDefinition::optional("name", FieldType::String));
    });
    let count = schema.queries[0].count_sibling();
    schema.queries.push(count);
    let (executor, adapter) = executor(schema, RuntimeConfig::default());
    executor
        .execute_with_security(r#"{ usersCount(name: "Alice") }"#, None, &principal())
        .await
        .expect("a count");
    assert!(where_text(&adapter).contains("Alice"), "{}", where_text(&adapter));
}

/// The expired-token gate is the operation gate's, so it binds on every door, not only on
/// the document read that used to carry it.
#[tokio::test]
async fn a_direct_read_and_a_count_refuse_an_expired_principal() {
    let (executor, adapter) = executor(plain_schema(), RuntimeConfig::default());
    let qm = direct_match(&executor, "{ users { id } }");
    let mut expired = principal();
    expired.expires_at = Utc::now() - chrono::Duration::minutes(1);
    let read = executor.execute_query_direct(&qm, None, Some(&expired), None).await;
    assert!(read.is_err_and(|e| e.to_string().contains("expired")));
    let count = executor.count_rows(&qm, None, Some(&expired)).await;
    assert!(count.is_err_and(|e| e.to_string().contains("expired")));
    assert!(adapter.captured_where().is_none() && adapter.captured_limit().is_none());
}

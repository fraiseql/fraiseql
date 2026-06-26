#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
use std::collections::HashMap;

use chrono::Utc;
use indexmap::IndexMap;

use super::{test_support::*, *};
use crate::{
    db::types::JsonbValue,
    runtime::{JsonbOptimizationOptions, JsonbStrategy, RuntimeConfig},
    schema::{
        AutoParams, CompiledSchema, CursorType, FieldDefinition, FieldDenyPolicy, FieldType,
        InjectedParamSource, QueryDefinition, RoleDefinition, SecurityConfig, TenancyConfig,
        TypeDefinition,
    },
    security::SecurityContext,
};

// ── mod query: basic query execution ─────────────────────────────────────

mod query {
    use super::*;

    #[tokio::test]
    async fn test_executor_new() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);
        assert_eq!(executor.schema().queries.len(), 1);
    }

    #[tokio::test]
    async fn test_execute_query() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter);

        let query = "{ users { id name } }";
        let result = executor.execute(query, None).await.unwrap();

        assert!(result.get("data").is_some());
        assert!(result["data"].get("users").is_some());
        assert!(result["data"]["users"][0].get("id").is_some());
        assert!(result["data"]["users"][0].get("name").is_some());
    }

    #[tokio::test]
    async fn test_execute_json() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter);

        let query = "{ users { id name } }";
        let result = executor.execute(query, None).await.unwrap();

        assert!(result.get("data").is_some());
        assert!(result["data"].get("users").is_some());
    }

    #[tokio::test]
    async fn test_executor_with_config() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let config = RuntimeConfig {
            cache_query_plans:    false,
            max_query_depth:      5,
            max_query_complexity: 500,
            max_page_size:        Some(1000),
            enable_tracing:       true,
            field_filter:         None,
            rls_policy:           None,
            field_authorizer:     None,
            authorizer:           None,
            query_timeout_ms:     30_000,
            jsonb_optimization:   JsonbOptimizationOptions::default(),
            query_validation:     None,
            audit_mutations:      false,
            changelog_enabled:    true,
        };
        let executor = Executor::with_config(schema, adapter, config);

        assert!(!executor.config().cache_query_plans);
        assert_eq!(executor.config().max_query_depth, 5);
        assert!(executor.config().enable_tracing);
    }
}

// ── mod introspection: __schema and __type queries ────────────────────────

mod introspection {
    use super::*;

    #[tokio::test]
    async fn test_introspection_schema_query() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let query = r"{ __schema { queryType { name } } }";
        let result = executor.execute(query, None).await.unwrap();

        assert!(result["data"].get("__schema").is_some());
    }

    #[tokio::test]
    async fn test_introspection_type_query() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let query = r#"{ __type(name: "Int") { kind name } }"#;
        let result = executor.execute(query, None).await.unwrap();

        assert!(result["data"].get("__type").is_some());
        assert_eq!(result["data"]["__type"]["name"], "Int");
    }

    #[tokio::test]
    async fn test_introspection_unknown_type() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let query = r#"{ __type(name: "UnknownType") { kind name } }"#;
        let result = executor.execute(query, None).await.unwrap();

        // Unknown type returns null
        assert!(result["data"]["__type"].is_null());
    }
}

// ── mod typename: root __typename meta-field (#450) ───────────────────────

mod typename {
    use super::*;

    #[tokio::test]
    async fn test_root_typename_resolves_to_query() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        // The canonical zero-cost health probe: `{ __typename }` → "Query".
        let result = executor.execute("{ __typename }", None).await.unwrap();
        assert_eq!(result, serde_json::json!({ "data": { "__typename": "Query" } }));
    }

    #[tokio::test]
    async fn test_root_typename_aliased() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let result = executor.execute("{ ping: __typename }", None).await.unwrap();
        assert_eq!(result, serde_json::json!({ "data": { "ping": "Query" } }));
    }

    #[tokio::test]
    async fn test_root_typename_mixed_with_regular_field() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter);

        // Mixed root: `__typename` resolves locally while `users` is dispatched.
        let result = executor.execute("{ __typename users { id } }", None).await.unwrap();
        assert_eq!(result["data"]["__typename"], "Query");
        assert!(result["data"]["users"].is_array());
        assert!(result["data"]["users"][0].get("id").is_some());
    }

    #[tokio::test]
    async fn test_root_typename_mixed_trailing() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter);

        // Order-independent: `__typename` after the real field must still resolve.
        let result = executor.execute("{ users { id } __typename }", None).await.unwrap();
        assert_eq!(result["data"]["__typename"], "Query");
        assert!(result["data"]["users"].is_array());
    }

    #[tokio::test]
    async fn test_root_typename_multiple_aliased() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        // Several aliased `__typename` roots, no DB round-trip at all.
        let result = executor.execute("{ a: __typename b: __typename }", None).await.unwrap();
        assert_eq!(result, serde_json::json!({ "data": { "a": "Query", "b": "Query" } }));
    }

    #[tokio::test]
    async fn test_root_typename_skipped_directive() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        // `@skip(if: true)` omits the meta-field, exactly like any other field.
        let result = executor.execute("{ __typename @skip(if: true) }", None).await.unwrap();
        assert_eq!(result, serde_json::json!({ "data": {} }));
    }

    #[tokio::test]
    async fn test_root_typename_excluded_directive() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let result = executor.execute("{ __typename @include(if: false) }", None).await.unwrap();
        assert_eq!(result, serde_json::json!({ "data": {} }));
    }

    #[tokio::test]
    async fn test_root_typename_kept_directive() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        // `@skip(if: false)` keeps the field — resolves to "Query".
        let result = executor.execute("{ __typename @skip(if: false) }", None).await.unwrap();
        assert_eq!(result, serde_json::json!({ "data": { "__typename": "Query" } }));
    }

    #[tokio::test]
    async fn test_root_typename_skip_via_variable() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        // Directive condition driven by a request variable.
        let vars = serde_json::json!({ "drop": true });
        let result = executor
            .execute("query($drop: Boolean!) { __typename @skip(if: $drop) }", Some(&vars))
            .await
            .unwrap();
        assert_eq!(result, serde_json::json!({ "data": {} }));
    }
}

// ── mod classify: query type detection ───────────────────────────────────

mod classify {
    use super::*;

    #[test]
    fn test_detect_introspection_schema() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let query = r"{ __schema { types { name } } }";
        assert_eq!(executor.classify_query(query).unwrap(), QueryType::IntrospectionSchema);
    }

    #[test]
    fn test_detect_introspection_type() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let query = r#"{ __type(name: "User") { fields { name } } }"#;
        assert_eq!(
            executor.classify_query(query).unwrap(),
            QueryType::IntrospectionType("User".to_string()),
        );
    }

    #[test]
    fn test_detect_root_typename() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let query = r"{ __typename }";
        match executor.classify_query(query).unwrap() {
            QueryType::TypeName {
                selection,
                operation_type,
            } => {
                assert_eq!(selection.response_key(), "__typename");
                assert_eq!(operation_type, "query");
            },
            other => panic!("expected TypeName, got {other:?}"),
        }
    }

    #[test]
    fn test_detect_root_typename_aliased() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        // The response key is the alias when one is provided.
        let query = r"{ ping: __typename }";
        match executor.classify_query(query).unwrap() {
            QueryType::TypeName {
                selection,
                operation_type,
            } => {
                assert_eq!(selection.response_key(), "ping");
                assert_eq!(operation_type, "query");
            },
            other => panic!("expected TypeName, got {other:?}"),
        }
    }

    #[test]
    fn test_detect_root_typename_on_mutation() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        // `mutation { __typename }` resolves to the Mutation root type — the
        // classifier branch must precede the mutation branch.
        let query = r"mutation { __typename }";
        match executor.classify_query(query).unwrap() {
            QueryType::TypeName {
                selection,
                operation_type,
            } => {
                assert_eq!(selection.response_key(), "__typename");
                assert_eq!(operation_type, "mutation");
            },
            other => panic!("expected TypeName, got {other:?}"),
        }
    }

    #[test]
    fn test_classify_typename_prefix_is_regular() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        // A field whose name merely begins with "__typename" must NOT be treated
        // as the meta-field — exact match only (mirrors the node substring guard).
        let query = r"{ __typenameExtra }";
        assert_eq!(executor.classify_query(query).unwrap(), QueryType::Regular);
    }

    #[test]
    fn test_classify_node_query_inline_id() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let query = r#"{ node(id: "VXNlcjoxMjM=") { ... on User { name } } }"#;
        assert!(matches!(executor.classify_query(query).unwrap(), QueryType::NodeQuery { .. }));
    }

    #[test]
    fn test_classify_node_query_with_variable() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let query = r"query GetNode($id: ID!) { node(id: $id) { id } }";
        assert!(matches!(executor.classify_query(query).unwrap(), QueryType::NodeQuery { .. }));
    }

    #[test]
    fn test_classify_node_query_extracts_inline_fragment_selections() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let query = r#"{ node(id: "VXNlcjoxMjM=") { ... on User { name email } } }"#;
        let qt = executor.classify_query(query).unwrap();
        match qt {
            QueryType::NodeQuery { selections } => {
                let names: Vec<&str> = selections.iter().map(|s| s.name.as_str()).collect();
                assert_eq!(names, vec!["name", "email"]);
            },
            other => panic!("expected NodeQuery, got {other:?}"),
        }
    }

    #[test]
    fn test_classify_node_query_direct_fields_without_fragment() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let query = r"query GetNode($id: ID!) { node(id: $id) { id name } }";
        let qt = executor.classify_query(query).unwrap();
        match qt {
            QueryType::NodeQuery { selections } => {
                let names: Vec<&str> = selections.iter().map(|s| s.name.as_str()).collect();
                assert_eq!(names, vec!["id", "name"]);
            },
            other => panic!("expected NodeQuery, got {other:?}"),
        }
    }

    #[test]
    fn test_classify_node_query_rejects_substring_match() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        // "nodeCounts" contains "node(" as a substring — must NOT match
        let query = r#"{ nodeCounts(id: "x") { total } }"#;
        assert_eq!(executor.classify_query(query).unwrap(), QueryType::Regular);
    }

    #[test]
    fn test_classify_introspection_type_extracts_name() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        // Standard double-quoted argument
        let q = r#"{ __type(name: "User") { name } }"#;
        assert_eq!(
            executor.classify_query(q).unwrap(),
            QueryType::IntrospectionType("User".to_string()),
        );

        // No space after colon
        let q2 = r#"{ __type(name:"Query") { name } }"#;
        assert_eq!(
            executor.classify_query(q2).unwrap(),
            QueryType::IntrospectionType("Query".to_string()),
        );
    }

    #[test]
    fn test_classify_no_false_positive_schema_in_comment() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        // __schema appears in a comment — should classify as Regular, not introspection.
        let q = "{ users { id } } # compare against __schema response";
        assert_eq!(executor.classify_query(q).unwrap(), QueryType::Regular);
    }

    #[test]
    fn test_classify_no_false_positive_service_in_argument() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        // "_service" appears as a string argument — must NOT route to federation.
        let q = r#"{ search(query: "_service_name") { id } }"#;
        assert_eq!(executor.classify_query(q).unwrap(), QueryType::Regular);
    }

    #[test]
    fn test_classify_no_false_positive_entities_alias() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        // "_entities" used as an alias — the actual field is "users", not _entities.
        // Must NOT route to federation.
        let q = r"{ _entities: users { id } }";
        assert_eq!(executor.classify_query(q).unwrap(), QueryType::Regular);
    }

    #[test]
    fn test_classify_federation_service() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let q = r"{ _service { sdl } }";
        assert_eq!(
            executor.classify_query(q).unwrap(),
            QueryType::Federation("_service".to_string()),
        );
    }

    #[test]
    fn test_classify_federation_entities() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let q = r#"{ _entities(representations: [{ __typename: "User", id: "1" }]) { ... on User { name } } }"#;
        assert_eq!(
            executor.classify_query(q).unwrap(),
            QueryType::Federation("_entities".to_string()),
        );
    }
}

// ── mod entities_authz: federation `_entities` authorization (Phase 03 C1b) ─
//
// The federation `_entities` resolver builds its own SQL in `fraiseql-federation`
// with no slot for a per-row RLS / `inject_params` predicate. Before C1b it applied
// none of the backing query's `requires_role` / RLS / `inject_params` gates, so any
// caller — including an anonymous one under an RLS-configured deployment — could
// resolve gated entities by id. It now fails closed; an authenticated request resolves
// RLS-/inject-backed types under the documented trusted-gateway assumption.
#[cfg(feature = "federation")]
mod entities_authz {
    use super::*;
    use crate::{
        schema::{FederationConfig, FederationEntity},
        security::DefaultRLSPolicy,
    };

    /// Federation-enabled schema exposing `User` (view `v_user`) via one query,
    /// configurable for the gates the `_entities` path enforces.
    fn entities_user_schema(
        requires_role: Option<&str>,
        inject_params: IndexMap<String, InjectedParamSource>,
    ) -> CompiledSchema {
        let mut schema = CompiledSchema::new();
        schema.federation = Some(FederationConfig {
            enabled: true,
            version: Some("v2".to_string()),
            entities: vec![FederationEntity {
                name:       "User".to_string(),
                key_fields: vec!["id".to_string()],
                ..Default::default()
            }],
            ..Default::default()
        });
        schema.queries.push(QueryDefinition {
            name: "users".to_string(),
            return_type: "User".to_string(),
            returns_list: true,
            nullable: false,
            arguments: Vec::new(),
            sql_source: Some("v_user".to_string()),
            description: None,
            auto_params: AutoParams::default(),
            deprecation: None,
            jsonb_column: "data".to_string(),
            relay: false,
            relay_cursor_column: None,
            relay_cursor_type: CursorType::default(),
            inject_params,
            cache_ttl_seconds: None,
            additional_views: vec![],
            requires_role: requires_role.map(str::to_string),
            rest_path: None,
            rest_method: None,
            native_columns: HashMap::new(),
        });
        schema.types.push({
            let mut t = TypeDefinition::new("User", "v_user");
            t.fields = vec![
                FieldDefinition::new("id", FieldType::String),
                FieldDefinition::new("name", FieldType::String),
            ];
            t
        });
        schema
    }

    /// `{ _entities(representations: ...) { ... on User { id name } } }`
    fn entities_query() -> &'static str {
        r#"{ _entities(representations: [{ __typename: "User", id: "1" }]) { ... on User { id name } } }"#
    }

    fn representations() -> serde_json::Value {
        serde_json::json!({
            "representations": [
                { "__typename": "User", "id": "11111111-1111-1111-1111-111111111111" }
            ]
        })
    }

    fn ctx_with_roles(roles: &[&str]) -> SecurityContext {
        SecurityContext {
            user_id:          "user-1".into(),
            roles:            roles.iter().map(|r| (*r).to_string()).collect(),
            tenant_id:        Some("tenant-abc".into()),
            scopes:           vec![],
            attributes:       HashMap::default(),
            request_id:       "req-1".to_string(),
            ip_address:       None,
            expires_at:       Utc::now() + chrono::Duration::hours(1),
            authenticated_at: Utc::now(),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        }
    }

    fn is_authz(err: &FraiseQLError) -> bool {
        matches!(err, FraiseQLError::Authorization { .. })
    }

    #[tokio::test]
    async fn entities_rls_anonymous_fails_closed() {
        let schema = entities_user_schema(None, IndexMap::new());
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let config = RuntimeConfig::default().with_rls_policy(Arc::new(DefaultRLSPolicy::new()));
        let executor = Executor::with_config(schema, adapter.clone(), config);

        let vars = representations();
        let err = executor.execute(entities_query(), Some(&vars)).await.unwrap_err();
        assert!(is_authz(&err), "anonymous _entities under RLS must be denied, got: {err}");
        assert!(
            adapter.captured_aggregate_sql().is_none(),
            "resolver must run no SQL when denied (fail closed)"
        );
    }

    #[tokio::test]
    async fn entities_requires_role_authenticated_without_role_denied() {
        let schema = entities_user_schema(Some("admin"), IndexMap::new());
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter.clone());

        let ctx = ctx_with_roles(&["viewer"]);
        let vars = representations();
        let err = executor
            .execute_with_security(entities_query(), Some(&vars), &ctx)
            .await
            .unwrap_err();
        assert!(is_authz(&err));
        assert!(adapter.captured_aggregate_sql().is_none());
    }

    #[tokio::test]
    async fn entities_requires_role_anonymous_denied() {
        let schema = entities_user_schema(Some("admin"), IndexMap::new());
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter.clone());

        let vars = representations();
        let err = executor.execute(entities_query(), Some(&vars)).await.unwrap_err();
        assert!(is_authz(&err));
        assert!(adapter.captured_aggregate_sql().is_none());
    }

    #[tokio::test]
    async fn entities_inject_anonymous_fails_closed() {
        let mut inject = IndexMap::new();
        inject.insert("tenant_id".to_string(), InjectedParamSource::Jwt("tenant_id".to_string()));
        let schema = entities_user_schema(None, inject);
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter.clone());

        let vars = representations();
        let err = executor.execute(entities_query(), Some(&vars)).await.unwrap_err();
        assert!(is_authz(&err));
        assert!(adapter.captured_aggregate_sql().is_none());
    }

    #[tokio::test]
    async fn entities_ungated_anonymous_resolves() {
        // No RLS, no inject, no role → the gate must not block; the resolver runs.
        let schema = entities_user_schema(None, IndexMap::new());
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter.clone());

        let vars = representations();
        let result = executor.execute(entities_query(), Some(&vars)).await.unwrap();
        assert!(result["data"].get("_entities").is_some());
        assert!(
            adapter.captured_aggregate_sql().is_some(),
            "ungated entity resolution must reach the resolver"
        );
    }

    #[tokio::test]
    async fn entities_rls_authenticated_resolves_trusted_gateway() {
        // Authenticated request: an app-level `rls_policy` (JSONB-shaped, targeting the
        // `data->>` view) cannot be composed onto the columnar federation entity table, so
        // it resolves under the documented trusted-gateway assumption. DB-native RLS keyed
        // on session variables is enforced separately (see entities_inject_* below).
        let schema = entities_user_schema(None, IndexMap::new());
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let config = RuntimeConfig::default().with_rls_policy(Arc::new(DefaultRLSPolicy::new()));
        let executor = Executor::with_config(schema, adapter.clone(), config);

        let ctx = ctx_with_roles(&["viewer"]);
        let vars = representations();
        let result = executor
            .execute_with_security(entities_query(), Some(&vars), &ctx)
            .await
            .unwrap();
        assert!(result["data"].get("_entities").is_some());
        assert!(adapter.captured_aggregate_sql().is_some());
    }

    #[tokio::test]
    async fn entities_inject_authenticated_composes_tenant_filter() {
        // C1b/R1 follow-up: an authenticated request for a tenant-scoped (`inject_params`)
        // type must have the per-row tenant predicate composed into the resolver SQL and the
        // caller's tenant bound as a parameter — not resolve every row under the
        // trusted-gateway assumption. The predicate is a columnar `NativeField`
        // (`"tenant_id" = $N`), ANDed onto the key `IN` clause.
        let mut inject = IndexMap::new();
        inject.insert("tenant_id".to_string(), InjectedParamSource::Jwt("tenant_id".to_string()));
        let schema = entities_user_schema(None, inject);
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter.clone());

        let ctx = ctx_with_roles(&["viewer"]); // tenant_id = "tenant-abc"
        let vars = representations();
        let result = executor
            .execute_with_security(entities_query(), Some(&vars), &ctx)
            .await
            .unwrap();
        assert!(result["data"].get("_entities").is_some());

        let sql = adapter.captured_aggregate_sql().expect("resolver must run");
        assert!(
            sql.contains("tenant_id"),
            "resolver SQL must compose the per-row tenant inject predicate, got: {sql}"
        );
        // The predicate is ANDed onto the key IN-clause with its placeholder numbered
        // after the IN params (one key → IN uses $1, the tenant predicate uses $2).
        assert!(
            sql.contains(" AND ") && sql.contains("$2"),
            "tenant predicate must be ANDed with a post-IN-clause placeholder, got: {sql}"
        );
        let params = adapter.captured_aggregate_params().unwrap_or_default();
        // Param order must match placeholder order: the key value first, the tenant second.
        assert_eq!(
            params.get(1),
            Some(&serde_json::json!("tenant-abc")),
            "the caller's tenant id must bind to $2 (offset past the IN clause), got: {params:?}"
        );
    }
}

// ── mod context: ExecutionContext lifecycle ───────────────────────────────

mod context {
    use super::*;

    #[test]
    fn test_execution_context_creation() {
        let ctx = ExecutionContext::new("query-123".to_string());
        assert_eq!(ctx.query_id(), "query-123");
        assert!(!ctx.is_cancelled());
    }

    #[test]
    fn test_execution_context_cancellation_token() {
        let ctx = ExecutionContext::new("query-456".to_string());
        let token = ctx.cancellation_token();
        assert!(!token.is_cancelled());

        // Cancel the token
        token.cancel();
        assert!(token.is_cancelled());
        assert!(ctx.is_cancelled());
    }

    #[tokio::test]
    async fn test_execute_with_context_success() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let ctx = ExecutionContext::new("test-query-1".to_string());
        let query = r"{ __schema { queryType { name } } }";

        let result = executor.execute_with_context(query, None, &ctx).await;
        let output = result.unwrap_or_else(|e| panic!("expected Ok for execute_with_context: {e}"));
        assert!(output["data"].get("__schema").is_some());
    }

    #[tokio::test]
    async fn test_execute_with_context_already_cancelled() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let ctx = ExecutionContext::new("test-query-2".to_string());
        let token = ctx.cancellation_token().clone();

        // Cancel before execution
        token.cancel();

        let query = r"{ __schema { queryType { name } } }";
        let result = executor.execute_with_context(query, None, &ctx).await;

        let err = result.expect_err("expected Err for already-cancelled context");
        match err {
            FraiseQLError::Cancelled { query_id, reason } => {
                assert_eq!(query_id, "test-query-2");
                assert!(reason.contains("before execution"));
            },
            e => panic!("Expected Cancelled error, got: {}", e),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn test_execute_with_context_cancelled_during_execution() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let ctx = ExecutionContext::new("test-query-3".to_string());
        let token = ctx.cancellation_token().clone();

        // Spawn a task to cancel after a short delay (instant with paused time)
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            token.cancel();
        });

        let query = r"{ __schema { queryType { name } } }";
        let result = executor.execute_with_context(query, None, &ctx).await;

        // Depending on timing, may succeed or be cancelled (both are acceptable)
        // But if cancelled, it should be our error
        if let Err(FraiseQLError::Cancelled { query_id, .. }) = result {
            assert_eq!(query_id, "test-query-3");
        }
    }

    #[test]
    fn test_execution_context_clone() {
        let ctx = ExecutionContext::new("query-clone".to_string());
        let ctx_clone = ctx.clone();

        assert_eq!(ctx.query_id(), ctx_clone.query_id());
        assert!(!ctx_clone.is_cancelled());

        // Cancel original
        ctx.cancellation_token().cancel();

        // Clone should also see cancellation (same token)
        assert!(ctx_clone.is_cancelled());
    }

    #[test]
    fn test_error_cancelled_constructor() {
        let err = FraiseQLError::cancelled("query-001", "user requested cancellation");

        assert!(err.to_string().contains("Query cancelled"));
        assert_eq!(err.status_code(), 408);
        assert_eq!(err.error_code(), "CANCELLED");
        assert!(err.is_retryable());
        // 408 is a 4xx (client-error) status; is_server_error() derives from
        // status_code() so it returns false. Retry semantics are the
        // load-bearing assertion for cancellation classification.
        assert!(err.is_client_error());
    }
}

// ── mod config: RuntimeConfig and JSONB optimization ─────────────────────

mod config {
    use super::*;

    #[test]
    fn test_jsonb_strategy_in_runtime_config() {
        let config = RuntimeConfig {
            cache_query_plans:    false,
            max_query_depth:      5,
            max_query_complexity: 500,
            max_page_size:        Some(1000),
            enable_tracing:       true,
            field_filter:         None,
            rls_policy:           None,
            field_authorizer:     None,
            authorizer:           None,
            query_timeout_ms:     30_000,
            jsonb_optimization:   JsonbOptimizationOptions::default(),
            query_validation:     None,
            audit_mutations:      false,
            changelog_enabled:    true,
        };

        assert_eq!(config.jsonb_optimization.default_strategy, JsonbStrategy::Project);
        assert_eq!(config.jsonb_optimization.auto_threshold_percent, 80);
    }

    #[test]
    fn test_jsonb_strategy_custom_config() {
        let custom_options = JsonbOptimizationOptions {
            default_strategy:       JsonbStrategy::Stream,
            auto_threshold_percent: 50,
        };

        let config = RuntimeConfig {
            cache_query_plans:    false,
            max_query_depth:      5,
            max_query_complexity: 500,
            max_page_size:        Some(1000),
            enable_tracing:       true,
            field_filter:         None,
            rls_policy:           None,
            field_authorizer:     None,
            authorizer:           None,
            query_timeout_ms:     30_000,
            jsonb_optimization:   custom_options,
            query_validation:     None,
            audit_mutations:      false,
            changelog_enabled:    true,
        };

        assert_eq!(config.jsonb_optimization.default_strategy, JsonbStrategy::Stream);
        assert_eq!(config.jsonb_optimization.auto_threshold_percent, 50);
    }
}

// ── mod inject: @inject parameter resolution (JWT claims → query params) ──

mod inject {
    use super::*;

    fn make_security_ctx(
        user_id: &str,
        tenant_id: Option<&str>,
        extra: &[(&str, serde_json::Value)],
    ) -> SecurityContext {
        use chrono::Utc;
        let now = Utc::now();
        SecurityContext {
            user_id:          crate::types::UserId::new(user_id),
            roles:            vec![],
            tenant_id:        tenant_id.map(crate::types::TenantId::new),
            scopes:           vec![],
            attributes:       extra.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect(),
            request_id:       "test-req".to_string(),
            ip_address:       None,
            authenticated_at: now,
            expires_at:       now + chrono::Duration::hours(1),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        }
    }

    #[test]
    fn test_resolve_inject_sub_maps_to_user_id() {
        let ctx = make_security_ctx("user-42", None, &[]);
        let source = InjectedParamSource::Jwt("sub".to_string());
        let result = resolve_inject_value("user_id", &source, &ctx).unwrap();
        assert_eq!(result, serde_json::Value::String("user-42".to_string()));
    }

    #[test]
    fn test_resolve_inject_tenant_id_claim() {
        let ctx = make_security_ctx("user-1", Some("tenant-abc"), &[]);
        let source = InjectedParamSource::Jwt("tenant_id".to_string());
        let result = resolve_inject_value("tenant_id", &source, &ctx).unwrap();
        assert_eq!(result, serde_json::Value::String("tenant-abc".to_string()));
    }

    #[test]
    fn test_resolve_inject_org_id_alias() {
        let ctx = make_security_ctx("user-1", Some("org-xyz"), &[]);
        let source = InjectedParamSource::Jwt("org_id".to_string());
        let result = resolve_inject_value("org_id", &source, &ctx).unwrap();
        assert_eq!(result, serde_json::Value::String("org-xyz".to_string()));
    }

    #[test]
    fn test_resolve_inject_custom_attribute() {
        let ctx =
            make_security_ctx("user-1", None, &[("department", serde_json::json!("engineering"))]);
        let source = InjectedParamSource::Jwt("department".to_string());
        let result = resolve_inject_value("dept", &source, &ctx).unwrap();
        assert_eq!(result, serde_json::Value::String("engineering".to_string()));
    }

    #[test]
    fn test_resolve_inject_missing_claim_returns_error() {
        let ctx = make_security_ctx("user-1", None, &[]);
        let source = InjectedParamSource::Jwt("org_id".to_string());
        let err = resolve_inject_value("org_id", &source, &ctx).unwrap_err();
        assert!(matches!(err, FraiseQLError::Validation { .. }));
        let msg = err.to_string();
        assert!(msg.contains("org_id"), "Error should mention claim name");
    }

    #[test]
    fn test_resolve_inject_missing_tenant_id_returns_error() {
        let ctx = make_security_ctx("user-1", None, &[]);
        let source = InjectedParamSource::Jwt("tenant_id".to_string());
        let err = resolve_inject_value("tenant_id", &source, &ctx).unwrap_err();
        assert!(matches!(err, FraiseQLError::Validation { .. }));
    }

    #[tokio::test]
    async fn test_query_with_inject_rejects_unauthenticated() {
        let mut schema = test_schema();
        let mut inject_params = IndexMap::new();
        inject_params.insert("org_id".to_string(), InjectedParamSource::Jwt("org_id".to_string()));
        schema.queries.push(QueryDefinition {
            name: "org_items".to_string(),
            return_type: "User".to_string(),
            returns_list: true,
            nullable: false,
            arguments: Vec::new(),
            sql_source: Some("v_org_items".to_string()),
            description: None,
            auto_params: AutoParams::default(),
            deprecation: None,
            jsonb_column: "data".to_string(),
            relay: false,
            relay_cursor_column: None,
            relay_cursor_type: CursorType::default(),
            inject_params,
            cache_ttl_seconds: None,
            additional_views: vec![],
            requires_role: None,
            rest_path: None,
            rest_method: None,
            native_columns: HashMap::new(),
        });
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        // Execute without security context — should fail with Validation error
        let result = executor.execute("{ org_items { id } }", None).await;
        let err = result.expect_err("Expected Err for unauthenticated inject query");
        assert!(
            matches!(err, FraiseQLError::Validation { .. }),
            "Expected Validation error, got: {err:?}"
        );
    }
}

// ── mod masking: null_masked_fields ──────────────────────────────────────

mod masking {
    use super::*;

    #[test]
    fn test_null_masked_fields_object() {
        let mut value = serde_json::json!({"id": 1, "email": "alice@example.com", "name": "Alice"});
        null_masked_fields(&mut value, &["email".to_string()]);
        assert_eq!(value, serde_json::json!({"id": 1, "email": null, "name": "Alice"}));
    }

    #[test]
    fn test_null_masked_fields_array() {
        let mut value = serde_json::json!([
            {"id": 1, "email": "a@b.com", "salary": 100_000},
            {"id": 2, "email": "c@d.com", "salary": 120_000},
        ]);
        null_masked_fields(&mut value, &["email".to_string(), "salary".to_string()]);
        assert_eq!(
            value,
            serde_json::json!([
                {"id": 1, "email": null, "salary": null},
                {"id": 2, "email": null, "salary": null},
            ])
        );
    }

    #[test]
    fn test_null_masked_fields_no_masked() {
        let mut value = serde_json::json!({"id": 1, "name": "Alice"});
        let original = value.clone();
        null_masked_fields(&mut value, &[]);
        assert_eq!(value, original);
    }
}

// ── mod planning: query plan generation ──────────────────────────────────

mod planning {
    use super::*;

    #[test]
    fn test_plan_query_regular() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let plan = executor.plan_query("{ users { id name } }", None).unwrap();
        assert_eq!(plan.query_type, "regular");
        assert!(plan.sql.contains("v_user"));
        assert_eq!(plan.views_accessed, vec!["v_user"]);
        assert!(plan.estimated_cost > 0);
    }

    #[test]
    fn test_plan_query_introspection() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let plan = executor.plan_query("{ __schema { types { name } } }", None).unwrap();
        assert_eq!(plan.query_type, "introspection");
        assert!(plan.sql.is_empty());
        assert!(plan.views_accessed.is_empty());
    }

    #[test]
    fn test_plan_query_empty_rejected() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let result = executor.plan_query("", None);
        assert!(result.is_err(), "expected Err for empty query, got: {result:?}");
    }
}

// ── mod security: DoS protection (alias / depth / complexity limits) ──────

mod security {
    // R10: Alias limit enforced independently of depth/complexity flags ─────

    /// When both depth and complexity validation are disabled, the alias limit
    /// must still be enforced. This tests that the alias check is NOT inside
    /// a depth/complexity gate and will catch alias amplification attacks even
    /// when other limits are turned off.
    #[test]
    fn test_alias_limit_enforced_when_depth_and_complexity_disabled() {
        use crate::graphql::complexity::{ComplexityValidationError, RequestValidator};

        let validator = RequestValidator::new()
            .with_depth_validation(false)
            .with_complexity_validation(false)
            .with_max_aliases(2);

        // 3 aliases — exceeds limit of 2.
        let query = "query { a: users { id } b: users { id } c: users { id } }";
        let result = validator.validate_query(query);

        let err = result
            .expect_err("alias limit must be enforced even when depth and complexity are disabled");
        assert!(
            matches!(
                err,
                ComplexityValidationError::TooManyAliases {
                    actual_aliases: 3,
                    ..
                }
            ),
            "error must be TooManyAliases with actual_aliases = 3, got: {err:?}"
        );
    }

    /// When aliases are within the limit, the query must pass even with other
    /// limits disabled — verifying that alias-disable=false doesn't block valid queries.
    #[test]
    fn test_alias_within_limit_passes_when_depth_and_complexity_disabled() {
        use crate::graphql::complexity::RequestValidator;

        let validator = RequestValidator::new()
            .with_depth_validation(false)
            .with_complexity_validation(false)
            .with_max_aliases(5);

        // 2 aliases — within limit of 5.
        let query = "query { a: users { id } b: users { id } }";
        validator.validate_query(query).unwrap_or_else(|e| {
            panic!(
                "query within alias limit must pass when depth and complexity are disabled: {e:?}"
            )
        });
    }
}

// ── mod field_rbac: C16+C17 — RBAC reject/mask through executor ──────────

mod field_rbac {
    use super::*;

    fn schema_with_rbac_fields() -> CompiledSchema {
        let mut schema = CompiledSchema::new();
        schema.queries.push(QueryDefinition {
            name:                "users".to_string(),
            return_type:         "User".to_string(),
            returns_list:        true,
            nullable:            false,
            arguments:           Vec::new(),
            sql_source:          Some("v_user".to_string()),
            description:         None,
            auto_params:         AutoParams::default(),
            deprecation:         None,
            jsonb_column:        "data".to_string(),
            relay:               false,
            relay_cursor_column: None,
            relay_cursor_type:   CursorType::default(),
            inject_params:       IndexMap::default(),
            cache_ttl_seconds:   None,
            additional_views:    vec![],
            requires_role:       None,
            rest_path:           None,
            rest_method:         None,
            native_columns:      HashMap::new(),
        });
        let mut user_type = TypeDefinition::new("User", "v_user");
        user_type.fields = vec![
            FieldDefinition {
                name:           "id".into(),
                field_type:     FieldType::Int,
                nullable:       false,
                description:    None,
                default_value:  None,
                vector_config:  None,
                alias:          None,
                deprecation:    None,
                requires_scope: None,
                on_deny:        FieldDenyPolicy::Reject,
                authorize:      false,
                encryption:     None,
                hierarchy:      None,
            },
            FieldDefinition {
                name:           "name".into(),
                field_type:     FieldType::String,
                nullable:       false,
                description:    None,
                default_value:  None,
                vector_config:  None,
                alias:          None,
                deprecation:    None,
                requires_scope: None,
                on_deny:        FieldDenyPolicy::Reject,
                authorize:      false,
                encryption:     None,
                hierarchy:      None,
            },
            // Protected field: reject when unauthorized
            FieldDefinition {
                name:           "salary".into(),
                field_type:     FieldType::Int,
                nullable:       true,
                description:    None,
                default_value:  None,
                vector_config:  None,
                alias:          None,
                deprecation:    None,
                requires_scope: Some("admin:*".to_string()),
                on_deny:        FieldDenyPolicy::Reject,
                authorize:      false,
                encryption:     None,
                hierarchy:      None,
            },
            // Protected field: mask when unauthorized
            FieldDefinition {
                name:           "email".into(),
                field_type:     FieldType::String,
                nullable:       true,
                description:    None,
                default_value:  None,
                vector_config:  None,
                alias:          None,
                deprecation:    None,
                requires_scope: Some("read:User.email".to_string()),
                on_deny:        FieldDenyPolicy::Mask,
                authorize:      false,
                encryption:     None,
                hierarchy:      None,
            },
        ];

        // Set up security config with role definitions for scope-based RBAC
        schema.security = Some(SecurityConfig {
            role_definitions: vec![
                RoleDefinition {
                    name:        "viewer".into(),
                    description: None,
                    scopes:      vec!["read:User".into()],
                },
                RoleDefinition {
                    name:        "admin".into(),
                    description: None,
                    scopes:      vec!["admin:*".into(), "read:User.email".into()],
                },
            ],
            default_role:     None,
            multi_tenant:     false,
            tenancy:          TenancyConfig::default(),
            additional:       HashMap::default(),
        });

        schema.types.push(user_type);
        schema
    }

    fn viewer_context() -> SecurityContext {
        SecurityContext {
            user_id:          "user-42".into(),
            roles:            vec!["viewer".to_string()],
            tenant_id:        None,
            scopes:           vec!["read:User".to_string()],
            attributes:       HashMap::default(),
            request_id:       "req-001".to_string(),
            ip_address:       None,
            expires_at:       Utc::now() + chrono::Duration::hours(1),
            authenticated_at: Utc::now(),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        }
    }

    fn admin_context() -> SecurityContext {
        SecurityContext {
            user_id:          "admin-1".into(),
            roles:            vec!["admin".to_string()],
            tenant_id:        None,
            scopes:           vec!["admin:*".to_string(), "read:User.email".to_string()],
            attributes:       HashMap::default(),
            request_id:       "req-002".to_string(),
            ip_address:       None,
            expires_at:       Utc::now() + chrono::Duration::hours(1),
            authenticated_at: Utc::now(),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        }
    }

    /// C16: Querying a rejected field as unauthorized user returns Authorization error
    #[tokio::test]
    async fn test_reject_field_returns_authorization_error() {
        let schema = schema_with_rbac_fields();
        let adapter = Arc::new(MockAdapter::new(mock_user_results()));
        let config = RuntimeConfig::default();
        let executor = Executor::with_config(schema, adapter, config);

        let ctx = viewer_context();
        let result = executor.execute_with_security("{ users { id salary } }", None, &ctx).await;

        assert!(result.is_err(), "querying rejected field should fail");
        let err = result.unwrap_err();
        let err_msg = format!("{err}");
        assert!(
            err_msg.contains("salary")
                || err_msg.contains("authorization")
                || err_msg.contains("Authorization")
                || err_msg.contains("forbidden")
                || err_msg.contains("Forbidden"),
            "error should mention the forbidden field or authorization, got: {err_msg}"
        );
    }

    /// C16b: Querying a rejected field as admin succeeds
    #[tokio::test]
    async fn test_reject_field_allowed_for_admin() {
        let schema = schema_with_rbac_fields();
        let adapter = Arc::new(MockAdapter::new(mock_user_results()));
        let config = RuntimeConfig::default();
        let executor = Executor::with_config(schema, adapter, config);

        let ctx = admin_context();
        let result = executor.execute_with_security("{ users { id salary } }", None, &ctx).await;

        assert!(
            result.is_ok(),
            "admin should be able to query rejected field: {:?}",
            result.err()
        );
    }

    /// C17: Querying a masked field as unauthorized user returns null
    #[tokio::test]
    async fn test_mask_field_returns_null_for_unauthorized() {
        let schema = schema_with_rbac_fields();
        let results = vec![JsonbValue::new(
            serde_json::json!({"id": 1, "name": "Alice", "email": "alice@example.com"}),
        )];
        let adapter = Arc::new(MockAdapter::new(results));
        let config = RuntimeConfig::default();
        let executor = Executor::with_config(schema, adapter, config);

        let ctx = viewer_context();
        let result = executor
            .execute_with_security("{ users { id email } }", None, &ctx)
            .await
            .unwrap();

        // Verify masking using Value directly
        let users = &result["data"]["users"];
        assert!(users.is_array(), "expected users array in response: {result}");
        for user in users.as_array().unwrap() {
            assert!(
                user["email"].is_null(),
                "masked field 'email' should be null for unauthorized user, got: {}",
                user["email"]
            );
            // id should still have real value
            assert!(!user["id"].is_null(), "unmasked field 'id' should have real value");
        }
    }

    /// C17b: Querying a masked field as authorized user returns real value
    #[tokio::test]
    async fn test_mask_field_returns_real_value_for_authorized() {
        let schema = schema_with_rbac_fields();
        let results = vec![JsonbValue::new(
            serde_json::json!({"id": 1, "name": "Alice", "email": "alice@example.com"}),
        )];
        let adapter = Arc::new(MockAdapter::new(results));
        let config = RuntimeConfig::default();
        let executor = Executor::with_config(schema, adapter, config);

        let ctx = admin_context();
        let result = executor
            .execute_with_security("{ users { id email } }", None, &ctx)
            .await
            .unwrap();

        let users = &result["data"]["users"];
        for user in users.as_array().unwrap() {
            assert_eq!(
                user["email"], "alice@example.com",
                "authorized user should see real email value"
            );
        }
    }

    /// C16+C17: Public fields always accessible
    #[tokio::test]
    async fn test_public_fields_always_accessible() {
        let schema = schema_with_rbac_fields();
        let adapter = Arc::new(MockAdapter::new(mock_user_results()));
        let config = RuntimeConfig::default();
        let executor = Executor::with_config(schema, adapter, config);

        let ctx = viewer_context();
        let result = executor.execute_with_security("{ users { id name } }", None, &ctx).await;

        assert!(result.is_ok(), "public fields should always be accessible: {:?}", result.err());
    }
}

// ── mod executor_paths: H4 — requires_role anti-enumeration tests ─────────

mod executor_paths {
    use super::*;

    /// H4: `requires_role` returns "not found" (anti-enumeration), not "forbidden"
    #[tokio::test]
    async fn test_requires_role_returns_not_found_not_forbidden() {
        let mut schema = test_schema();
        schema.queries[0].requires_role = Some("admin".to_string());
        let adapter = Arc::new(MockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter);

        // No security context at all → should say "not found"
        let result = executor.execute("{ users { id } }", None).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not found in schema"),
            "requires_role should produce 'not found', not 'forbidden', got: {err}"
        );
        assert!(
            !err.contains("forbidden") && !err.contains("Forbidden"),
            "must not reveal the query exists behind a role gate, got: {err}"
        );
    }

    /// H4: `requires_role` with wrong role still returns "not found"
    #[tokio::test]
    async fn test_requires_role_wrong_role_returns_not_found() {
        let mut schema = test_schema();
        schema.queries[0].requires_role = Some("admin".to_string());
        let adapter = Arc::new(MockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter);

        let ctx = SecurityContext {
            user_id:          "user-42".into(),
            roles:            vec!["viewer".to_string()],
            tenant_id:        None,
            scopes:           vec![],
            attributes:       HashMap::default(),
            request_id:       "req-001".to_string(),
            ip_address:       None,
            expires_at:       Utc::now() + chrono::Duration::hours(1),
            authenticated_at: Utc::now(),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        };
        let result = executor.execute_with_security("{ users { id } }", None, &ctx).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not found in schema"),
            "wrong role should produce 'not found', got: {err}"
        );
    }

    /// H4: `requires_role` with correct role succeeds
    #[tokio::test]
    async fn test_requires_role_correct_role_succeeds() {
        let mut schema = test_schema();
        schema.queries[0].requires_role = Some("admin".to_string());
        let adapter = Arc::new(MockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter);

        let ctx = SecurityContext {
            user_id:          "admin-1".into(),
            roles:            vec!["admin".to_string()],
            tenant_id:        None,
            scopes:           vec![],
            attributes:       HashMap::default(),
            request_id:       "req-002".to_string(),
            ip_address:       None,
            expires_at:       Utc::now() + chrono::Duration::hours(1),
            authenticated_at: Utc::now(),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        };
        let result = executor.execute_with_security("{ users { id } }", None, &ctx).await;
        assert!(result.is_ok(), "correct role should succeed: {:?}", result.err());
    }
}

// ── mod parse_cache: AST cache behaviour ─────────────────────────────────

mod parse_cache {
    use super::*;

    #[tokio::test]
    async fn test_cache_empty_before_first_execute() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter);

        assert_eq!(executor.parse_cache_entry_count(), 0, "cache must be empty before any call");
    }

    #[tokio::test]
    async fn test_cache_populated_after_first_execute() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter);

        executor.execute("{ users { id name } }", None).await.unwrap();

        // moka may apply a brief maintenance delay; run_pending_tasks() drains it.
        executor.ctx.parse_cache.run_pending_tasks();
        assert_eq!(
            executor.parse_cache_entry_count(),
            1,
            "one distinct query must produce exactly one cache entry"
        );
    }

    #[tokio::test]
    async fn test_cache_no_double_insert_for_repeated_query() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter);

        let query = "{ users { id name } }";
        executor.execute(query, None).await.unwrap();
        executor.execute(query, None).await.unwrap();
        executor.execute(query, None).await.unwrap();

        executor.ctx.parse_cache.run_pending_tasks();
        assert_eq!(
            executor.parse_cache_entry_count(),
            1,
            "repeating the same query must not grow the cache beyond 1 entry"
        );
    }

    #[tokio::test]
    async fn test_cache_separate_entries_for_distinct_queries() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter);

        executor.execute("{ users { id name } }", None).await.unwrap();
        executor.execute("{ users { id } }", None).await.unwrap();

        executor.ctx.parse_cache.run_pending_tasks();
        assert_eq!(
            executor.parse_cache_entry_count(),
            2,
            "two distinct query strings must produce two cache entries"
        );
    }
}

// ── mod field_authz: #423 dynamic field-level authorization ──────────────

mod field_authz {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use super::*;
    use crate::{
        cache::{ResponseCache, ResponseCacheConfig},
        error::{FraiseQLError, Result as FqlResult},
        security::{FieldAuthorizer, FieldAuthzDecision, FieldAuthzRequest},
    };

    // ---- reference authorizers --------------------------------------------

    struct AllowAll;
    impl FieldAuthorizer for AllowAll {
        fn authorize_field(&self, _req: &FieldAuthzRequest<'_>) -> FqlResult<FieldAuthzDecision> {
            Ok(FieldAuthzDecision::Allow)
        }
    }

    struct DenyReject;
    impl FieldAuthorizer for DenyReject {
        fn authorize_field(&self, _req: &FieldAuthzRequest<'_>) -> FqlResult<FieldAuthzDecision> {
            Ok(FieldAuthzDecision::Deny {
                code:    "nope".into(),
                on_deny: FieldDenyPolicy::Reject,
            })
        }
    }

    struct Raising;
    impl FieldAuthorizer for Raising {
        fn authorize_field(&self, _req: &FieldAuthzRequest<'_>) -> FqlResult<FieldAuthzDecision> {
            Err(FraiseQLError::Validation {
                message: "policy backend down".into(),
                path:    None,
            })
        }
    }

    struct OwnerOnly;
    impl FieldAuthorizer for OwnerOnly {
        fn authorize_field(&self, req: &FieldAuthzRequest<'_>) -> FqlResult<FieldAuthzDecision> {
            let owner = req.parent.and_then(|p| p.get("owner_id")).and_then(|v| v.as_str());
            if owner == Some(req.principal.user_id.as_str()) {
                Ok(FieldAuthzDecision::Allow)
            } else {
                Ok(FieldAuthzDecision::Deny {
                    code:    "not_owner".into(),
                    on_deny: FieldDenyPolicy::Mask,
                })
            }
        }
    }

    /// Panics if consulted — proves the authorizer is NOT called when nothing is gated.
    struct PanicIfCalled;
    impl FieldAuthorizer for PanicIfCalled {
        fn authorize_field(&self, _req: &FieldAuthzRequest<'_>) -> FqlResult<FieldAuthzDecision> {
            panic!("field authorizer must not be consulted here");
        }
    }

    /// First call → Allow, every later call → Deny(Mask). Proves the response cache is
    /// bypassed for gated queries (D5b): a stale `Allow` is never replayed.
    struct FlipAuthorizer {
        calls: AtomicUsize,
    }
    impl FieldAuthorizer for FlipAuthorizer {
        fn authorize_field(&self, _req: &FieldAuthzRequest<'_>) -> FqlResult<FieldAuthzDecision> {
            if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                Ok(FieldAuthzDecision::Allow)
            } else {
                Ok(FieldAuthzDecision::Deny {
                    code:    "later".into(),
                    on_deny: FieldDenyPolicy::Mask,
                })
            }
        }
    }

    // ---- schema / fixtures ------------------------------------------------

    fn users_query() -> QueryDefinition {
        QueryDefinition {
            name:                "users".to_string(),
            return_type:         "User".to_string(),
            returns_list:        true,
            nullable:            false,
            arguments:           Vec::new(),
            sql_source:          Some("v_user".to_string()),
            description:         None,
            auto_params:         AutoParams::default(),
            deprecation:         None,
            jsonb_column:        "data".to_string(),
            relay:               false,
            relay_cursor_column: None,
            relay_cursor_type:   CursorType::default(),
            inject_params:       IndexMap::default(),
            cache_ttl_seconds:   None,
            additional_views:    vec![],
            requires_role:       None,
            rest_path:           None,
            rest_method:         None,
            native_columns:      HashMap::new(),
        }
    }

    /// `User` type with a policy-gated `email` field. `email_scope` layers a static
    /// `requires_scope` (with the given deny policy) on top for AND-composition tests.
    fn gated_schema(email_scope: Option<(&str, FieldDenyPolicy)>) -> CompiledSchema {
        let mut schema = CompiledSchema::new();
        schema.queries.push(users_query());

        let mut email = FieldDefinition::nullable("email", FieldType::String).with_authorize(true);
        if let Some((scope, on_deny)) = email_scope {
            email = email.with_requires_scope(scope).with_on_deny(on_deny);
        }

        let mut user_type = TypeDefinition::new("User", "v_user");
        user_type.fields = vec![
            FieldDefinition::new("id", FieldType::Int),
            FieldDefinition::nullable("name", FieldType::String),
            email,
            FieldDefinition::nullable("owner_id", FieldType::String),
        ];

        schema.security = Some(SecurityConfig {
            role_definitions: vec![RoleDefinition {
                name:        "viewer".into(),
                description: None,
                scopes:      vec!["read:User".into()],
            }],
            default_role:     None,
            multi_tenant:     false,
            tenancy:          TenancyConfig::default(),
            additional:       HashMap::default(),
        });

        schema.types.push(user_type);
        schema
    }

    fn ctx(user_id: &str) -> SecurityContext {
        SecurityContext {
            user_id:          user_id.into(),
            roles:            vec!["viewer".to_string()],
            tenant_id:        None,
            scopes:           vec!["read:User".to_string()],
            attributes:       HashMap::default(),
            request_id:       "req-authz".to_string(),
            ip_address:       None,
            expires_at:       Utc::now() + chrono::Duration::hours(1),
            authenticated_at: Utc::now(),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        }
    }

    fn rows() -> Vec<JsonbValue> {
        vec![
            JsonbValue::new(serde_json::json!({
                "id": 1, "name": "Alice", "email": "alice@x.com", "owner_id": "user-1"
            })),
            JsonbValue::new(serde_json::json!({
                "id": 2, "name": "Bob", "email": "bob@x.com", "owner_id": "user-2"
            })),
        ]
    }

    // ---- tests ------------------------------------------------------------

    // HONESTY-1: a raising policy denies the whole query (403); the value is never served.
    #[tokio::test]
    async fn raising_policy_denies_query() {
        let executor = Executor::with_config(
            gated_schema(None),
            Arc::new(MockAdapter::new(rows())),
            RuntimeConfig::default().with_field_authorizer(Arc::new(Raising)),
        );
        let result = executor
            .execute_with_security("{ users { id email } }", None, &ctx("user-1"))
            .await;
        assert!(result.is_err(), "raising policy must fail closed");
        let msg = format!("{}", result.unwrap_err());
        assert!(!msg.contains("alice@x.com"), "must never leak the field value: {msg}");
    }

    // HONESTY-2: an explicit Deny{Reject} is a 403 and names the field.
    #[tokio::test]
    async fn deny_reject_returns_authorization() {
        let executor = Executor::with_config(
            gated_schema(None),
            Arc::new(MockAdapter::new(rows())),
            RuntimeConfig::default().with_field_authorizer(Arc::new(DenyReject)),
        );
        let result = executor
            .execute_with_security("{ users { id email } }", None, &ctx("user-1"))
            .await;
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("email"), "error should name the denied field: {msg}");
    }

    // NO-AUTHZ-CONFIGURED: a gated field selected with no authorizer configured → fail closed.
    #[tokio::test]
    async fn gated_field_without_authorizer_fails_closed() {
        let executor = Executor::with_config(
            gated_schema(None),
            Arc::new(MockAdapter::new(rows())),
            RuntimeConfig::default(), // no field_authorizer
        );
        let result = executor
            .execute_with_security("{ users { id email } }", None, &ctx("user-1"))
            .await;
        assert!(result.is_err(), "gated field with no authorizer must fail closed");
    }

    // OWNER: Deny{Mask} unless parent.owner_id == principal → per-row masking; proves the
    // `parent` carries the *unselected* owner_id (full-row fetch, D3).
    #[tokio::test]
    async fn owner_only_masks_non_owner_rows() {
        let executor = Executor::with_config(
            gated_schema(None),
            Arc::new(MockAdapter::new(rows())),
            RuntimeConfig::default().with_field_authorizer(Arc::new(OwnerOnly)),
        );
        let result = executor
            .execute_with_security("{ users { id email } }", None, &ctx("user-1"))
            .await
            .unwrap();
        let users = result["data"]["users"].as_array().unwrap();
        assert_eq!(users[0]["email"], "alice@x.com", "owner row keeps the field");
        assert!(users[1]["email"].is_null(), "non-owner row is masked: {}", users[1]);
    }

    // PASSTHROUGH: no gated field selected → authorizer never consulted, normal response.
    #[tokio::test]
    async fn no_gated_field_selected_skips_authorizer() {
        let executor = Executor::with_config(
            gated_schema(None),
            Arc::new(MockAdapter::new(rows())),
            RuntimeConfig::default().with_field_authorizer(Arc::new(PanicIfCalled)),
        );
        let result = executor
            .execute_with_security("{ users { id name } }", None, &ctx("user-1"))
            .await
            .unwrap();
        let users = result["data"]["users"].as_array().unwrap();
        assert_eq!(users[0]["name"], "Alice");
        assert!(users[0].get("email").is_none(), "email not selected → absent");
    }

    // AND-1: static requires_scope Reject fires before the dynamic authorizer.
    #[tokio::test]
    async fn static_reject_precedes_dynamic_allow() {
        let executor = Executor::with_config(
            gated_schema(Some(("admin:*", FieldDenyPolicy::Reject))),
            Arc::new(MockAdapter::new(rows())),
            RuntimeConfig::default().with_field_authorizer(Arc::new(AllowAll)),
        );
        // viewer lacks admin:* → static Reject → 403, even though the dynamic policy allows.
        let result = executor
            .execute_with_security("{ users { id email } }", None, &ctx("user-1"))
            .await;
        assert!(result.is_err(), "static reject must win over a dynamic Allow");
    }

    // AND-2: static Mask already nulled the field → dynamic authorizer is not consulted.
    #[tokio::test]
    async fn static_mask_skips_dynamic_authorizer() {
        let executor = Executor::with_config(
            gated_schema(Some(("read:User.email", FieldDenyPolicy::Mask))),
            Arc::new(MockAdapter::new(rows())),
            // PanicIfCalled would blow up if the dynamic pass ran on the masked field.
            RuntimeConfig::default().with_field_authorizer(Arc::new(PanicIfCalled)),
        );
        let result = executor
            .execute_with_security("{ users { id email } }", None, &ctx("user-1"))
            .await
            .unwrap();
        let users = result["data"]["users"].as_array().unwrap();
        assert!(users[0]["email"].is_null(), "statically-masked field stays null");
    }

    // CACHE-BYPASS (D5b): with the response cache enabled, a gated query is never cached,
    // so a policy that flips Allow→Deny between calls is honoured fresh each time.
    #[tokio::test]
    async fn response_cache_bypassed_for_gated_query() {
        let cache = Arc::new(ResponseCache::new(ResponseCacheConfig {
            enabled:     true,
            max_entries: 100,
            ttl_seconds: 3600,
        }));
        let executor = Executor::with_config(
            gated_schema(None),
            Arc::new(MockAdapter::new(rows())),
            RuntimeConfig::default().with_field_authorizer(Arc::new(FlipAuthorizer {
                calls: AtomicUsize::new(0),
            })),
        )
        .with_response_cache(cache);

        let q = "{ users { id email } }";
        let first = executor.execute_with_security(q, None, &ctx("user-1")).await.unwrap();
        let second = executor.execute_with_security(q, None, &ctx("user-1")).await.unwrap();

        // First call → Allow (email present); second → Deny(Mask) (email null).
        // If the cache were used, the second would replay the first.
        assert_eq!(first["data"]["users"][0]["email"], "alice@x.com");
        assert!(
            second["data"]["users"][0]["email"].is_null(),
            "a stale cached Allow must not be replayed for a gated query"
        );
    }

    // PATH COVERAGE (#423): the unauthenticated query path has no principal, so a
    // gated field cannot be authorized — it fails closed even with a permissive
    // authorizer configured.
    #[tokio::test]
    async fn anonymous_query_with_gated_field_fails_closed() {
        let executor = Executor::with_config(
            gated_schema(None),
            Arc::new(MockAdapter::new(rows())),
            RuntimeConfig::default().with_field_authorizer(Arc::new(AllowAll)),
        );
        let res = executor.execute("{ users { id email } }", None).await;
        assert!(res.is_err(), "unauthenticated query selecting a gated field must fail closed");
    }

    // PATH COVERAGE (#423): the Relay `node` lookup has no SecurityContext and emits
    // the entity blob directly — a gated resolved type fails closed.
    #[tokio::test]
    async fn node_query_with_gated_type_fails_closed() {
        let executor = Executor::with_config(
            gated_schema(None),
            Arc::new(MockAdapter::new(rows())),
            RuntimeConfig::default().with_field_authorizer(Arc::new(AllowAll)),
        );
        // node(id: base64("User:123")) → resolved type "User" has a gated field.
        let res = executor
            .execute(r#"{ node(id: "VXNlcjoxMjM=") { ... on User { email } } }"#, None)
            .await;
        assert!(res.is_err(), "node lookup of a gated type must fail closed");
    }

    // The schema-level gated-field helpers used by the default-deny guards.
    #[test]
    fn schema_gated_field_helpers() {
        let gated = gated_schema(None);
        assert!(gated.type_has_gated_field("User"));
        assert!(!gated.type_has_gated_field("Unknown"));
        assert!(gated.has_any_authorize_field());

        // A schema with no policy-gated fields.
        assert!(!test_schema().has_any_authorize_field());
    }
}

// ── mod operation_authz: #422 operation-level authorization ──────────────

mod operation_authz {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::{
        cache::{ResponseCache, ResponseCacheConfig},
        error::{FraiseQLError, Result as FqlResult},
        schema::{MutationDefinition, MutationOperation},
        security::{Authorizer, AuthzDecision, AuthzRequest},
    };

    // ---- reference authorizers --------------------------------------------

    struct AllowAll;
    impl Authorizer for AllowAll {
        fn authorize(&self, _req: &AuthzRequest<'_>) -> FqlResult<AuthzDecision> {
            Ok(AuthzDecision::Allow)
        }
    }

    struct DenyAll;
    impl Authorizer for DenyAll {
        fn authorize(&self, _req: &AuthzRequest<'_>) -> FqlResult<AuthzDecision> {
            Ok(AuthzDecision::Deny {
                reason: "nope".into(),
            })
        }
    }

    struct Raising;
    impl Authorizer for Raising {
        fn authorize(&self, _req: &AuthzRequest<'_>) -> FqlResult<AuthzDecision> {
            Err(FraiseQLError::Validation {
                message: "policy backend down".into(),
                path:    None,
            })
        }
    }

    /// Denies one named operation, allows all others.
    struct DenyNamed(&'static str);
    impl Authorizer for DenyNamed {
        fn authorize(&self, req: &AuthzRequest<'_>) -> FqlResult<AuthzDecision> {
            if req.name == self.0 {
                Ok(AuthzDecision::Deny {
                    reason: "named-deny".into(),
                })
            } else {
                Ok(AuthzDecision::Allow)
            }
        }
    }

    /// Allows only authenticated requests; denies anonymous (`principal == None`).
    struct RequireAuthenticated;
    impl Authorizer for RequireAuthenticated {
        fn authorize(&self, req: &AuthzRequest<'_>) -> FqlResult<AuthzDecision> {
            if req.principal.is_some() {
                Ok(AuthzDecision::Allow)
            } else {
                Ok(AuthzDecision::Deny {
                    reason: "auth required".into(),
                })
            }
        }
    }

    /// First call → Allow, every later call → Deny. Proves the gate runs BEFORE the
    /// response cache: a stale `Allow` is never replayed for a now-denied operation.
    struct Flip {
        calls: AtomicUsize,
    }
    impl Authorizer for Flip {
        fn authorize(&self, _req: &AuthzRequest<'_>) -> FqlResult<AuthzDecision> {
            if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                Ok(AuthzDecision::Allow)
            } else {
                Ok(AuthzDecision::Deny {
                    reason: "later".into(),
                })
            }
        }
    }

    /// Panics if consulted — proves the gate is a no-op when no authorizer is set.
    struct PanicIfCalled;
    impl Authorizer for PanicIfCalled {
        fn authorize(&self, _req: &AuthzRequest<'_>) -> FqlResult<AuthzDecision> {
            panic!("authorizer must not be consulted");
        }
    }

    // ---- fixtures ---------------------------------------------------------

    fn ctx(user_id: &str) -> SecurityContext {
        SecurityContext {
            user_id:          user_id.into(),
            roles:            vec![],
            tenant_id:        None,
            scopes:           vec![],
            attributes:       HashMap::default(),
            request_id:       "req-op-authz".to_string(),
            ip_address:       None,
            expires_at:       Utc::now() + chrono::Duration::hours(1),
            authenticated_at: Utc::now(),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        }
    }

    fn admin_ctx(user_id: &str) -> SecurityContext {
        let mut c = ctx(user_id);
        c.roles = vec!["admin".to_string()];
        c
    }

    /// A `users` query gated by `requires_role = "admin"`.
    fn schema_requires_admin() -> CompiledSchema {
        let mut schema = test_schema();
        schema.queries[0].requires_role = Some("admin".to_string());
        schema
    }

    /// A schema with a `createUser` insert mutation.
    fn schema_with_mutation() -> CompiledSchema {
        let mut schema = CompiledSchema::new();
        let mut def = MutationDefinition::new("createUser", "User");
        def.sql_source = Some("fn_create_user".to_string());
        def.operation = MutationOperation::Insert {
            table: "users".to_string(),
        };
        schema.mutations.push(def);
        schema
    }

    fn is_authz(err: &FraiseQLError) -> bool {
        matches!(err, FraiseQLError::Authorization { .. })
    }

    // ---- authenticated query path -----------------------------------------

    // HONESTY: a Deny on the authenticated path → 403; the data is never served.
    #[tokio::test]
    async fn authenticated_deny_returns_403() {
        let executor = Executor::with_config(
            test_schema(),
            Arc::new(MockAdapter::new(mock_user_results())),
            RuntimeConfig::default().with_authorizer(Arc::new(DenyAll)),
        );
        let err = executor
            .execute_with_security("{ users { id name } }", None, &ctx("u1"))
            .await
            .unwrap_err();
        assert!(is_authz(&err), "deny must map to Authorization/403: {err:?}");
        assert!(!format!("{err}").contains("Alice"), "must not leak data");
    }

    // HONESTY: a raising authorizer fails closed → 403; the policy error is not leaked.
    #[tokio::test]
    async fn authenticated_raise_fails_closed() {
        let executor = Executor::with_config(
            test_schema(),
            Arc::new(MockAdapter::new(mock_user_results())),
            RuntimeConfig::default().with_authorizer(Arc::new(Raising)),
        );
        let err = executor
            .execute_with_security("{ users { id name } }", None, &ctx("u1"))
            .await
            .unwrap_err();
        assert!(is_authz(&err), "raise must fail closed to Authorization/403: {err:?}");
        assert!(!format!("{err}").contains("backend down"), "policy error must not leak");
    }

    // ALLOW passes through to a normal response.
    #[tokio::test]
    async fn authenticated_allow_passes() {
        let executor = Executor::with_config(
            test_schema(),
            Arc::new(MockAdapter::new(mock_user_results())),
            RuntimeConfig::default().with_authorizer(Arc::new(AllowAll)),
        );
        let result = executor
            .execute_with_security("{ users { id name } }", None, &ctx("u1"))
            .await
            .unwrap();
        assert_eq!(result["data"]["users"][0]["name"], "Alice");
    }

    // ---- anonymous query path ---------------------------------------------

    #[tokio::test]
    async fn anonymous_deny_returns_403() {
        let executor = Executor::with_config(
            test_schema(),
            Arc::new(MockAdapter::new(mock_user_results())),
            RuntimeConfig::default().with_authorizer(Arc::new(DenyAll)),
        );
        let err = executor.execute("{ users { id name } }", None).await.unwrap_err();
        assert!(is_authz(&err), "anon deny must map to Authorization/403: {err:?}");
    }

    // The anon path passes `principal = None`; the authenticated path passes `Some`.
    #[tokio::test]
    async fn anonymous_principal_is_none_authenticated_is_some() {
        let executor = Executor::with_config(
            test_schema(),
            Arc::new(MockAdapter::new(mock_user_results())),
            RuntimeConfig::default().with_authorizer(Arc::new(RequireAuthenticated)),
        );
        // Anonymous → principal None → denied.
        assert!(executor.execute("{ users { id name } }", None).await.is_err());
        // Authenticated → principal Some → allowed.
        assert!(
            executor
                .execute_with_security("{ users { id name } }", None, &ctx("u1"))
                .await
                .is_ok()
        );
    }

    // ---- multi-root --------------------------------------------------------

    // Deny on ANY root denies the whole request, before any root is dispatched.
    #[tokio::test]
    async fn multi_root_denies_on_any_root() {
        let executor = Executor::with_config(
            test_schema(),
            Arc::new(MockAdapter::new(mock_user_results())),
            RuntimeConfig::default().with_authorizer(Arc::new(DenyNamed("secret"))),
        );
        let err = executor.execute("{ users { id name } secret { id } }", None).await.unwrap_err();
        match err {
            FraiseQLError::Authorization { resource, .. } => {
                assert_eq!(resource.as_deref(), Some("secret"), "denied root is the secret one");
            },
            other => panic!("expected Authorization, got {other:?}"),
        }
    }

    // ---- other operation kinds are gated (no bypass) -----------------------

    #[tokio::test]
    async fn introspection_is_gated() {
        let executor = Executor::with_config(
            test_schema(),
            Arc::new(MockAdapter::new(vec![])),
            RuntimeConfig::default().with_authorizer(Arc::new(DenyAll)),
        );
        assert!(executor.execute("{ __schema { queryType { name } } }", None).await.is_err());
    }

    #[tokio::test]
    async fn aggregate_is_gated() {
        let executor = Executor::with_config(
            test_schema(),
            Arc::new(MockAdapter::new(vec![])),
            RuntimeConfig::default().with_authorizer(Arc::new(DenyAll)),
        );
        // Classified as Aggregate by the `_aggregate` suffix; the gate runs before dispatch.
        assert!(executor.execute("{ users_aggregate { count } }", None).await.is_err());
    }

    #[tokio::test]
    async fn node_is_gated() {
        let executor = Executor::with_config(
            test_schema(),
            Arc::new(MockAdapter::new(vec![])),
            RuntimeConfig::default().with_authorizer(Arc::new(DenyAll)),
        );
        let res = executor
            .execute(r#"{ node(id: "VXNlcjoxMjM=") { ... on User { id } } }"#, None)
            .await;
        assert!(res.is_err(), "node lookup must route through the op authorizer");
    }

    // ---- requires_role AND-composition (enumeration-hiding preserved) ------

    // An allowing authorizer does NOT bypass `requires_role`: a principal lacking the
    // role still gets the enumeration-hiding "not found" Validation error, NOT 403.
    #[tokio::test]
    async fn allow_does_not_bypass_requires_role() {
        let executor = Executor::with_config(
            schema_requires_admin(),
            Arc::new(MockAdapter::new(mock_user_results())),
            RuntimeConfig::default().with_authorizer(Arc::new(AllowAll)),
        );
        let err = executor
            .execute_with_security("{ users { id name } }", None, &ctx("u1")) // no admin role
            .await
            .unwrap_err();
        assert!(
            matches!(err, FraiseQLError::Validation { .. }),
            "requires_role must stay enumeration-hiding (Validation 'not found'), got {err:?}"
        );
        assert!(!is_authz(&err), "must NOT regress to Authorization (would leak existence)");
    }

    // The authorizer runs FIRST: a Deny wins even for a principal that holds the role.
    #[tokio::test]
    async fn deny_wins_over_satisfied_requires_role() {
        let executor = Executor::with_config(
            schema_requires_admin(),
            Arc::new(MockAdapter::new(mock_user_results())),
            RuntimeConfig::default().with_authorizer(Arc::new(DenyAll)),
        );
        let err = executor
            .execute_with_security("{ users { id name } }", None, &admin_ctx("admin-1"))
            .await
            .unwrap_err();
        assert!(is_authz(&err), "authorizer deny must win (403), got {err:?}");
    }

    // ---- response cache safety --------------------------------------------

    // The gate runs before the response cache: a warm cache from an earlier Allow does
    // NOT let a later Deny through.
    #[tokio::test]
    async fn deny_not_bypassed_by_warm_response_cache() {
        let cache = Arc::new(ResponseCache::new(ResponseCacheConfig {
            enabled:     true,
            max_entries: 100,
            ttl_seconds: 3600,
        }));
        let executor = Executor::with_config(
            test_schema(),
            Arc::new(MockAdapter::new(mock_user_results())),
            RuntimeConfig::default().with_authorizer(Arc::new(Flip {
                calls: AtomicUsize::new(0),
            })),
        )
        .with_response_cache(cache);

        let q = "{ users { id name } }";
        // First call → Allow → executes and warms the cache.
        assert!(executor.execute_with_security(q, None, &ctx("u1")).await.is_ok());
        // Second call → Deny → 403, even though the cache holds the first response.
        let err = executor.execute_with_security(q, None, &ctx("u1")).await.unwrap_err();
        assert!(is_authz(&err), "a warm cache must not replay an Allow past a later Deny");
    }

    // ---- no authorizer configured → zero-cost no-op ------------------------

    // With no authorizer configured the gate is a no-op: a `PanicIfCalled` authorizer
    // would fire if the gate ran, so its ABSENCE from the config means the query runs
    // normally. (That a configured authorizer IS consulted is proven by every deny test
    // above.)
    #[tokio::test]
    async fn no_authorizer_is_never_consulted() {
        let executor = Executor::with_config(
            test_schema(),
            Arc::new(MockAdapter::new(mock_user_results())),
            RuntimeConfig::default(),
        );
        assert!(executor.execute("{ users { id name } }", None).await.is_ok());
    }

    // A configured authorizer that panics is reached on execution (proves the gate
    // actually consults the configured authorizer rather than skipping it).
    #[tokio::test]
    #[should_panic(expected = "authorizer must not be consulted")]
    async fn configured_authorizer_is_consulted() {
        let executor = Executor::with_config(
            test_schema(),
            Arc::new(MockAdapter::new(mock_user_results())),
            RuntimeConfig::default().with_authorizer(Arc::new(PanicIfCalled)),
        );
        let _ = executor.execute("{ users { id name } }", None).await;
    }

    // ---- mutation path: gated at the universal chokepoint ------------------

    // Authenticated GraphQL mutation → execute_mutation_impl gate.
    #[tokio::test]
    async fn mutation_via_execute_with_security_deny() {
        let executor = Executor::with_config(
            schema_with_mutation(),
            Arc::new(MockAdapter::new(vec![])),
            RuntimeConfig::default().with_authorizer(Arc::new(DenyAll)),
        );
        let err = executor
            .execute_with_security("mutation { createUser { id } }", None, &ctx("u1"))
            .await
            .unwrap_err();
        assert!(is_authz(&err), "authenticated mutation deny → 403: {err:?}");
    }

    // Anonymous GraphQL mutation → execute_mutation_query → execute_mutation_impl gate.
    #[tokio::test]
    async fn mutation_via_anonymous_graphql_deny() {
        let executor = Executor::with_config(
            schema_with_mutation(),
            Arc::new(MockAdapter::new(vec![])),
            RuntimeConfig::default().with_authorizer(Arc::new(DenyAll)),
        );
        let err = executor.execute("mutation { createUser { id } }", None).await.unwrap_err();
        assert!(is_authz(&err), "anonymous GraphQL mutation deny → 403: {err:?}");
    }

    // LOAD-BEARING (delta 1): the direct `execute_mutation` API — used by the anonymous
    // REST write path — bypasses both `*_internal` chokepoints, so it MUST be gated at
    // `execute_mutation_impl`. A deny here proves the anon-REST-write bypass is closed.
    #[tokio::test]
    async fn mutation_via_direct_api_deny_closes_anon_rest_bypass() {
        let executor = Executor::with_config(
            schema_with_mutation(),
            Arc::new(MockAdapter::new(vec![])),
            RuntimeConfig::default().with_authorizer(Arc::new(DenyAll)),
        );
        let err = executor.execute_mutation("createUser", None, &[]).await.unwrap_err();
        assert!(
            is_authz(&err),
            "direct execute_mutation deny → 403 (anon-REST bypass closed): {err:?}"
        );
    }

    // A raising authorizer fails the mutation closed.
    #[tokio::test]
    async fn mutation_raise_fails_closed() {
        let executor = Executor::with_config(
            schema_with_mutation(),
            Arc::new(MockAdapter::new(vec![])),
            RuntimeConfig::default().with_authorizer(Arc::new(Raising)),
        );
        let err = executor.execute_mutation("createUser", None, &[]).await.unwrap_err();
        assert!(is_authz(&err), "raising authorizer must fail the mutation closed: {err:?}");
    }

    // ALLOW lets the mutation past the gate (it then proceeds to execution — the empty
    // mock response fails downstream, but NOT with an Authorization error).
    #[tokio::test]
    async fn mutation_allow_passes_the_gate() {
        let executor = Executor::with_config(
            schema_with_mutation(),
            Arc::new(MockAdapter::new(vec![])),
            RuntimeConfig::default().with_authorizer(Arc::new(AllowAll)),
        );
        let result = executor.execute_mutation("createUser", None, &[]).await;
        // The gate allowed it; any resulting error is downstream, not Authorization.
        if let Err(err) = result {
            assert!(!is_authz(&err), "AllowAll must not block at the authz gate: {err:?}");
        }
    }

    // An unknown mutation name keeps its "not found" Validation (enumeration-hiding):
    // the gate runs AFTER find_mutation.
    #[tokio::test]
    async fn unknown_mutation_keeps_not_found_with_authorizer() {
        let executor = Executor::with_config(
            schema_with_mutation(),
            Arc::new(MockAdapter::new(vec![])),
            RuntimeConfig::default().with_authorizer(Arc::new(DenyAll)),
        );
        let err = executor.execute_mutation("doesNotExist", None, &[]).await.unwrap_err();
        assert!(
            matches!(err, FraiseQLError::Validation { .. }),
            "unknown mutation must stay 'not found' (Validation), got {err:?}"
        );
    }

    // ---- REST direct-read path (execute_query_direct / count_rows) --------
    // These runner methods bypass both `*_internal` chokepoints; they carry the gate
    // themselves (delta 2) so REST GET / count / streaming / embedding are covered.

    fn users_match() -> crate::runtime::matcher::QueryMatch {
        crate::runtime::QueryMatcher::new(test_schema())
            .match_query("{ users { id name } }", None)
            .unwrap()
    }

    #[tokio::test]
    async fn rest_direct_read_deny_returns_403() {
        let executor = Executor::with_config(
            test_schema(),
            Arc::new(MockAdapter::new(mock_user_results())),
            RuntimeConfig::default().with_authorizer(Arc::new(DenyAll)),
        );
        let qm = users_match();
        // Authenticated REST read.
        let err = executor.execute_query_direct(&qm, None, Some(&ctx("u1"))).await.unwrap_err();
        assert!(is_authz(&err), "REST direct read deny → 403: {err:?}");
        // Anonymous REST read (principal None) is gated too.
        let err = executor.execute_query_direct(&qm, None, None).await.unwrap_err();
        assert!(is_authz(&err), "anonymous REST direct read deny → 403: {err:?}");
    }

    #[tokio::test]
    async fn rest_count_rows_deny_returns_403() {
        let executor = Executor::with_config(
            test_schema(),
            Arc::new(MockAdapter::new(mock_user_results())),
            RuntimeConfig::default().with_authorizer(Arc::new(DenyAll)),
        );
        let qm = users_match();
        let err = executor.count_rows(&qm, None, Some(&ctx("u1"))).await.unwrap_err();
        assert!(is_authz(&err), "REST count_rows deny → 403: {err:?}");
    }

    #[tokio::test]
    async fn rest_direct_read_allow_passes() {
        let executor = Executor::with_config(
            test_schema(),
            Arc::new(MockAdapter::new(mock_user_results())),
            RuntimeConfig::default().with_authorizer(Arc::new(AllowAll)),
        );
        let qm = users_match();
        let result = executor.execute_query_direct(&qm, None, Some(&ctx("u1"))).await.unwrap();
        assert!(result["users"].is_array() || result.get("data").is_some(), "allowed: {result}");
    }

    // NOTE: the aggregate/window *embedder* entries (`Executor::execute_aggregate_query`
    // / `execute_window_query` in core.rs) carry an identical gate. They have no server
    // route and require a full `FactTableMetadata` fixture to invoke; the GraphQL
    // aggregate path (which routes through the chokepoint) is covered by
    // `aggregate_is_gated` above, and the embedder gate is structurally identical.
}

//! Tests for `routes/api/` modules.
#![allow(unused_imports)]

mod admin_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(deprecated)] // Reason: testing deprecated items

    use std::collections::HashMap;

    use super::super::admin::*;

    // ── CacheStatus (Issue #183) ──────────────────────────────────────────────

    #[test]
    #[allow(deprecated)] // Reason: testing deprecated variant
    fn cache_status_serializes_to_snake_case() {
        let json = serde_json::to_string(&CacheStatus::RlsGuardOnly).unwrap();
        assert_eq!(json, "\"rls_guard_only\"");

        let json = serde_json::to_string(&CacheStatus::Disabled).unwrap();
        assert_eq!(json, "\"disabled\"");

        let json = serde_json::to_string(&CacheStatus::Active).unwrap();
        assert_eq!(json, "\"active\"");
    }

    #[test]
    #[allow(deprecated)] // Reason: testing deprecated function
    fn cache_status_from_config_enabled() {
        assert_eq!(CacheStatus::from_cache_enabled(true), CacheStatus::RlsGuardOnly);
    }

    #[test]
    #[allow(deprecated)] // Reason: testing deprecated function
    fn cache_status_from_config_disabled() {
        assert_eq!(CacheStatus::from_cache_enabled(false), CacheStatus::Disabled);
    }

    #[test]
    #[allow(deprecated)] // Reason: testing deprecated variant
    fn cache_status_deserializes_from_snake_case() {
        let status: CacheStatus = serde_json::from_str("\"rls_guard_only\"").unwrap();
        assert_eq!(status, CacheStatus::RlsGuardOnly);

        let status: CacheStatus = serde_json::from_str("\"active\"").unwrap();
        assert_eq!(status, CacheStatus::Active);
    }

    // ── Grafana & other tests ───────────────────────────────────────────────

    #[test]
    fn test_grafana_dashboard_is_valid_json() {
        let parsed: serde_json::Value =
            serde_json::from_str(include_str!("../../../resources/fraiseql-dashboard.json"))
                .expect("fraiseql-dashboard.json must be valid JSON");

        assert_eq!(parsed["title"], "FraiseQL Performance");
        assert_eq!(parsed["uid"], "fraiseql-perf-v1");
        assert!(
            parsed["panels"].as_array().map_or(0, |p| p.len()) >= 10,
            "dashboard should have at least 10 panels"
        );
    }

    #[test]
    fn test_reload_schema_request_empty_path() {
        let request = ReloadSchemaRequest {
            schema_path:   String::new(),
            validate_only: false,
        };

        assert!(request.schema_path.is_empty());
    }

    #[test]
    fn test_reload_schema_request_with_path() {
        let request = ReloadSchemaRequest {
            schema_path:   "/path/to/schema.json".to_string(),
            validate_only: false,
        };

        assert!(!request.schema_path.is_empty());
    }

    #[test]
    fn test_cache_clear_scope_validation() {
        let valid_scopes = vec!["all", "entity", "pattern"];

        for scope in valid_scopes {
            let request = CacheClearRequest {
                scope:       scope.to_string(),
                entity_type: None,
                pattern:     None,
            };
            assert_eq!(request.scope, scope);
        }
    }

    #[test]
    fn test_admin_config_response_has_version() {
        let response = AdminConfigResponse {
            version: "2.0.0-a1".to_string(),
            config:  HashMap::new(),
        };

        assert!(!response.version.is_empty());
    }

    #[test]
    fn test_reload_schema_response_success() {
        let response = ReloadSchemaResponse {
            success: true,
            message: "Reloaded".to_string(),
        };

        assert!(response.success);
    }

    #[test]
    fn test_reload_schema_response_failure() {
        let response = ReloadSchemaResponse {
            success: false,
            message: "Failed to load".to_string(),
        };

        assert!(!response.success);
    }

    #[test]
    fn test_cache_clear_response_counts_entries() {
        let response = CacheClearResponse {
            success:         true,
            entries_cleared: 42,
            message:         "Cleared".to_string(),
        };

        assert_eq!(response.entries_cleared, 42);
    }

    #[test]
    fn test_cache_clear_request_entity_required_for_entity_scope() {
        let request = CacheClearRequest {
            scope:       "entity".to_string(),
            entity_type: Some("User".to_string()),
            pattern:     None,
        };

        assert_eq!(request.scope, "entity");
        assert_eq!(request.entity_type.as_deref(), Some("User"));
    }

    #[test]
    fn test_cache_clear_request_pattern_required_for_pattern_scope() {
        let request = CacheClearRequest {
            scope:       "pattern".to_string(),
            entity_type: None,
            pattern:     Some("*_user".to_string()),
        };

        assert_eq!(request.scope, "pattern");
        assert_eq!(request.pattern.as_deref(), Some("*_user"));
    }

    #[test]
    fn test_admin_config_response_sanitization_excludes_paths() {
        let response = AdminConfigResponse {
            version: "2.0.0".to_string(),
            config:  {
                let mut m = HashMap::new();
                m.insert("port".to_string(), "8000".to_string());
                m.insert("host".to_string(), "0.0.0.0".to_string());
                m.insert("tls_enabled".to_string(), "true".to_string());
                m
            },
        };

        assert_eq!(response.config.get("port"), Some(&"8000".to_string()));
        assert_eq!(response.config.get("host"), Some(&"0.0.0.0".to_string()));
        assert_eq!(response.config.get("tls_enabled"), Some(&"true".to_string()));
        assert!(!response.config.contains_key("cert_file"));
        assert!(!response.config.contains_key("key_file"));
    }

    #[test]
    fn test_admin_config_response_includes_limits() {
        let response = AdminConfigResponse {
            version: "2.0.0".to_string(),
            config:  {
                let mut m = HashMap::new();
                m.insert("max_request_size".to_string(), "10MB".to_string());
                m.insert("request_timeout".to_string(), "30s".to_string());
                m.insert("max_concurrent_requests".to_string(), "1000".to_string());
                m
            },
        };

        assert!(response.config.contains_key("max_request_size"));
        assert!(response.config.contains_key("request_timeout"));
        assert!(response.config.contains_key("max_concurrent_requests"));
    }

    #[test]
    fn test_cache_stats_response_structure() {
        let response = CacheStatsResponse {
            entries_count: 100,
            cache_enabled: true,
            ttl_secs:      60,
            message:       "Cache statistics".to_string(),
        };

        assert_eq!(response.entries_count, 100);
        assert!(response.cache_enabled);
        assert_eq!(response.ttl_secs, 60);
        assert!(!response.message.is_empty());
    }

    #[test]
    fn test_reload_schema_request_validates_path() {
        let request = ReloadSchemaRequest {
            schema_path:   "/path/to/schema.json".to_string(),
            validate_only: false,
        };

        assert!(!request.schema_path.is_empty());
    }

    #[test]
    fn test_reload_schema_request_validate_only_flag() {
        let request = ReloadSchemaRequest {
            schema_path:   "/path/to/schema.json".to_string(),
            validate_only: true,
        };

        assert!(request.validate_only);
    }

    #[test]
    fn test_reload_schema_response_indicates_success() {
        let response = ReloadSchemaResponse {
            success: true,
            message: "Schema reloaded".to_string(),
        };

        assert!(response.success);
        assert!(!response.message.is_empty());
    }

    // ── S33: reload_schema path traversal guards ───────────────────────────

    #[test]
    fn reload_schema_rejects_path_traversal() {
        assert!(
            validate_schema_path("../../etc/passwd", None).is_err(),
            "../../etc/passwd must be rejected"
        );
        assert!(
            validate_schema_path("schemas/../../../etc/shadow", None).is_err(),
            "embedded .. must be rejected"
        );
        assert!(validate_schema_path("..", None).is_err(), "bare .. must be rejected");
    }

    #[test]
    fn reload_schema_rejects_absolute_outside_base() {
        let base = std::path::Path::new("/var/fraiseql");
        assert!(
            validate_schema_path("/etc/passwd", Some(base)).is_err(),
            "/etc/passwd must be rejected when base is /var/fraiseql"
        );
        assert!(
            validate_schema_path("/var/fraiseql/../../etc/passwd", Some(base)).is_err(),
            "traversal through base must be rejected"
        );
    }

    #[test]
    fn reload_schema_accepts_safe_relative_path() {
        assert!(
            validate_schema_path("schema.compiled.json", None).is_ok(),
            "simple relative path must be accepted"
        );
        assert!(
            validate_schema_path("schemas/schema.compiled.json", None).is_ok(),
            "nested relative path must be accepted"
        );
    }

    #[test]
    fn reload_schema_accepts_path_within_base() {
        let base = std::path::Path::new("/var/fraiseql");
        assert!(
            validate_schema_path("schema.compiled.json", Some(base)).is_ok(),
            "relative path within base must be accepted"
        );
        assert!(
            validate_schema_path("/var/fraiseql/schema.compiled.json", Some(base)).is_ok(),
            "absolute path within base must be accepted"
        );
    }

    #[test]
    fn test_reload_schema_request_carries_audit_fields() {
        let req = ReloadSchemaRequest {
            schema_path:   "/var/run/fraiseql/schema.compiled.json".to_string(),
            validate_only: false,
        };
        assert!(!req.schema_path.is_empty(), "schema_path must be present for audit log");
        let _ = req.validate_only;
    }

    #[test]
    fn test_cache_clear_request_carries_audit_fields() {
        let all_req = CacheClearRequest {
            scope:       "all".to_string(),
            entity_type: None,
            pattern:     None,
        };
        assert_eq!(all_req.scope, "all");

        let entity_req = CacheClearRequest {
            scope:       "entity".to_string(),
            entity_type: Some("Order".to_string()),
            pattern:     None,
        };
        assert!(
            entity_req.entity_type.is_some(),
            "entity scope must carry entity_type for audit"
        );

        let pattern_req = CacheClearRequest {
            scope:       "pattern".to_string(),
            entity_type: None,
            pattern:     Some("v_order*".to_string()),
        };
        assert!(pattern_req.pattern.is_some(), "pattern scope must carry pattern for audit");
    }
}

mod design_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    use super::super::design::SeverityCountResponse;

    #[test]
    fn test_severity_count_response() {
        let resp = SeverityCountResponse {
            critical: 1,
            warning:  3,
            info:     5,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"critical\":1"));
    }
}

#[cfg(feature = "federation")]
mod federation_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

    use super::super::federation::*;

    #[test]
    fn test_default_format() {
        assert_eq!(default_format(), "json");
    }

    #[test]
    fn test_subgraph_info_creation() {
        let info = SubgraphInfo {
            name:     "test".to_string(),
            url:      "http://test.local".to_string(),
            entities: vec!["Entity1".to_string()],
            healthy:  true,
        };

        assert_eq!(info.name, "test");
        assert!(info.healthy);
    }

    #[test]
    fn test_subgraphs_response_creation() {
        let response = SubgraphsResponse { subgraphs: vec![] };

        assert!(response.subgraphs.is_empty());
    }

    #[test]
    fn test_graph_response_creation() {
        let response = GraphResponse {
            format:  "json".to_string(),
            content: "{}".to_string(),
        };

        assert_eq!(response.format, "json");
    }

    #[test]
    fn test_generate_json_graph_no_federation() {
        let json = generate_json_graph(None);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(parsed["subgraphs"].as_array().unwrap().is_empty());
        assert!(parsed["edges"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_generate_dot_graph_no_federation() {
        let dot = generate_dot_graph(None);
        assert!(dot.contains("digraph"));
        assert!(dot.contains("rankdir"));
    }

    #[test]
    fn test_generate_mermaid_graph_no_federation() {
        let mermaid = generate_mermaid_graph(None);
        assert!(mermaid.contains("graph LR"));
    }

    #[test]
    fn test_plan_response_not_cached() {
        let response = PlanResponse {
            cached: false,
            schema_fingerprint: "abc123".to_string(),
            #[cfg(feature = "federation")]
            fetches: None,
            #[cfg(not(feature = "federation"))]
            fetches: None,
        };

        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["cached"], false);
        assert!(json["fetches"].is_null());
    }

    #[cfg(feature = "federation")]
    #[test]
    fn test_plan_response_with_fetches() {
        use fraiseql_core::federation::SubgraphFetch;

        let response = PlanResponse {
            cached:             true,
            schema_fingerprint: "fp123".to_string(),
            fetches:            Some(vec![SubgraphFetch {
                subgraph:     "users".to_string(),
                query:        "{ user(id: $id) { name } }".to_string(),
                entity_types: vec!["User".to_string()],
                depends_on:   None,
            }]),
        };

        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["cached"], true);
        let fetches = json["fetches"].as_array().unwrap();
        assert_eq!(fetches.len(), 1);
        assert_eq!(fetches[0]["subgraph"], "users");
        assert_eq!(fetches[0]["query"], "{ user(id: $id) { name } }");
    }

    #[test]
    fn test_plan_query_deserialization() {
        let json = r#"{"query": "{ users { id name } }"}"#;
        let query: PlanQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.query, "{ users { id name } }");
    }
}

mod metadata_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable assertions

    use std::sync::Arc;

    use async_trait::async_trait;
    use axum::extract::State;
    use fraiseql_core::{
        db::{
            WhereClause,
            traits::DatabaseAdapter,
            types::{DatabaseType, JsonbValue, PoolMetrics},
        },
        error::Result as FraiseQLResult,
        runtime::Executor,
        schema::{
            CompiledSchema, FieldDefinition, FieldDenyPolicy, FieldEncryptionConfig, FieldType,
            TypeDefinition,
        },
    };

    use super::super::metadata::{FieldSecurityMetadata, flatten_field_metadata, metadata_handler};
    use crate::routes::graphql::AppState;

    #[derive(Debug, Clone)]
    struct StubAdapter;

    // Reason: async_trait is required by the DatabaseAdapter trait definition
    #[async_trait]
    impl DatabaseAdapter for StubAdapter {
        async fn execute_where_query(
            &self,
            _view: &str,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[fraiseql_core::db::types::OrderByClause]>,
        ) -> FraiseQLResult<Vec<JsonbValue>> {
            Ok(vec![])
        }

        async fn execute_with_projection(
            &self,
            _view: &str,
            _projection: Option<&fraiseql_core::schema::SqlProjectionHint>,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[fraiseql_core::db::types::OrderByClause]>,
        ) -> FraiseQLResult<Vec<JsonbValue>> {
            Ok(vec![])
        }

        fn database_type(&self) -> DatabaseType {
            DatabaseType::SQLite
        }

        async fn health_check(&self) -> FraiseQLResult<()> {
            Ok(())
        }

        fn pool_metrics(&self) -> PoolMetrics {
            PoolMetrics::default()
        }

        async fn execute_raw_query(
            &self,
            _sql: &str,
        ) -> FraiseQLResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }

        async fn execute_parameterized_aggregate(
            &self,
            _sql: &str,
            _params: &[serde_json::Value],
        ) -> FraiseQLResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }
    }

    fn make_security_schema() -> CompiledSchema {
        let email_field = FieldDefinition::new("email", FieldType::String).with_encryption(
            FieldEncryptionConfig {
                key_reference: "keys/email".to_string(),
                algorithm:     "AES-256-GCM".to_string(),
            },
        );
        let ssn_field = FieldDefinition::new("ssn", FieldType::String)
            .with_requires_scope("read:pii")
            .with_on_deny(FieldDenyPolicy::Mask);
        let mut user_type = TypeDefinition::new("User", "v_user");
        user_type.fields = vec![email_field, ssn_field];
        CompiledSchema {
            types: vec![user_type],
            ..CompiledSchema::default()
        }
    }

    #[tokio::test]
    async fn metadata_handler_returns_200_with_correct_body() {
        let schema = make_security_schema();
        let executor = Arc::new(Executor::new(schema, Arc::new(StubAdapter)));
        let state = AppState::new(executor);

        let axum::Json(resp) = metadata_handler(State(state)).await;

        assert_eq!(resp.status, "success");

        let meta = &resp.data.metadata;
        assert_eq!(meta.len(), 2);

        let email = meta.get("User.email").unwrap();
        assert_eq!(email.encrypted, Some(true));
        assert!(email.requires_scope.is_none());
        assert!(email.on_deny.is_none());

        let ssn = meta.get("User.ssn").unwrap();
        assert_eq!(ssn.requires_scope.as_deref(), Some("read:pii"));
        assert_eq!(ssn.on_deny.as_deref(), Some("mask"));
        assert!(ssn.encrypted.is_none());
    }

    #[tokio::test]
    async fn metadata_handler_body_serialises_to_expected_json() {
        let schema = make_security_schema();
        let executor = Arc::new(Executor::new(schema, Arc::new(StubAdapter)));
        let state = AppState::new(executor);

        let axum::Json(resp) = metadata_handler(State(state)).await;
        let json = serde_json::to_value(&resp).unwrap();

        assert_eq!(
            json,
            serde_json::json!({
                "status": "success",
                "data": {
                    "metadata": {
                        "User.email": {"encrypted": true},
                        "User.ssn":   {"requires_scope": "read:pii", "on_deny": "mask"}
                    }
                }
            })
        );
    }

    fn make_schema() -> CompiledSchema {
        let email_field = FieldDefinition::new("email", FieldType::String).with_encryption(
            FieldEncryptionConfig {
                key_reference: "keys/email".to_string(),
                algorithm:     "AES-256-GCM".to_string(),
            },
        );

        let ssn_field = FieldDefinition::new("ssn", FieldType::String)
            .with_requires_scope("read:pii")
            .with_on_deny(FieldDenyPolicy::Mask);

        let id_field = FieldDefinition::new("id", FieldType::String);

        let mut user_type = TypeDefinition::new("User", "v_user");
        user_type.fields = vec![email_field, ssn_field, id_field];

        let mut admin_type = TypeDefinition::new("AdminDashboard", "v_admin_dashboard");
        admin_type.requires_role = Some("admin".to_string());

        let mut plain_type = TypeDefinition::new("PlainType", "v_plain");
        plain_type.fields = vec![FieldDefinition::new("name", FieldType::String)];

        CompiledSchema {
            types: vec![user_type, admin_type, plain_type],
            ..CompiledSchema::default()
        }
    }

    #[test]
    fn encrypted_field_is_present() {
        let map = flatten_field_metadata(&make_schema());
        let entry = map.get("User.email").unwrap();
        assert_eq!(entry.encrypted, Some(true));
        assert!(entry.requires_scope.is_none());
        assert!(entry.on_deny.is_none());
        assert!(entry.requires_role.is_none());
    }

    #[test]
    fn scoped_field_with_mask_is_present() {
        let map = flatten_field_metadata(&make_schema());
        let entry = map.get("User.ssn").unwrap();
        assert_eq!(entry.requires_scope.as_deref(), Some("read:pii"));
        assert_eq!(entry.on_deny.as_deref(), Some("mask"));
        assert!(entry.encrypted.is_none());
        assert!(entry.requires_role.is_none());
    }

    #[test]
    fn type_level_requires_role_is_present() {
        let map = flatten_field_metadata(&make_schema());
        let entry = map.get("AdminDashboard").unwrap();
        assert_eq!(entry.requires_role.as_deref(), Some("admin"));
        assert!(entry.encrypted.is_none());
        assert!(entry.requires_scope.is_none());
        assert!(entry.on_deny.is_none());
    }

    #[test]
    fn default_annotated_field_is_omitted() {
        let map = flatten_field_metadata(&make_schema());
        assert!(!map.contains_key("User.id"), "User.id has no annotations");
    }

    #[test]
    fn plain_type_fields_are_omitted() {
        let map = flatten_field_metadata(&make_schema());
        assert!(!map.contains_key("PlainType.name"));
        assert!(!map.contains_key("PlainType"));
    }

    #[test]
    fn exact_entry_count_is_three() {
        let map = flatten_field_metadata(&make_schema());
        assert_eq!(map.len(), 3, "unexpected keys: {map:?}");
    }

    #[test]
    fn empty_schema_produces_empty_map() {
        let map = flatten_field_metadata(&CompiledSchema::default());
        assert!(map.is_empty());
    }

    #[test]
    fn email_serialises_to_encrypted_only() {
        let map = flatten_field_metadata(&make_schema());
        let json = serde_json::to_value(map.get("User.email").unwrap()).unwrap();
        assert_eq!(json, serde_json::json!({"encrypted": true}));
    }

    #[test]
    fn ssn_serialises_to_scope_and_deny() {
        let map = flatten_field_metadata(&make_schema());
        let json = serde_json::to_value(map.get("User.ssn").unwrap()).unwrap();
        assert_eq!(json, serde_json::json!({"requires_scope": "read:pii", "on_deny": "mask"}));
    }

    #[test]
    fn admin_dashboard_serialises_to_role_only() {
        let map = flatten_field_metadata(&make_schema());
        let json = serde_json::to_value(map.get("AdminDashboard").unwrap()).unwrap();
        assert_eq!(json, serde_json::json!({"requires_role": "admin"}));
    }

    #[test]
    fn reject_on_deny_does_not_appear_in_output() {
        let mut field =
            FieldDefinition::new("salary", FieldType::String).with_requires_scope("read:payroll");
        field.on_deny = FieldDenyPolicy::Reject;

        let mut type_def = TypeDefinition::new("Employee", "v_employee");
        type_def.fields = vec![field];

        let schema = CompiledSchema {
            types: vec![type_def],
            ..CompiledSchema::default()
        };

        let map = flatten_field_metadata(&schema);
        let entry = map.get("Employee.salary").unwrap();
        assert!(entry.on_deny.is_none(), "Reject is the default — must not appear");
        let json = serde_json::to_value(entry).unwrap();
        assert!(!json.as_object().unwrap().contains_key("on_deny"));
    }

    #[test]
    fn is_empty_true_when_all_none() {
        let meta = FieldSecurityMetadata {
            encrypted:      None,
            requires_scope: None,
            on_deny:        None,
            requires_role:  None,
        };
        assert!(meta.is_empty());
    }

    #[test]
    fn is_empty_false_when_any_some() {
        let meta = FieldSecurityMetadata {
            encrypted:      Some(true),
            requires_scope: None,
            on_deny:        None,
            requires_role:  None,
        };
        assert!(!meta.is_empty());
    }
}

mod openapi_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

    use super::super::openapi::get_openapi_spec;

    #[test]
    fn test_openapi_spec_parses_as_json() {
        let spec = get_openapi_spec();
        let parsed: serde_json::Value = serde_json::from_str(&spec)
            .expect("OpenAPI spec is valid JSON: generated by crate at compile-time");
        assert!(parsed.is_object());
    }

    #[test]
    fn test_openapi_spec_has_all_required_fields() {
        let spec = get_openapi_spec();
        let parsed: serde_json::Value = serde_json::from_str(&spec).unwrap();

        assert!(parsed.get("openapi").is_some(), "OpenAPI spec must contain 'openapi' key");
        assert!(parsed.get("info").is_some(), "OpenAPI spec must contain 'info' key");
        assert!(parsed.get("paths").is_some(), "OpenAPI spec must contain 'paths' key");
        assert!(parsed.get("components").is_some(), "OpenAPI spec must contain 'components' key");
    }

    #[test]
    fn test_openapi_spec_version() {
        let spec = get_openapi_spec();
        let parsed: serde_json::Value = serde_json::from_str(&spec).unwrap();

        assert_eq!(parsed["openapi"].as_str(), Some("3.0.0"), "Should be OpenAPI 3.0.0");
    }

    #[test]
    fn test_openapi_spec_documents_10_endpoints() {
        let spec = get_openapi_spec();
        let parsed: serde_json::Value = serde_json::from_str(&spec).unwrap();

        let paths = &parsed["paths"];
        let count = paths.as_object().map_or(0, |m| m.len());

        assert_eq!(count, 10, "Should document all 10 API endpoint paths");
    }

    #[test]
    fn test_openapi_has_security_schemes() {
        let spec = get_openapi_spec();
        let parsed: serde_json::Value = serde_json::from_str(&spec).unwrap();

        let schemes = &parsed["components"]["securitySchemes"];
        assert!(
            schemes.get("BearerAuth").is_some(),
            "security schemes must include 'BearerAuth'"
        );
    }

    #[test]
    fn test_openapi_has_component_schemas() {
        let spec = get_openapi_spec();
        let parsed: serde_json::Value = serde_json::from_str(&spec).unwrap();

        let schemas = &parsed["components"]["schemas"];
        assert!(
            schemas.get("ExplainRequest").is_some(),
            "component schemas must include 'ExplainRequest'"
        );
        assert!(
            schemas.get("ExplainResponse").is_some(),
            "component schemas must include 'ExplainResponse'"
        );
        assert!(
            schemas.get("ReloadSchemaRequest").is_some(),
            "component schemas must include 'ReloadSchemaRequest'"
        );
    }
}

mod query_tests {
    use super::super::query::*;

    #[test]
    fn test_generate_warnings_deep() {
        let complexity = ComplexityInfo {
            depth:       15,
            complexity:  10,
            alias_count: 0,
        };
        let warnings = generate_warnings(&complexity);
        assert!(!warnings.is_empty());
        assert!(warnings[0].contains("depth"));
    }

    #[test]
    fn test_generate_warnings_high_complexity() {
        let complexity = ComplexityInfo {
            depth:       3,
            complexity:  200,
            alias_count: 0,
        };
        let warnings = generate_warnings(&complexity);
        assert!(!warnings.is_empty());
        assert!(warnings.iter().any(|w| w.contains("complexity")));
    }

    #[test]
    fn test_generate_warnings_high_alias_count() {
        let complexity = ComplexityInfo {
            depth:       2,
            complexity:  5,
            alias_count: 35,
        };
        let warnings = generate_warnings(&complexity);
        assert!(warnings.iter().any(|w| w.contains("alias")));
    }

    #[test]
    fn test_estimate_cost() {
        let complexity = ComplexityInfo {
            depth:       2,
            complexity:  3,
            alias_count: 0,
        };
        let cost = estimate_cost(&complexity);
        assert!(cost > 0);
    }

    #[test]
    fn test_stats_response_structure() {
        let response = StatsResponse {
            total_queries:      100,
            successful_queries: 95,
            failed_queries:     5,
            average_latency_ms: 42.5,
        };
        assert_eq!(response.total_queries, 100);
        assert_eq!(response.successful_queries, 95);
        assert_eq!(response.failed_queries, 5);
        assert!(response.average_latency_ms > 0.0);
    }

    #[test]
    fn test_explain_response_structure() {
        let response = ExplainResponse {
            query:          "query { users { id } }".to_string(),
            sql:            Some("SELECT id FROM users".to_string()),
            complexity:     ComplexityInfo {
                depth:       2,
                complexity:  2,
                alias_count: 0,
            },
            warnings:       vec![],
            estimated_cost: 50,
            views_accessed: vec!["v_user".to_string()],
            query_type:     "regular".to_string(),
            database_plan:  None,
        };

        assert!(!response.query.is_empty());
        assert_eq!(response.sql.as_deref(), Some("SELECT id FROM users"));
        assert_eq!(response.complexity.depth, 2);
        assert_eq!(response.estimated_cost, 50);
    }

    #[test]
    fn test_validate_request_structure() {
        let request = ValidateRequest {
            query: "query { users { id } }".to_string(),
        };
        assert!(!request.query.is_empty());
    }

    #[test]
    fn test_explain_request_structure() {
        let request = ExplainRequest {
            query:     "query { users { id } }".to_string(),
            variables: None,
        };
        assert!(!request.query.is_empty());
    }

    #[test]
    fn test_debug_disabled_no_db_explain() {
        use fraiseql_core::schema::DebugConfig;

        assert!(!is_db_explain_enabled(None));

        let config = DebugConfig {
            enabled: true,
            database_explain: false,
            ..Default::default()
        };
        assert!(!is_db_explain_enabled(Some(&config)));
    }

    #[test]
    fn test_debug_enabled_db_explain() {
        use fraiseql_core::schema::DebugConfig;

        let config = DebugConfig {
            enabled: true,
            database_explain: true,
            ..Default::default()
        };
        assert!(is_db_explain_enabled(Some(&config)));
    }

    #[test]
    fn test_debug_master_switch_required() {
        use fraiseql_core::schema::DebugConfig;

        let config = DebugConfig {
            enabled: false,
            database_explain: true,
            ..Default::default()
        };
        assert!(!is_db_explain_enabled(Some(&config)));
    }
}

mod schema_tests {
    use super::super::schema::{GraphQLSchemaResponse, JsonSchemaResponse};

    #[test]
    fn test_graphql_response_creation() {
        let response = GraphQLSchemaResponse {
            schema: "type Query { hello: String }".to_string(),
        };

        assert_eq!(response.schema, "type Query { hello: String }");
    }

    #[test]
    fn test_json_response_creation() {
        let response = JsonSchemaResponse {
            schema: serde_json::json!({"types": []}),
        };

        assert!(response.schema.is_object());
    }
}

mod tenant_admin_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code

    use std::sync::Arc;

    use async_trait::async_trait;
    use fraiseql_core::{
        db::{
            WhereClause,
            traits::DatabaseAdapter,
            types::{DatabaseType, JsonbValue, PoolMetrics},
        },
        error::Result as FraiseQLResult,
        runtime::Executor,
        schema::CompiledSchema,
    };

    use crate::routes::graphql::{AppState, TenantExecutorRegistry};

    #[derive(Debug, Clone)]
    struct StubAdapter;

    #[async_trait]
    impl DatabaseAdapter for StubAdapter {
        async fn execute_where_query(
            &self,
            _view: &str,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[fraiseql_core::db::types::OrderByClause]>,
        ) -> FraiseQLResult<Vec<JsonbValue>> {
            Ok(vec![])
        }

        async fn execute_with_projection(
            &self,
            _view: &str,
            _projection: Option<&fraiseql_core::schema::SqlProjectionHint>,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[fraiseql_core::db::types::OrderByClause]>,
        ) -> FraiseQLResult<Vec<JsonbValue>> {
            Ok(vec![])
        }

        fn database_type(&self) -> DatabaseType {
            DatabaseType::SQLite
        }

        async fn health_check(&self) -> FraiseQLResult<()> {
            Ok(())
        }

        fn pool_metrics(&self) -> PoolMetrics {
            PoolMetrics::default()
        }

        async fn execute_raw_query(
            &self,
            _sql: &str,
        ) -> FraiseQLResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }

        async fn execute_parameterized_aggregate(
            &self,
            _sql: &str,
            _params: &[serde_json::Value],
        ) -> FraiseQLResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }
    }

    fn make_multitenant_state() -> AppState<StubAdapter> {
        let schema = CompiledSchema::default();
        let executor = Arc::new(Executor::new(schema, Arc::new(StubAdapter)));
        let state = AppState::new(executor);
        let registry = TenantExecutorRegistry::new(state.executor.clone());
        state.with_tenant_registry(Arc::new(registry))
    }

    fn make_single_tenant_state() -> AppState<StubAdapter> {
        let schema = CompiledSchema::default();
        let executor = Arc::new(Executor::new(schema, Arc::new(StubAdapter)));
        AppState::new(executor)
    }

    #[test]
    fn test_single_tenant_mode_has_no_registry() {
        let state = make_single_tenant_state();
        assert!(state.tenant_registry().is_none());
    }

    #[test]
    fn test_multi_tenant_empty_registry() {
        let state = make_multitenant_state();
        let registry = state.tenant_registry().unwrap();
        assert!(registry.is_empty());
        assert_eq!(registry.tenant_keys().len(), 0);
    }

    #[test]
    fn test_register_and_list_tenants() {
        let state = make_multitenant_state();
        let registry = state.tenant_registry().unwrap();

        let executor = Arc::new(Executor::new(CompiledSchema::default(), Arc::new(StubAdapter)));
        registry.upsert("tenant-abc", executor);

        assert_eq!(registry.len(), 1);
        assert_eq!(registry.tenant_keys(), vec!["tenant-abc"]);
    }

    #[test]
    fn test_upsert_existing_returns_false() {
        let state = make_multitenant_state();
        let registry = state.tenant_registry().unwrap();

        let executor = Arc::new(Executor::new(CompiledSchema::default(), Arc::new(StubAdapter)));
        assert!(registry.upsert("tenant-abc", executor));

        let executor2 = Arc::new(Executor::new(CompiledSchema::default(), Arc::new(StubAdapter)));
        assert!(!registry.upsert("tenant-abc", executor2));
    }

    #[test]
    fn test_delete_unknown_returns_error() {
        let state = make_multitenant_state();
        let registry = state.tenant_registry().unwrap();
        assert!(registry.remove("unknown").is_err());
    }

    #[test]
    fn test_get_tenant_metadata_via_registry() {
        let state = make_multitenant_state();
        let registry = state.tenant_registry().unwrap();

        let mut schema = CompiledSchema::default();
        schema
            .queries
            .push(fraiseql_core::schema::QueryDefinition::new("users", "User"));
        let executor = Arc::new(Executor::new(schema, Arc::new(StubAdapter)));
        registry.upsert("tenant-abc", executor);

        let exec = registry.executor_for(Some("tenant-abc")).unwrap();
        assert_eq!(exec.schema().queries.len(), 1);
        assert_eq!(exec.schema().mutations.len(), 0);
    }

    #[tokio::test]
    async fn test_health_check_registered_tenant() {
        let state = make_multitenant_state();
        let registry = state.tenant_registry().unwrap();

        let executor = Arc::new(Executor::new(CompiledSchema::default(), Arc::new(StubAdapter)));
        registry.upsert("tenant-abc", executor);

        assert!(registry.health_check("tenant-abc").await.is_ok());
    }

    #[tokio::test]
    async fn test_health_check_unknown_tenant() {
        let state = make_multitenant_state();
        let registry = state.tenant_registry().unwrap();

        assert!(registry.health_check("unknown").await.is_err());
    }

    #[test]
    fn test_domain_registry_register_and_list() {
        let state = make_multitenant_state();
        let registry = state.tenant_registry().unwrap();

        let executor = Arc::new(Executor::new(CompiledSchema::default(), Arc::new(StubAdapter)));
        registry.upsert("tenant-abc", executor);

        state.domain_registry().register("api.acme.com", "tenant-abc");

        let mappings = state.domain_registry().domains();
        assert_eq!(mappings.len(), 1);
        assert_eq!(mappings[0].0, "api.acme.com");
        assert_eq!(mappings[0].1, "tenant-abc");
    }

    #[test]
    fn test_domain_registry_remove() {
        let state = make_multitenant_state();

        state.domain_registry().register("api.acme.com", "tenant-abc");
        assert!(state.domain_registry().remove("api.acme.com"));
        assert!(!state.domain_registry().remove("api.acme.com"));
    }

    #[test]
    fn test_domain_registry_lookup_with_port() {
        let state = make_multitenant_state();
        state.domain_registry().register("api.acme.com", "tenant-abc");

        assert_eq!(
            state.domain_registry().lookup("api.acme.com:8080"),
            Some("tenant-abc".to_string())
        );
    }

    #[test]
    fn test_domain_empty_in_single_tenant_mode() {
        let state = make_single_tenant_state();
        assert!(state.domain_registry().is_empty());
    }
}

mod usage_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code

    use std::sync::Arc;

    use async_trait::async_trait;
    use axum::{
        Router,
        body::Body,
        http::{Method, Request, StatusCode},
        middleware,
        routing::get,
    };
    use fraiseql_core::{
        db::{
            WhereClause,
            traits::DatabaseAdapter,
            types::{DatabaseType, JsonbValue, PoolMetrics},
        },
        error::Result as FraiseQLResult,
        runtime::Executor,
        schema::CompiledSchema,
    };
    use tower::ServiceExt as _;

    use super::super::usage::usage_handler;
    use crate::{
        middleware::{BearerAuthState, bearer_auth_middleware},
        routes::graphql::AppState,
        usage::aggregator::UsageAggregator,
    };

    #[derive(Debug, Clone)]
    struct StubAdapter;

    #[async_trait]
    impl DatabaseAdapter for StubAdapter {
        async fn execute_where_query(
            &self,
            _view: &str,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[fraiseql_core::db::types::OrderByClause]>,
        ) -> FraiseQLResult<Vec<JsonbValue>> {
            Ok(vec![])
        }

        async fn execute_with_projection(
            &self,
            _view: &str,
            _projection: Option<&fraiseql_core::schema::SqlProjectionHint>,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[fraiseql_core::db::types::OrderByClause]>,
        ) -> FraiseQLResult<Vec<JsonbValue>> {
            Ok(vec![])
        }

        fn database_type(&self) -> DatabaseType {
            DatabaseType::SQLite
        }

        async fn health_check(&self) -> FraiseQLResult<()> {
            Ok(())
        }

        fn pool_metrics(&self) -> PoolMetrics {
            PoolMetrics::default()
        }

        async fn execute_raw_query(
            &self,
            _sql: &str,
        ) -> FraiseQLResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }

        async fn execute_parameterized_aggregate(
            &self,
            _sql: &str,
            _params: &[serde_json::Value],
        ) -> FraiseQLResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }
    }

    fn make_state_with_usage(usage: Arc<UsageAggregator>) -> AppState<StubAdapter> {
        let schema = CompiledSchema::default();
        let executor = Arc::new(Executor::new(schema, Arc::new(StubAdapter)));
        AppState::new(executor).with_usage(usage)
    }

    fn make_router(usage: Arc<UsageAggregator>) -> Router {
        let state = make_state_with_usage(usage);
        Router::new()
            .route("/api/v1/admin/usage", get(usage_handler::<StubAdapter>))
            .with_state(state)
    }

    fn make_authed_router(usage: Arc<UsageAggregator>) -> Router {
        let state = make_state_with_usage(usage);
        let auth_state = BearerAuthState::new("secret-token".to_string());
        Router::new()
            .route("/api/v1/admin/usage", get(usage_handler::<StubAdapter>))
            .route_layer(middleware::from_fn_with_state(auth_state, bearer_auth_middleware))
            .with_state(state)
    }

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn test_usage_invalid_period_returns_400() {
        let router = make_router(Arc::new(UsageAggregator::new()));

        for bad_period in &["2026", "26-04", "2026/04", "2026-13", "2026-00", ""] {
            let req = Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/admin/usage?tenant_id=acme&period={bad_period}"))
                .body(Body::empty())
                .unwrap();

            let resp = router.clone().oneshot(req).await.unwrap();
            assert_eq!(
                resp.status(),
                StatusCode::BAD_REQUEST,
                "expected 400 for period {bad_period:?}"
            );

            let json = body_json(resp).await;
            assert_eq!(json["error"], "invalid period format");
        }
    }

    #[tokio::test]
    async fn test_usage_happy_path_response_shape() {
        let usage = Arc::new(UsageAggregator::new());
        let event = |entity: &str| crate::usage::events::MutationAuditEvent {
            mutation_name: format!("create_{entity}"),
            entity_type:   entity.to_owned(),
            operation:     "create".to_owned(),
            tenant_id:     "acme".to_owned(),
            period:        "2026-05".to_owned(),
        };
        for _ in 0..3 {
            usage.record(&event("User"));
        }
        for _ in 0..2 {
            usage.record(&event("Order"));
        }

        let router = make_router(Arc::clone(&usage));

        let req = Request::builder()
            .method(Method::GET)
            .uri("/api/v1/admin/usage?tenant_id=acme&period=2026-05")
            .body(Body::empty())
            .unwrap();

        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let json = body_json(resp).await;
        assert_eq!(json["tenant_id"], "acme");
        assert_eq!(json["period"], "2026-05");
        assert_eq!(json["usage"]["mutations"]["User"], 3);
        assert_eq!(json["usage"]["mutations"]["Order"], 2);
    }

    #[tokio::test]
    async fn test_usage_unknown_tenant_returns_empty_mutations() {
        let usage = Arc::new(UsageAggregator::new());
        let router = make_router(Arc::clone(&usage));

        let req = Request::builder()
            .method(Method::GET)
            .uri("/api/v1/admin/usage?tenant_id=nobody&period=2026-05")
            .body(Body::empty())
            .unwrap();

        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let json = body_json(resp).await;
        assert_eq!(json["tenant_id"], "nobody");
        assert_eq!(json["period"], "2026-05");
        assert!(json["usage"]["mutations"].is_object());
        assert_eq!(json["usage"]["mutations"].as_object().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_usage_unknown_period_returns_empty_mutations() {
        let usage = Arc::new(UsageAggregator::new());
        usage.record(&crate::usage::events::MutationAuditEvent {
            mutation_name: "create_user".to_owned(),
            entity_type:   "User".to_owned(),
            operation:     "create".to_owned(),
            tenant_id:     "acme".to_owned(),
            period:        "2026-04".to_owned(),
        });

        let router = make_router(usage);

        let req = Request::builder()
            .method(Method::GET)
            .uri("/api/v1/admin/usage?tenant_id=acme&period=2026-05")
            .body(Body::empty())
            .unwrap();

        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let json = body_json(resp).await;
        assert!(json["usage"]["mutations"].as_object().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_usage_unauthenticated_returns_401() {
        let router = make_authed_router(Arc::new(UsageAggregator::new()));

        let req = Request::builder()
            .method(Method::GET)
            .uri("/api/v1/admin/usage?tenant_id=acme&period=2026-05")
            .body(Body::empty())
            .unwrap();

        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_usage_wrong_token_returns_403() {
        let router = make_authed_router(Arc::new(UsageAggregator::new()));

        let req = Request::builder()
            .method(Method::GET)
            .uri("/api/v1/admin/usage?tenant_id=acme&period=2026-05")
            .header("Authorization", "Bearer wrong-token")
            .body(Body::empty())
            .unwrap();

        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_usage_correct_token_returns_200() {
        let router = make_authed_router(Arc::new(UsageAggregator::new()));

        let req = Request::builder()
            .method(Method::GET)
            .uri("/api/v1/admin/usage?tenant_id=acme&period=2026-05")
            .header("Authorization", "Bearer secret-token")
            .body(Body::empty())
            .unwrap();

        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}

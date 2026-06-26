#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::{
    query::QueryDefinition,
    schema::{CompiledSchema, SubscribableEntity},
};
#[cfg(feature = "federation")]
use crate::schema::config_types::FederationEntity;
use crate::schema::{
    CURRENT_SCHEMA_FORMAT_VERSION, MutationDefinition,
    config_types::{FederationConfig, NamingConvention},
    graphql_type_defs::TypeDefinition,
    observer_types::{ObserverDefinition, RetryConfig},
    security_config::{RoleDefinition, SecurityConfig},
};

#[test]
fn test_compiled_schema_with_observers() {
    let json = r#"{
        "types": [],
        "enums": [],
        "input_types": [],
        "interfaces": [],
        "unions": [],
        "queries": [],
        "mutations": [],
        "subscriptions": [],
        "observers": [
            {
                "name": "onHighValueOrder",
                "entity": "Order",
                "event": "INSERT",
                "condition": "total > 1000",
                "actions": [
                    {
                        "type": "webhook",
                        "url": "https://api.example.com/webhook"
                    }
                ],
                "retry": {
                    "max_attempts": 3,
                    "backoff_strategy": "exponential",
                    "initial_delay_ms": 1000,
                    "max_delay_ms": 60000
                }
            }
        ]
    }"#;

    let schema = CompiledSchema::from_json(json, false).unwrap();

    assert!(schema.has_observers());
    assert_eq!(schema.observer_count(), 1);

    let observer = schema.find_observer("onHighValueOrder").unwrap();
    assert_eq!(observer.entity, "Order");
    assert_eq!(observer.event, "INSERT");
    assert_eq!(observer.condition, Some("total > 1000".to_string()));
    assert_eq!(observer.actions.len(), 1);
    assert_eq!(observer.retry.max_attempts, 3);
    assert!(observer.retry.is_exponential());
}

#[test]
fn test_compiled_schema_backward_compatible() {
    // Schema without observers field should still load
    let json = r#"{
        "types": [],
        "enums": [],
        "input_types": [],
        "interfaces": [],
        "unions": [],
        "queries": [],
        "mutations": [],
        "subscriptions": []
    }"#;

    let schema = CompiledSchema::from_json(json, false).unwrap();
    assert!(!schema.has_observers());
    assert_eq!(schema.observer_count(), 0);
}

#[test]
fn test_find_observers_for_entity() {
    let schema = CompiledSchema {
        observers: vec![
            ObserverDefinition::new("onOrderInsert", "Order", "INSERT"),
            ObserverDefinition::new("onOrderUpdate", "Order", "UPDATE"),
            ObserverDefinition::new("onUserInsert", "User", "INSERT"),
        ],
        ..Default::default()
    };

    let order_observers = schema.find_observers_for_entity("Order");
    assert_eq!(order_observers.len(), 2);

    let user_observers = schema.find_observers_for_entity("User");
    assert_eq!(user_observers.len(), 1);
}

#[test]
fn test_find_observers_for_event() {
    let schema = CompiledSchema {
        observers: vec![
            ObserverDefinition::new("onOrderInsert", "Order", "INSERT"),
            ObserverDefinition::new("onOrderUpdate", "Order", "UPDATE"),
            ObserverDefinition::new("onUserInsert", "User", "INSERT"),
        ],
        ..Default::default()
    };

    let insert_observers = schema.find_observers_for_event("INSERT");
    assert_eq!(insert_observers.len(), 2);

    let update_observers = schema.find_observers_for_event("UPDATE");
    assert_eq!(update_observers.len(), 1);
}

#[test]
fn test_observer_definition_builder() {
    let observer = ObserverDefinition::new("test", "Order", "INSERT")
        .with_condition("total > 1000")
        .with_action(serde_json::json!({"type": "webhook", "url": "https://example.com"}))
        .with_retry(RetryConfig::exponential(5, 1000, 60000));

    assert_eq!(observer.name, "test");
    assert_eq!(observer.entity, "Order");
    assert_eq!(observer.event, "INSERT");
    assert!(observer.has_condition());
    assert_eq!(observer.action_count(), 1);
    assert_eq!(observer.retry.max_attempts, 5);
}

#[test]
fn test_retry_config_types() {
    let exponential = RetryConfig::exponential(3, 1000, 60000);
    assert!(exponential.is_exponential());
    assert!(!exponential.is_linear());
    assert!(!exponential.is_fixed());

    let linear = RetryConfig::linear(3, 1000, 60000);
    assert!(!linear.is_exponential());
    assert!(linear.is_linear());
    assert!(!linear.is_fixed());

    let fixed = RetryConfig::fixed(3, 5000);
    assert!(!fixed.is_exponential());
    assert!(!fixed.is_linear());
    assert!(fixed.is_fixed());
    assert_eq!(fixed.initial_delay_ms, 5000);
    assert_eq!(fixed.max_delay_ms, 5000);
}

// =========================================================================
// content_hash tests
// =========================================================================

#[test]
fn test_content_hash_stable() {
    let schema = CompiledSchema::default();
    assert_eq!(
        schema.content_hash(),
        schema.content_hash(),
        "Same schema must produce same hash"
    );
}

#[test]
fn test_content_hash_length() {
    let hash = CompiledSchema::default().content_hash();
    assert_eq!(hash.len(), 32, "Hash must be 32 hex chars (16 bytes)");
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()), "Hash must be valid hex");
}

#[test]
fn test_content_hash_changes_on_field_rename() {
    let mut schema_a = CompiledSchema::default();
    schema_a
        .queries
        .push(QueryDefinition::new("users", "User").with_sql_source("v_user"));

    let mut schema_b = CompiledSchema::default();
    schema_b
        .queries
        .push(QueryDefinition::new("users", "User").with_sql_source("v_account")); // different view

    assert_ne!(
        schema_a.content_hash(),
        schema_b.content_hash(),
        "Schemas with different view names must produce different hashes"
    );
}

// =========================================================================
// #366 @subscribable tests
// =========================================================================

#[test]
fn subscribable_defaults_empty_when_absent() {
    // A compiled schema that predates #366 has no `subscribable` key — it must
    // deserialize to an empty list (back-compat), mirroring the `authorize` field.
    let json = r#"{ "types": [], "queries": [], "mutations": [], "subscriptions": [] }"#;
    let schema: CompiledSchema = serde_json::from_str(json).unwrap();
    assert!(schema.subscribable.is_empty(), "absent subscribable defaults to empty");
}

#[test]
fn subscribable_empty_is_not_serialized_so_hash_is_unchanged() {
    // skip_serializing_if = "Vec::is_empty" keeps a non-subscribable schema's JSON
    // (and therefore its content_hash) byte-identical to a pre-#366 schema — so
    // existing golden/insta fixtures need no re-blessing.
    let schema = CompiledSchema::default();
    let json = serde_json::to_string(&schema).unwrap();
    assert!(!json.contains("subscribable"), "empty subscribable is omitted: {json}");
}

#[test]
fn subscribable_round_trips_when_present() {
    let schema = CompiledSchema {
        subscribable: vec![SubscribableEntity {
            entity_type: "Post".to_string(),
            tables:      vec!["tb_post".to_string(), "public.tb_post_archive".to_string()],
            pre_image:   false,
        }],
        ..CompiledSchema::default()
    };
    let json = serde_json::to_string(&schema).unwrap();
    assert!(json.contains("subscribable"), "present subscribable is serialized");
    let back: CompiledSchema = serde_json::from_str(&json).unwrap();
    assert_eq!(back.subscribable, schema.subscribable, "subscribable round-trips");
}

#[test]
fn subscribable_participates_in_content_hash() {
    let plain = CompiledSchema::default();
    let with_sub = CompiledSchema {
        subscribable: vec![SubscribableEntity {
            entity_type: "Post".to_string(),
            tables:      vec!["tb_post".to_string()],
            pre_image:   false,
        }],
        ..CompiledSchema::default()
    };
    assert_ne!(
        plain.content_hash(),
        with_sub.content_hash(),
        "adding a subscribable entity changes the content hash"
    );
}

// =========================================================================
// has_rls_configured tests
// =========================================================================

#[test]
fn test_has_rls_configured_no_security() {
    let schema = CompiledSchema::default();
    assert!(
        !schema.has_rls_configured(),
        "Schema with no security section must return false"
    );
}

#[test]
fn test_has_rls_configured_with_empty_policies() {
    let mut sec = SecurityConfig::default();
    sec.additional.insert("policies".to_string(), serde_json::json!([]));
    let schema = CompiledSchema {
        security: Some(sec),
        ..CompiledSchema::default()
    };
    assert!(!schema.has_rls_configured(), "Empty policies array must return false");
}

#[test]
fn test_has_rls_configured_with_policies() {
    let mut sec = SecurityConfig::default();
    sec.additional.insert(
        "policies".to_string(),
        serde_json::json!([{"name": "tenant_isolation", "condition": "tenant_id = $1"}]),
    );
    let schema = CompiledSchema {
        security: Some(sec),
        ..CompiledSchema::default()
    };
    assert!(schema.has_rls_configured(), "Non-empty policies array must return true");
}

#[test]
fn test_has_rls_configured_no_policies_key() {
    let mut sec = SecurityConfig::default();
    sec.additional
        .insert("rate_limiting".to_string(), serde_json::json!({"enabled": true}));
    let schema = CompiledSchema {
        security: Some(sec),
        ..CompiledSchema::default()
    };
    assert!(!schema.has_rls_configured(), "Security without policies key must return false");
}

// ---------------------------------------------------------------------------
// schema.rs tests
// ---------------------------------------------------------------------------
// -------------------------------------------------------------------------
// Helpers
// -------------------------------------------------------------------------

fn make_type_def(name: &str) -> TypeDefinition {
    TypeDefinition {
        name:                name.into(),
        sql_source:          format!("v_{}", name.to_lowercase()).as_str().into(),
        jsonb_column:        "data".to_string(),
        fields:              vec![],
        description:         None,
        sql_projection_hint: None,
        implements:          vec![],
        requires_role:       None,
        is_error:            false,
        relay:               false,
        relationships:       vec![],
    }
}

fn make_query(name: &str, return_type: &str) -> QueryDefinition {
    QueryDefinition::new(name, return_type)
}

fn make_mutation(name: &str, return_type: &str) -> MutationDefinition {
    MutationDefinition::new(name, return_type)
}

// -------------------------------------------------------------------------
// Constructor behaviour
// -------------------------------------------------------------------------

#[test]
fn new_returns_empty_schema() {
    let schema = CompiledSchema::new();
    assert!(schema.types.is_empty());
    assert!(schema.queries.is_empty());
    assert!(schema.mutations.is_empty());
    assert!(schema.subscriptions.is_empty());
    assert!(schema.enums.is_empty());
    assert!(schema.interfaces.is_empty());
    assert!(schema.unions.is_empty());
}

#[test]
fn from_json_empty_array_fields() {
    let json = r#"{"types":[],"queries":[],"mutations":[],"subscriptions":[]}"#;
    let schema = CompiledSchema::from_json(json, false).unwrap();
    assert_eq!(schema.types.len(), 0);
    assert_eq!(schema.queries.len(), 0);
    assert_eq!(schema.mutations.len(), 0);
    assert_eq!(schema.subscriptions.len(), 0);
}

#[test]
fn from_json_minimal_empty_object() {
    // All fields have #[serde(default)] — an empty JSON object is valid
    let schema = CompiledSchema::from_json("{}", false).unwrap();
    assert!(schema.types.is_empty());
    assert!(schema.queries.is_empty());
}

#[test]
fn from_json_invalid_returns_error() {
    let result = CompiledSchema::from_json("not json at all", false);
    assert!(result.is_err());
}

#[test]
fn from_json_builds_query_index() {
    let json = r#"{
        "types": [{"name":"User","sql_source":"v_user","fields":[]}],
        "queries": [{"name":"users","return_type":"User"}],
        "mutations": [],
        "subscriptions": []
    }"#;
    let schema = CompiledSchema::from_json(json, false).unwrap();
    assert!(schema.query_index.contains_key("users"));
    assert_eq!(schema.query_index["users"], 0);
}

#[test]
fn from_json_builds_mutation_index() {
    let json = r#"{
        "types": [{"name":"User","sql_source":"v_user","fields":[]}],
        "mutations": [{"name":"createUser","return_type":"User"}],
        "queries": [],
        "subscriptions": []
    }"#;
    let schema = CompiledSchema::from_json(json, false).unwrap();
    assert!(schema.mutation_index.contains_key("createUser"));
}

// -------------------------------------------------------------------------
// Serialization round-trip
// -------------------------------------------------------------------------

#[test]
fn to_json_and_back_is_identity() {
    let mut schema = CompiledSchema::new();
    schema.schema_format_version = Some(1);
    let json = schema.to_json().unwrap();
    let schema2 = CompiledSchema::from_json(&json, false).unwrap();
    assert_eq!(schema, schema2);
}

#[test]
fn to_json_pretty_is_valid_json() {
    let schema = CompiledSchema::new();
    let pretty = schema.to_json_pretty().unwrap();
    // Should re-parse without error
    let _: serde_json::Value = serde_json::from_str(&pretty).unwrap();
}

// -------------------------------------------------------------------------
// Format version
// -------------------------------------------------------------------------

#[test]
fn validate_format_version_none_is_ok() {
    let schema = CompiledSchema::new(); // schema_format_version = None
    assert!(schema.validate_format_version().is_ok());
}

#[test]
fn validate_format_version_current_is_ok() {
    let mut schema = CompiledSchema::new();
    schema.schema_format_version = Some(CURRENT_SCHEMA_FORMAT_VERSION);
    assert!(schema.validate_format_version().is_ok());
}

#[test]
fn validate_format_version_mismatch_is_err() {
    let mut schema = CompiledSchema::new();
    schema.schema_format_version = Some(CURRENT_SCHEMA_FORMAT_VERSION + 1);
    let result = schema.validate_format_version();
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(msg.contains("mismatch"));
}

// -------------------------------------------------------------------------
// Index building
// -------------------------------------------------------------------------

#[test]
fn build_indexes_populates_all_three_maps() {
    let mut schema = CompiledSchema::new();
    schema.queries.push(make_query("getUser", "User"));
    schema.mutations.push(make_mutation("createUser", "User"));
    schema.build_indexes();
    assert!(schema.query_index.contains_key("getUser"));
    assert!(schema.mutation_index.contains_key("createUser"));
}

#[test]
fn build_indexes_multiple_queries() {
    let mut schema = CompiledSchema::new();
    schema.queries.push(make_query("alpha", "A"));
    schema.queries.push(make_query("beta", "B"));
    schema.queries.push(make_query("gamma", "C"));
    schema.build_indexes();
    assert_eq!(schema.query_index["alpha"], 0);
    assert_eq!(schema.query_index["beta"], 1);
    assert_eq!(schema.query_index["gamma"], 2);
}

// -------------------------------------------------------------------------
// Finder methods
// -------------------------------------------------------------------------

#[test]
fn find_type_returns_none_for_missing() {
    let schema = CompiledSchema::new();
    assert!(schema.find_type("Ghost").is_none());
}

#[test]
fn find_type_returns_existing() {
    let mut schema = CompiledSchema::new();
    schema.types.push(make_type_def("User"));
    assert!(schema.find_type("User").is_some());
    assert_eq!(schema.find_type("User").unwrap().name, "User");
}

#[test]
fn find_query_uses_index_when_populated() {
    let json = r#"{
        "types": [{"name":"User","sql_source":"v_user","fields":[]}],
        "queries": [{"name":"users","return_type":"User"}],
        "mutations": [],
        "subscriptions": []
    }"#;
    let schema = CompiledSchema::from_json(json, false).unwrap();
    let q = schema.find_query("users");
    assert!(q.is_some());
    assert_eq!(q.unwrap().name, "users");
}

#[test]
fn find_query_falls_back_to_linear_scan_without_index() {
    // Build schema directly without calling build_indexes
    let mut schema = CompiledSchema::new();
    schema.queries.push(make_query("direct", "String"));
    // query_index is empty but queries is not — should fall back to linear scan
    let q = schema.find_query("direct");
    assert!(q.is_some());
}

#[test]
fn find_query_returns_none_for_missing() {
    let schema = CompiledSchema::from_json("{}", false).unwrap();
    assert!(schema.find_query("nope").is_none());
}

#[test]
fn find_mutation_returns_correct_entry() {
    let json = r#"{
        "types": [{"name":"User","sql_source":"v_user","fields":[]}],
        "mutations": [{"name":"createUser","return_type":"User"}],
        "queries": [],
        "subscriptions": []
    }"#;
    let schema = CompiledSchema::from_json(json, false).unwrap();
    assert!(schema.find_mutation("createUser").is_some());
    assert!(schema.find_mutation("nope").is_none());
}

#[test]
fn find_interface_returns_none_when_absent() {
    let schema = CompiledSchema::new();
    assert!(schema.find_interface("Node").is_none());
}

#[test]
fn find_implementors_filters_by_interface() {
    let mut schema = CompiledSchema::new();
    let mut user = make_type_def("User");
    user.implements = vec!["Node".to_string()];
    schema.types.push(user);
    schema.types.push(make_type_def("Product")); // does not implement Node

    let implementors = schema.find_implementors("Node");
    assert_eq!(implementors.len(), 1);
    assert_eq!(implementors[0].name, "User");
}

// -------------------------------------------------------------------------
// operation_count
// -------------------------------------------------------------------------

#[test]
fn operation_count_sums_all_three() {
    let mut schema = CompiledSchema::new();
    schema.queries.push(make_query("q1", "String"));
    schema.queries.push(make_query("q2", "String"));
    schema.mutations.push(make_mutation("m1", "String"));
    assert_eq!(schema.operation_count(), 3);
}

#[test]
fn operation_count_zero_for_empty_schema() {
    assert_eq!(CompiledSchema::new().operation_count(), 0);
}

// -------------------------------------------------------------------------
// Fact tables
// -------------------------------------------------------------------------

#[test]
fn fact_table_add_and_get() {
    use crate::compiler::fact_table::{DimensionColumn, FactTableMetadata};

    let mut schema = CompiledSchema::new();
    assert!(!schema.has_fact_tables());

    let meta = FactTableMetadata {
        table_name:               "tf_sales".to_string(),
        measures:                 vec![],
        dimensions:               DimensionColumn {
            name:  "data".to_string(),
            paths: vec![],
        },
        denormalized_filters:     vec![],
        calendar_dimensions:      vec![],
        partial_period:           None,
        native_measures:          std::collections::HashMap::new(),
        native_dimension_mapping: std::collections::HashMap::new(),
    };
    schema.add_fact_table("tf_sales".to_string(), meta);

    assert!(schema.has_fact_tables());
    assert!(schema.get_fact_table("tf_sales").is_some());
    assert!(schema.get_fact_table("tf_missing").is_none());
}

#[test]
fn list_fact_tables_returns_all_names() {
    use crate::compiler::fact_table::{DimensionColumn, FactTableMetadata};

    let make_meta = |name: &str| FactTableMetadata {
        table_name:               name.to_string(),
        measures:                 vec![],
        dimensions:               DimensionColumn {
            name:  "data".to_string(),
            paths: vec![],
        },
        denormalized_filters:     vec![],
        calendar_dimensions:      vec![],
        partial_period:           None,
        native_measures:          std::collections::HashMap::new(),
        native_dimension_mapping: std::collections::HashMap::new(),
    };

    let mut schema = CompiledSchema::new();
    schema.add_fact_table("tf_a".to_string(), make_meta("tf_a"));
    schema.add_fact_table("tf_b".to_string(), make_meta("tf_b"));

    let names = schema.list_fact_tables();
    assert_eq!(names.len(), 2);
    assert!(names.contains(&"tf_a"));
    assert!(names.contains(&"tf_b"));
}

// -------------------------------------------------------------------------
// Observers
// -------------------------------------------------------------------------

#[test]
fn has_observers_false_for_empty_schema() {
    assert!(!CompiledSchema::new().has_observers());
}

#[test]
fn find_observer_returns_by_name() {
    let mut schema = CompiledSchema::new();
    schema.observers.push(ObserverDefinition::new("onInsert", "Order", "INSERT"));
    assert!(schema.find_observer("onInsert").is_some());
    assert!(schema.find_observer("missing").is_none());
}

#[test]
fn find_observers_for_entity_filters_correctly() {
    let mut schema = CompiledSchema::new();
    schema.observers.push(ObserverDefinition::new("obs1", "Order", "INSERT"));
    schema.observers.push(ObserverDefinition::new("obs2", "Order", "UPDATE"));
    schema.observers.push(ObserverDefinition::new("obs3", "User", "INSERT"));

    let order_obs = schema.find_observers_for_entity("Order");
    assert_eq!(order_obs.len(), 2);
    let user_obs = schema.find_observers_for_entity("User");
    assert_eq!(user_obs.len(), 1);
}

#[test]
fn find_observers_for_event_filters_correctly() {
    let mut schema = CompiledSchema::new();
    schema.observers.push(ObserverDefinition::new("obs1", "Order", "INSERT"));
    schema.observers.push(ObserverDefinition::new("obs2", "User", "INSERT"));
    schema.observers.push(ObserverDefinition::new("obs3", "Order", "DELETE"));

    let inserts = schema.find_observers_for_event("INSERT");
    assert_eq!(inserts.len(), 2);
}

#[test]
fn observer_count_matches_vec_length() {
    let mut schema = CompiledSchema::new();
    assert_eq!(schema.observer_count(), 0);
    schema.observers.push(ObserverDefinition::new("o1", "A", "INSERT"));
    assert_eq!(schema.observer_count(), 1);
}

// -------------------------------------------------------------------------
// Security helpers
// -------------------------------------------------------------------------

#[test]
fn is_multi_tenant_false_by_default() {
    assert!(!CompiledSchema::new().is_multi_tenant());
}

#[test]
fn is_multi_tenant_true_when_configured() {
    let mut schema = CompiledSchema::new();
    let mut sec = SecurityConfig::new();
    sec.multi_tenant = true;
    schema.security = Some(sec);
    assert!(schema.is_multi_tenant());
}

// ── tenancy_mode / tenancy_config ──────────────────────────────────

#[test]
fn tenancy_mode_none_by_default() {
    use crate::schema::TenancyMode;
    assert_eq!(CompiledSchema::new().tenancy_mode(), TenancyMode::None);
}

#[test]
fn tenancy_mode_row_when_configured() {
    use crate::schema::{TenancyConfig, TenancyMode};
    let mut schema = CompiledSchema::new();
    let mut sec = SecurityConfig::new();
    sec.tenancy = TenancyConfig {
        mode:         TenancyMode::Row,
        tenant_claim: "tenant_id".to_string(),
    };
    schema.security = Some(sec);
    assert_eq!(schema.tenancy_mode(), TenancyMode::Row);
}

#[test]
fn tenancy_mode_schema_when_configured() {
    use crate::schema::{TenancyConfig, TenancyMode};
    let mut schema = CompiledSchema::new();
    let mut sec = SecurityConfig::new();
    sec.tenancy = TenancyConfig {
        mode:         TenancyMode::Schema,
        tenant_claim: "org_id".to_string(),
    };
    schema.security = Some(sec);
    assert_eq!(schema.tenancy_mode(), TenancyMode::Schema);
}

#[test]
fn tenancy_config_none_without_security() {
    assert!(CompiledSchema::new().tenancy_config().is_none());
}

#[test]
fn tenancy_config_returns_default_when_security_present() {
    use crate::schema::TenancyMode;
    let mut schema = CompiledSchema::new();
    schema.security = Some(SecurityConfig::new());
    let tc = schema.tenancy_config().unwrap();
    assert_eq!(tc.mode, TenancyMode::None);
    assert_eq!(tc.tenant_claim, "tenant_id");
}

#[test]
fn tenancy_round_trip_through_json() {
    use crate::schema::{TenancyConfig, TenancyMode};
    let mut schema = CompiledSchema::new();
    let mut sec = SecurityConfig::new();
    sec.tenancy = TenancyConfig {
        mode:         TenancyMode::Row,
        tenant_claim: "org_id".to_string(),
    };
    schema.security = Some(sec);
    schema.schema_format_version = Some(1);

    let json = schema.to_json().unwrap();
    let restored = CompiledSchema::from_json(&json, false).unwrap();
    assert_eq!(restored.tenancy_mode(), TenancyMode::Row);
    assert_eq!(restored.tenancy_config().unwrap().tenant_claim, "org_id");
}

#[test]
fn find_role_returns_none_without_security_config() {
    assert!(CompiledSchema::new().find_role("admin").is_none());
}

#[test]
fn find_role_returns_defined_role() {
    let mut schema = CompiledSchema::new();
    let mut sec = SecurityConfig::new();
    sec.add_role(RoleDefinition::new("editor", vec!["read:*".to_string()]));
    schema.security = Some(sec);
    assert!(schema.find_role("editor").is_some());
}

#[test]
fn role_has_scope_false_without_security() {
    assert!(!CompiledSchema::new().role_has_scope("admin", "read:*"));
}

#[test]
fn role_has_scope_true_when_granted() {
    let mut schema = CompiledSchema::new();
    let mut sec = SecurityConfig::new();
    sec.add_role(RoleDefinition::new("admin", vec!["read:*".to_string()]));
    schema.security = Some(sec);
    assert!(schema.role_has_scope("admin", "read:anything"));
    assert!(!schema.role_has_scope("admin", "write:anything"));
}

#[test]
fn get_role_scopes_empty_for_missing_role() {
    let schema = CompiledSchema::new();
    assert!(schema.get_role_scopes("ghost").is_empty());
}

// -------------------------------------------------------------------------
// Federation metadata
// -------------------------------------------------------------------------

#[test]
fn federation_metadata_none_when_no_federation() {
    assert!(CompiledSchema::new().federation_metadata().is_none());
}

#[test]
fn federation_metadata_none_when_disabled() {
    let mut schema = CompiledSchema::new();
    schema.federation = Some(FederationConfig {
        enabled: false,
        ..Default::default()
    });
    assert!(schema.federation_metadata().is_none());
}

#[test]
#[cfg(feature = "federation")]
fn federation_metadata_some_when_enabled() {
    let mut schema = CompiledSchema::new();
    schema.federation = Some(FederationConfig {
        enabled: true,
        version: Some("v2".to_string()),
        entities: vec![FederationEntity {
            name:       "User".to_string(),
            key_fields: vec!["id".to_string()],
        }],
        ..Default::default()
    });
    let meta = schema.federation_metadata();
    assert!(meta.is_some());
    let meta = meta.unwrap();
    assert!(meta.enabled);
    assert_eq!(meta.types.len(), 1);
    assert_eq!(meta.types[0].name, "User");
}

// -------------------------------------------------------------------------
// content_hash
// -------------------------------------------------------------------------

#[test]
fn content_hash_is_32_hex_chars() {
    let hash = CompiledSchema::new().content_hash();
    assert_eq!(hash.len(), 32);
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn content_hash_is_stable() {
    let schema = CompiledSchema::new();
    assert_eq!(schema.content_hash(), schema.content_hash());
}

#[test]
fn content_hash_differs_for_different_schemas() {
    let s1 = CompiledSchema::new();
    let mut s2 = CompiledSchema::new();
    s2.schema_format_version = Some(1);
    assert_ne!(s1.content_hash(), s2.content_hash());
}

// -------------------------------------------------------------------------
// has_rls_configured
// -------------------------------------------------------------------------

#[test]
fn has_rls_configured_false_without_security() {
    assert!(!CompiledSchema::new().has_rls_configured());
}

#[test]
fn has_rls_configured_false_when_policies_empty() {
    let mut schema = CompiledSchema::new();
    let mut sec = SecurityConfig::new();
    sec.additional.insert("policies".to_string(), serde_json::json!([]));
    schema.security = Some(sec);
    assert!(!schema.has_rls_configured());
}

#[test]
fn has_rls_configured_true_when_policies_present() {
    let mut schema = CompiledSchema::new();
    let mut sec = SecurityConfig::new();
    sec.additional.insert(
        "policies".to_string(),
        serde_json::json!([{"table": "orders", "using": "tenant_id = current_setting('app.tenant_id')"}]),
    );
    schema.security = Some(sec);
    assert!(schema.has_rls_configured());
}

// -------------------------------------------------------------------------
// validate()
// -------------------------------------------------------------------------

#[test]
fn validate_empty_schema_is_ok() {
    assert!(CompiledSchema::new().validate().is_ok());
}

#[test]
fn validate_detects_duplicate_type_names() {
    let mut schema = CompiledSchema::new();
    schema.types.push(make_type_def("User"));
    schema.types.push(make_type_def("User")); // duplicate
    let result = schema.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("Duplicate type name")));
}

#[test]
fn validate_detects_duplicate_query_names() {
    let mut schema = CompiledSchema::new();
    schema.queries.push(make_query("getUser", "String"));
    schema.queries.push(make_query("getUser", "String")); // duplicate
    let result = schema.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("Duplicate query name")));
}

#[test]
fn validate_detects_duplicate_mutation_names() {
    let mut schema = CompiledSchema::new();
    schema.mutations.push(make_mutation("createUser", "String"));
    schema.mutations.push(make_mutation("createUser", "String")); // duplicate
    let result = schema.validate();
    assert!(result.is_err());
}

#[test]
fn validate_undefined_return_type_in_query_is_error() {
    let mut schema = CompiledSchema::new();
    // No "Widget" type defined
    schema.queries.push(make_query("getWidget", "Widget"));
    let result = schema.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("Widget")));
}

#[test]
fn validate_builtin_scalar_return_type_is_ok() {
    let mut schema = CompiledSchema::new();
    schema.queries.push(make_query("ping", "String"));
    schema.queries.push(make_query("count", "Int"));
    assert!(schema.validate().is_ok());
}

#[test]
fn validate_defined_type_as_return_type_is_ok() {
    let mut schema = CompiledSchema::new();
    schema.types.push(make_type_def("User"));
    schema.queries.push(make_query("getUser", "User"));
    assert!(schema.validate().is_ok());
}

// -------------------------------------------------------------------------
// raw_schema
// -------------------------------------------------------------------------

#[test]
fn raw_schema_returns_sdl_when_set() {
    let mut schema = CompiledSchema::new();
    schema.schema_sdl = Some("type Query { ping: String }".to_string());
    assert_eq!(schema.raw_schema(), "type Query { ping: String }");
}

#[test]
fn raw_schema_generates_from_types_when_sdl_absent() {
    let mut schema = CompiledSchema::new();
    schema.types.push(make_type_def("User"));
    let sdl = schema.raw_schema();
    assert!(sdl.contains("User"));
}

#[test]
fn raw_schema_includes_root_query_and_mutation() {
    // Root operations live in `queries`/`mutations`, NOT as `Query`/`Mutation`
    // object types in `types`. The generated SDL must still render them — otherwise
    // it advertises no root fields, and the federation `_service` SDL built from it
    // fails gateway composition with NO_QUERIES.
    use crate::schema::{ArgumentDefinition, FieldType};

    let mut schema = CompiledSchema::new();
    schema.types.push(make_type_def("User"));

    let mut users = QueryDefinition::new("users", "User");
    users.returns_list = true;
    let mut user = QueryDefinition::new("user", "User");
    user.nullable = true;
    user.arguments = vec![ArgumentDefinition::new("id", FieldType::Id)];
    schema.queries.push(users);
    schema.queries.push(user);

    let mut create = MutationDefinition::new("createUser", "User");
    create.arguments = vec![ArgumentDefinition::new("name", FieldType::String)];
    schema.mutations.push(create);

    let sdl = schema.raw_schema();

    assert!(sdl.contains("type Query {"), "SDL must declare a root Query type:\n{sdl}");
    assert!(sdl.contains("users: [User!]!"), "list query rendered as a list type:\n{sdl}");
    assert!(sdl.contains("user(id: ID!): User"), "nullable single query with arg:\n{sdl}");
    assert!(sdl.contains("type Mutation {"), "SDL must declare a root Mutation type:\n{sdl}");
    assert!(sdl.contains("createUser(name: String!): User"), "mutation field:\n{sdl}");
}

#[cfg(feature = "federation")]
#[test]
fn service_sdl_advertises_root_query_fields() {
    // End-to-end: the `_service { sdl }` a gateway composes must expose the root
    // query fields, the entity `@key`, and the federation plumbing together.
    let mut schema = CompiledSchema::new();
    schema.types.push(make_type_def("User"));
    let mut users = QueryDefinition::new("users", "User");
    users.returns_list = true;
    schema.queries.push(users);
    schema.federation = Some(FederationConfig {
        enabled: true,
        version: Some("v2".to_string()),
        entities: vec![FederationEntity {
            name:       "User".to_string(),
            key_fields: vec!["id".to_string()],
        }],
        ..Default::default()
    });

    let meta = schema.federation_metadata().expect("federation enabled");
    let sdl = crate::federation::generate_service_sdl(&schema.raw_schema(), &meta);

    assert!(
        sdl.contains("users: [User!]!"),
        "root query must be in the _service SDL:\n{sdl}"
    );
    assert!(sdl.contains("@key(fields: \"id\")"), "entity key must be present:\n{sdl}");
    assert!(sdl.contains("_service"), "federation plumbing must be present:\n{sdl}");
}

// -------------------------------------------------------------------------
// is_builtin_type (private fn — tested via validate())
// -------------------------------------------------------------------------

#[test]
fn builtin_scalar_types_pass_validation() {
    let scalars = [
        "String", "Int", "Float", "Boolean", "ID", "DateTime", "Date", "Time", "JSON", "UUID",
        "Decimal",
    ];
    for scalar in scalars {
        let mut schema = CompiledSchema::new();
        schema.queries.push(make_query("q", scalar));
        assert!(schema.validate().is_ok(), "{scalar} should be a recognised built-in");
    }
}

#[test]
fn unknown_scalar_fails_validation() {
    let mut schema = CompiledSchema::new();
    schema.queries.push(make_query("q", "Blob"));
    assert!(schema.validate().is_err());
}

// ── Operation name normalization (issue #199) ────────────────────────

#[test]
fn find_query_exact_match() {
    let mut schema = CompiledSchema::new();
    schema.types.push(make_type_def("User"));
    schema.queries.push(make_query("users", "User"));
    schema.build_indexes();
    assert!(schema.find_query("users").is_some());
}

#[test]
fn find_query_camel_to_snake_fallback() {
    let mut schema = CompiledSchema::new();
    schema.types.push(make_type_def("DnsServer"));
    schema.queries.push(make_query("dns_servers", "DnsServer"));
    schema.build_indexes();
    // Exact match works
    assert!(schema.find_query("dns_servers").is_some());
    // camelCase fallback also works
    assert!(schema.find_query("dnsServers").is_some());
}

#[test]
fn find_query_camel_to_snake_fallback_without_index() {
    let mut schema = CompiledSchema::new();
    schema.types.push(make_type_def("DnsServer"));
    schema.queries.push(make_query("dns_servers", "DnsServer"));
    // No build_indexes() — exercises the linear scan fallback path
    assert!(schema.find_query("dnsServers").is_some());
}

#[test]
fn find_mutation_camel_to_snake_fallback() {
    let mut schema = CompiledSchema::new();
    schema.types.push(make_type_def("Location"));
    schema.mutations.push(make_mutation("create_location", "Location"));
    schema.build_indexes();
    assert!(schema.find_mutation("createLocation").is_some());
}

#[test]
fn find_query_returns_none_for_unknown() {
    let mut schema = CompiledSchema::new();
    schema.types.push(make_type_def("User"));
    schema.queries.push(make_query("users", "User"));
    schema.build_indexes();
    assert!(schema.find_query("nonexistent").is_none());
}

// ── NamingConvention (issue #216) ───────────────────────────────────

#[test]
fn display_name_preserve_returns_unchanged() {
    let schema = CompiledSchema::new(); // default = Preserve
    assert_eq!(schema.display_name("create_dns_server"), "create_dns_server");
    assert_eq!(schema.display_name("dns_servers"), "dns_servers");
}

#[test]
fn display_name_camel_case_converts() {
    let mut schema = CompiledSchema::new();
    schema.naming_convention = NamingConvention::CamelCase;
    assert_eq!(schema.display_name("create_dns_server"), "createDnsServer");
    assert_eq!(schema.display_name("dns_servers"), "dnsServers");
    assert_eq!(schema.display_name("delete_outreach_sequence"), "deleteOutreachSequence");
}

#[test]
fn camel_case_index_lookup() {
    let mut schema = CompiledSchema::new();
    schema.naming_convention = NamingConvention::CamelCase;
    schema.types.push(make_type_def("DnsServer"));
    schema.queries.push(make_query("dns_servers", "DnsServer"));
    schema.mutations.push(make_mutation("create_dns_server", "DnsServer"));
    schema.build_indexes();

    // camelCase lookup via index
    assert!(schema.find_query("dnsServers").is_some());
    assert!(schema.find_mutation("createDnsServer").is_some());

    // Original snake_case still works
    assert!(schema.find_query("dns_servers").is_some());
    assert!(schema.find_mutation("create_dns_server").is_some());
}

#[test]
fn preserve_convention_no_camel_index_entry() {
    let mut schema = CompiledSchema::new(); // Preserve
    schema.types.push(make_type_def("DnsServer"));
    schema.queries.push(make_query("dns_servers", "DnsServer"));
    schema.build_indexes();

    // Only 1 index entry (the original name); camelCase only works via fallback
    assert_eq!(schema.query_index.len(), 1);
    assert!(schema.query_index.contains_key("dns_servers"));
}

#[test]
fn naming_convention_serde_in_compiled_schema() {
    let mut schema = CompiledSchema::new();
    schema.naming_convention = NamingConvention::CamelCase;
    let json = schema.to_json().unwrap();
    let restored = CompiledSchema::from_json(&json, false).unwrap();
    assert_eq!(restored.naming_convention, NamingConvention::CamelCase);
}

#[test]
fn schema_integrity_verification() {
    use sha2::{Digest, Sha256};

    let schema = CompiledSchema::new();
    let body = schema.to_json().unwrap();

    // Simulate CLI: parse to Value, serialize without hash to get canonical form,
    // then compute hash on that canonical form (matches what from_json verifies against).
    let value: serde_json::Value = serde_json::from_str(&body).unwrap();
    let canonical = serde_json::to_string_pretty(&value).unwrap();
    let hash = Sha256::digest(canonical.as_bytes());
    let hash_hex = hex::encode(&hash[..16]);

    // Build wrapped JSON with _content_hash as first field
    let obj = value.as_object().unwrap();
    let mut new_obj = serde_json::Map::new();
    new_obj.insert("_content_hash".to_string(), serde_json::Value::String(hash_hex));
    for (k, v) in obj {
        new_obj.insert(k.clone(), v.clone());
    }
    let wrapped_json = serde_json::to_string_pretty(&serde_json::Value::Object(new_obj)).unwrap();

    // from_json with strict=true should accept
    let restored = CompiledSchema::from_json(&wrapped_json, true).unwrap();
    assert_eq!(restored.types.len(), schema.types.len());

    // Test mismatch: change hash in Value
    let mut mismatch_value: serde_json::Value = serde_json::from_str(&wrapped_json).unwrap();
    let mismatch_obj = mismatch_value.as_object_mut().unwrap();
    mismatch_obj.insert(
        "_content_hash".to_string(),
        serde_json::Value::String("0000000000000000".to_string()),
    );
    let mismatch_json = serde_json::to_string_pretty(&mismatch_value).unwrap();
    let result = CompiledSchema::from_json(&mismatch_json, true);
    assert!(result.is_err(), "Expected validation error for hash mismatch");

    // Test missing hash with strict
    let result = CompiledSchema::from_json(&body, true);
    assert!(result.is_err(), "Expected error for missing hash in strict mode");

    // Test missing hash with non-strict
    let restored = CompiledSchema::from_json(&body, false).unwrap();
    assert_eq!(restored.types.len(), schema.types.len());
}

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use fraiseql_core::schema::NamingConvention;
use indexmap::IndexMap;

use crate::schema::{
    intermediate::{IntermediateQuery, IntermediateSchema, IntermediateType},
    validator::schema_validator::SchemaValidator,
};

#[test]
fn test_validate_empty_schema() {
    let schema = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![],
        enums:                vec![],
        input_types:          vec![],
        interfaces:           vec![],
        unions:               vec![],
        queries:              vec![],
        mutations:            vec![],
        subscriptions:        vec![],
        fragments:            None,
        directives:           None,
        fact_tables:          None,
        aggregate_queries:    None,
        observers:            None,
        custom_scalars:       None,
        observers_config:     None,
        subscriptions_config: None,
        validation_config:    None,
        federation_config:    None,
        debug_config:         None,
        mcp_config:           None,
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
    };

    let report = SchemaValidator::validate(&schema).unwrap();
    assert!(report.is_valid());
}

#[test]
fn test_detect_unknown_return_type() {
    let schema = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![],
        enums:                vec![],
        input_types:          vec![],
        interfaces:           vec![],
        unions:               vec![],
        queries:              vec![IntermediateQuery {
            name:              "users".to_string(),
            return_type:       "UnknownType".to_string(),
            returns_list:      true,
            nullable:          false,
            arguments:         vec![],
            description:       None,
            sql_source:        Some("users".to_string()),
            auto_params:       None,
            deprecated:        None,
            jsonb_column:      None,
            relay:             false,
            inject:            IndexMap::default(),
            cache_ttl_seconds: None,
            additional_views:  vec![],
            requires_role:     None,
            relay_cursor_type: None,
        }],
        mutations:            vec![],
        subscriptions:        vec![],
        fragments:            None,
        directives:           None,
        fact_tables:          None,
        aggregate_queries:    None,
        observers:            None,
        custom_scalars:       None,
        observers_config:     None,
        subscriptions_config: None,
        validation_config:    None,
        federation_config:    None,
        debug_config:         None,
        mcp_config:           None,
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
    };

    let report = SchemaValidator::validate(&schema).unwrap();
    assert!(!report.is_valid());
    assert_eq!(report.error_count(), 1);
    assert!(report.errors[0].message.contains("unknown type 'UnknownType'"));
}

#[test]
fn test_detect_duplicate_query_names() {
    let schema = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![IntermediateType {
            name:          "User".to_string(),
            fields:        vec![],
            description:   None,
            implements:    vec![],
            requires_role: None,
            is_error:      false,
            relay:         false,
        }],
        enums:                vec![],
        input_types:          vec![],
        interfaces:           vec![],
        unions:               vec![],
        queries:              vec![
            IntermediateQuery {
                name:              "users".to_string(),
                return_type:       "User".to_string(),
                returns_list:      true,
                nullable:          false,
                arguments:         vec![],
                description:       None,
                sql_source:        Some("users".to_string()),
                auto_params:       None,
                deprecated:        None,
                jsonb_column:      None,
                relay:             false,
                inject:            IndexMap::default(),
                cache_ttl_seconds: None,
                additional_views:  vec![],
                requires_role:     None,
                relay_cursor_type: None,
            },
            IntermediateQuery {
                name:              "users".to_string(), // Duplicate!
                return_type:       "User".to_string(),
                returns_list:      true,
                nullable:          false,
                arguments:         vec![],
                description:       None,
                sql_source:        Some("users".to_string()),
                auto_params:       None,
                deprecated:        None,
                jsonb_column:      None,
                relay:             false,
                inject:            IndexMap::default(),
                cache_ttl_seconds: None,
                additional_views:  vec![],
                requires_role:     None,
                relay_cursor_type: None,
            },
        ],
        mutations:            vec![],
        subscriptions:        vec![],
        fragments:            None,
        directives:           None,
        fact_tables:          None,
        aggregate_queries:    None,
        observers:            None,
        custom_scalars:       None,
        observers_config:     None,
        subscriptions_config: None,
        validation_config:    None,
        federation_config:    None,
        debug_config:         None,
        mcp_config:           None,
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
    };

    let report = SchemaValidator::validate(&schema).unwrap();
    assert!(!report.is_valid());
    assert!(report.errors.iter().any(|e| e.message.contains("Duplicate query name")));
}

#[test]
fn test_warning_for_query_without_sql_source() {
    let schema = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![IntermediateType {
            name:          "User".to_string(),
            fields:        vec![],
            description:   None,
            implements:    vec![],
            requires_role: None,
            is_error:      false,
            relay:         false,
        }],
        enums:                vec![],
        input_types:          vec![],
        interfaces:           vec![],
        unions:               vec![],
        queries:              vec![IntermediateQuery {
            name:              "users".to_string(),
            return_type:       "User".to_string(),
            returns_list:      true,
            nullable:          false,
            arguments:         vec![],
            description:       None,
            sql_source:        None, // Missing SQL source
            auto_params:       None,
            deprecated:        None,
            jsonb_column:      None,
            relay:             false,
            inject:            IndexMap::default(),
            cache_ttl_seconds: None,
            additional_views:  vec![],
            requires_role:     None,
            relay_cursor_type: None,
        }],
        mutations:            vec![],
        subscriptions:        vec![],
        fragments:            None,
        directives:           None,
        fact_tables:          None,
        aggregate_queries:    None,
        observers:            None,
        custom_scalars:       None,
        observers_config:     None,
        subscriptions_config: None,
        validation_config:    None,
        federation_config:    None,
        debug_config:         None,
        mcp_config:           None,
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
    };

    let report = SchemaValidator::validate(&schema).unwrap();
    assert!(report.is_valid()); // Still valid, just a warning
    assert_eq!(report.warning_count(), 1);
    assert!(report.errors[0].message.contains("no sql_source"));
}

#[test]
fn test_valid_observer() {
    use serde_json::json;

    use crate::schema::intermediate::{IntermediateObserver, IntermediateRetryConfig};

    let schema = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![IntermediateType {
            name:          "Order".to_string(),
            fields:        vec![],
            description:   None,
            implements:    vec![],
            requires_role: None,
            is_error:      false,
            relay:         false,
        }],
        enums:                vec![],
        input_types:          vec![],
        interfaces:           vec![],
        unions:               vec![],
        queries:              vec![],
        mutations:            vec![],
        subscriptions:        vec![],
        fragments:            None,
        directives:           None,
        fact_tables:          None,
        aggregate_queries:    None,
        observers:            Some(vec![IntermediateObserver {
            name:      "onOrderCreated".to_string(),
            entity:    "Order".to_string(),
            event:     "INSERT".to_string(),
            actions:   vec![json!({
                "type": "webhook",
                "url": "https://example.com/orders"
            })],
            condition: None,
            retry:     IntermediateRetryConfig {
                max_attempts:     3,
                backoff_strategy: "exponential".to_string(),
                initial_delay_ms: 100,
                max_delay_ms:     60000,
            },
        }]),
        custom_scalars:       None,
        observers_config:     None,
        subscriptions_config: None,
        validation_config:    None,
        federation_config:    None,
        debug_config:         None,
        mcp_config:           None,
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
    };

    let report = SchemaValidator::validate(&schema).unwrap();
    assert!(report.is_valid(), "Valid observer should pass validation");
    assert_eq!(report.error_count(), 0);
}

#[test]
fn test_observer_with_unknown_entity() {
    use serde_json::json;

    use crate::schema::intermediate::{IntermediateObserver, IntermediateRetryConfig};

    let schema = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![],
        enums:                vec![],
        input_types:          vec![],
        interfaces:           vec![],
        unions:               vec![],
        queries:              vec![],
        mutations:            vec![],
        subscriptions:        vec![],
        fragments:            None,
        directives:           None,
        fact_tables:          None,
        aggregate_queries:    None,
        observers:            Some(vec![IntermediateObserver {
            name:      "onOrderCreated".to_string(),
            entity:    "UnknownEntity".to_string(),
            event:     "INSERT".to_string(),
            actions:   vec![json!({"type": "webhook", "url": "https://example.com"})],
            condition: None,
            retry:     IntermediateRetryConfig {
                max_attempts:     3,
                backoff_strategy: "exponential".to_string(),
                initial_delay_ms: 100,
                max_delay_ms:     60000,
            },
        }]),
        custom_scalars:       None,
        observers_config:     None,
        subscriptions_config: None,
        validation_config:    None,
        federation_config:    None,
        debug_config:         None,
        mcp_config:           None,
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
    };

    let report = SchemaValidator::validate(&schema).unwrap();
    assert!(!report.is_valid());
    assert!(report.errors.iter().any(|e| e.message.contains("unknown entity")));
}

#[test]
fn test_observer_with_invalid_event() {
    use serde_json::json;

    use crate::schema::intermediate::{IntermediateObserver, IntermediateRetryConfig};

    let schema = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![IntermediateType {
            name:          "Order".to_string(),
            fields:        vec![],
            description:   None,
            implements:    vec![],
            requires_role: None,
            is_error:      false,
            relay:         false,
        }],
        enums:                vec![],
        input_types:          vec![],
        interfaces:           vec![],
        unions:               vec![],
        queries:              vec![],
        mutations:            vec![],
        subscriptions:        vec![],
        fragments:            None,
        directives:           None,
        fact_tables:          None,
        aggregate_queries:    None,
        observers:            Some(vec![IntermediateObserver {
            name:      "onOrderCreated".to_string(),
            entity:    "Order".to_string(),
            event:     "INVALID_EVENT".to_string(),
            actions:   vec![json!({"type": "webhook", "url": "https://example.com"})],
            condition: None,
            retry:     IntermediateRetryConfig {
                max_attempts:     3,
                backoff_strategy: "exponential".to_string(),
                initial_delay_ms: 100,
                max_delay_ms:     60000,
            },
        }]),
        custom_scalars:       None,
        observers_config:     None,
        subscriptions_config: None,
        validation_config:    None,
        federation_config:    None,
        debug_config:         None,
        mcp_config:           None,
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
    };

    let report = SchemaValidator::validate(&schema).unwrap();
    assert!(!report.is_valid());
    assert!(report.errors.iter().any(|e| e.message.contains("invalid event")));
}

#[test]
fn test_observer_with_invalid_action_type() {
    use serde_json::json;

    use crate::schema::intermediate::{IntermediateObserver, IntermediateRetryConfig};

    let schema = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![IntermediateType {
            name:          "Order".to_string(),
            fields:        vec![],
            description:   None,
            implements:    vec![],
            requires_role: None,
            is_error:      false,
            relay:         false,
        }],
        enums:                vec![],
        input_types:          vec![],
        interfaces:           vec![],
        unions:               vec![],
        queries:              vec![],
        mutations:            vec![],
        subscriptions:        vec![],
        fragments:            None,
        directives:           None,
        fact_tables:          None,
        aggregate_queries:    None,
        observers:            Some(vec![IntermediateObserver {
            name:      "onOrderCreated".to_string(),
            entity:    "Order".to_string(),
            event:     "INSERT".to_string(),
            actions:   vec![json!({"type": "invalid_action"})],
            condition: None,
            retry:     IntermediateRetryConfig {
                max_attempts:     3,
                backoff_strategy: "exponential".to_string(),
                initial_delay_ms: 100,
                max_delay_ms:     60000,
            },
        }]),
        custom_scalars:       None,
        observers_config:     None,
        subscriptions_config: None,
        validation_config:    None,
        federation_config:    None,
        debug_config:         None,
        mcp_config:           None,
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
    };

    let report = SchemaValidator::validate(&schema).unwrap();
    assert!(!report.is_valid());
    assert!(report.errors.iter().any(|e| e.message.contains("invalid type")));
}

#[test]
fn test_observer_with_invalid_retry_config() {
    use serde_json::json;

    use crate::schema::intermediate::{IntermediateObserver, IntermediateRetryConfig};

    let schema = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![IntermediateType {
            name:          "Order".to_string(),
            fields:        vec![],
            description:   None,
            implements:    vec![],
            requires_role: None,
            is_error:      false,
            relay:         false,
        }],
        enums:                vec![],
        input_types:          vec![],
        interfaces:           vec![],
        unions:               vec![],
        queries:              vec![],
        mutations:            vec![],
        subscriptions:        vec![],
        fragments:            None,
        directives:           None,
        fact_tables:          None,
        aggregate_queries:    None,
        observers:            Some(vec![IntermediateObserver {
            name:      "onOrderCreated".to_string(),
            entity:    "Order".to_string(),
            event:     "INSERT".to_string(),
            actions:   vec![json!({"type": "webhook", "url": "https://example.com"})],
            condition: None,
            retry:     IntermediateRetryConfig {
                max_attempts:     3,
                backoff_strategy: "invalid_strategy".to_string(),
                initial_delay_ms: 100,
                max_delay_ms:     60000,
            },
        }]),
        custom_scalars:       None,
        observers_config:     None,
        subscriptions_config: None,
        validation_config:    None,
        federation_config:    None,
        debug_config:         None,
        mcp_config:           None,
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
    };

    let report = SchemaValidator::validate(&schema).unwrap();
    assert!(!report.is_valid());
    assert!(report.errors.iter().any(|e| e.message.contains("invalid backoff_strategy")));
}

#[test]
fn test_query_injection_in_sql_source_rejected() {
    let schema = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![IntermediateType {
            name:          "User".to_string(),
            fields:        vec![],
            description:   None,
            implements:    vec![],
            requires_role: None,
            is_error:      false,
            relay:         false,
        }],
        enums:                vec![],
        input_types:          vec![],
        interfaces:           vec![],
        unions:               vec![],
        queries:              vec![IntermediateQuery {
            name:              "users".to_string(),
            return_type:       "User".to_string(),
            returns_list:      true,
            nullable:          false,
            arguments:         vec![],
            description:       None,
            sql_source:        Some("v_user\"; DROP TABLE users; --".to_string()),
            auto_params:       None,
            deprecated:        None,
            jsonb_column:      None,
            relay:             false,
            inject:            IndexMap::default(),
            cache_ttl_seconds: None,
            additional_views:  vec![],
            requires_role:     None,
            relay_cursor_type: None,
        }],
        mutations:            vec![],
        subscriptions:        vec![],
        fragments:            None,
        directives:           None,
        fact_tables:          None,
        aggregate_queries:    None,
        observers:            None,
        custom_scalars:       None,
        observers_config:     None,
        subscriptions_config: None,
        validation_config:    None,
        federation_config:    None,
        debug_config:         None,
        mcp_config:           None,
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
    };

    let report = SchemaValidator::validate(&schema).unwrap();
    assert!(!report.is_valid());
    assert!(report.errors.iter().any(|e| e.message.contains("valid SQL identifier")));
}

#[test]
fn test_query_schema_qualified_sql_source_passes() {
    let schema = IntermediateSchema {
        security:             None,
        version:              "2.0.0".to_string(),
        types:                vec![IntermediateType {
            name:          "User".to_string(),
            fields:        vec![],
            description:   None,
            implements:    vec![],
            requires_role: None,
            is_error:      false,
            relay:         false,
        }],
        enums:                vec![],
        input_types:          vec![],
        interfaces:           vec![],
        unions:               vec![],
        queries:              vec![IntermediateQuery {
            name:              "users".to_string(),
            return_type:       "User".to_string(),
            returns_list:      true,
            nullable:          false,
            arguments:         vec![],
            description:       None,
            sql_source:        Some("public.v_user".to_string()),
            auto_params:       None,
            deprecated:        None,
            jsonb_column:      None,
            relay:             false,
            inject:            IndexMap::default(),
            cache_ttl_seconds: None,
            additional_views:  vec![],
            requires_role:     None,
            relay_cursor_type: None,
        }],
        mutations:            vec![],
        subscriptions:        vec![],
        fragments:            None,
        directives:           None,
        fact_tables:          None,
        aggregate_queries:    None,
        observers:            None,
        custom_scalars:       None,
        observers_config:     None,
        subscriptions_config: None,
        validation_config:    None,
        federation_config:    None,
        debug_config:         None,
        mcp_config:           None,
        rest_config:          None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
    };

    let report = SchemaValidator::validate(&schema).unwrap();
    // Should only have the usual "no sql_source" warnings for other queries, not errors
    assert!(report.is_valid(), "Schema-qualified sql_source should be valid");
}

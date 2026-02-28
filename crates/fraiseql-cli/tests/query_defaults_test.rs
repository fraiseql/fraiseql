//! Integration tests for `[query_defaults]` in `fraiseql.toml`.
//!
//! Verifies the three-tier priority chain:
//!   hardcoded fallback (all-true) < `[query_defaults]` TOML < per-query `auto_params`

use fraiseql_cli::schema::{
    SchemaConverter,
    intermediate::{
        IntermediateAutoParams, IntermediateQuery, IntermediateQueryDefaults, IntermediateSchema,
        IntermediateType,
    },
};

// =============================================================================
// Helper
// =============================================================================

fn base_schema_with_query(query: IntermediateQuery, query_defaults: Option<IntermediateQueryDefaults>) -> IntermediateSchema {
    IntermediateSchema {
        version:           "2.0.0".to_string(),
        types:             vec![IntermediateType {
            name:        query.return_type.clone(),
            fields:      vec![],
            description: None,
            implements:  vec![],
            is_error:    false,
            relay:       false,
        }],
        enums:             vec![],
        input_types:       vec![],
        interfaces:        vec![],
        unions:            vec![],
        queries:           vec![query],
        mutations:         vec![],
        subscriptions:     vec![],
        fragments:         None,
        directives:        None,
        fact_tables:       None,
        aggregate_queries: None,
        observers:         None,
        custom_scalars:    None,
        security:          None,
        observers_config:  None,
        federation_config: None,
        query_defaults,
    }
}

fn list_query(name: &str, auto_params: Option<IntermediateAutoParams>) -> IntermediateQuery {
    IntermediateQuery {
        name:         name.to_string(),
        return_type:  "Item".to_string(),
        returns_list: true,
        nullable:     false,
        arguments:    vec![],
        description:  None,
        sql_source:   Some("v_item".to_string()),
        auto_params,
        deprecated:   None,
        jsonb_column: None,
        relay:        false,
        inject:       indexmap::IndexMap::default(),
        cache_ttl_seconds: None,
    }
}

fn single_query(name: &str) -> IntermediateQuery {
    IntermediateQuery {
        name:         name.to_string(),
        return_type:  "Item".to_string(),
        returns_list: false,
        nullable:     true,
        arguments:    vec![],
        description:  None,
        sql_source:   Some("v_item".to_string()),
        auto_params:  None,
        deprecated:   None,
        jsonb_column: None,
        relay:        false,
        inject:       indexmap::IndexMap::default(),
        cache_ttl_seconds: None,
    }
}

fn relay_query(name: &str) -> IntermediateQuery {
    IntermediateQuery {
        name:         name.to_string(),
        return_type:  "Item".to_string(),
        returns_list: true,
        nullable:     false,
        arguments:    vec![],
        description:  None,
        sql_source:   Some("v_item".to_string()),
        auto_params:  None,
        deprecated:   None,
        jsonb_column: None,
        relay:        true,
        inject:       indexmap::IndexMap::default(),
        cache_ttl_seconds: None,
    }
}

// =============================================================================
// Unit tests — resolve_auto_params logic (via SchemaConverter::convert)
// =============================================================================

#[test]
fn test_no_toml_defaults_list_query_all_true() {
    // No query_defaults → all-true fallback (historical behaviour preserved)
    let schema = base_schema_with_query(list_query("items", None), None);
    let compiled = SchemaConverter::convert(schema).unwrap();
    let params = &compiled.queries[0].auto_params;
    assert!(params.has_where,    "where should default to true");
    assert!(params.has_order_by, "order_by should default to true");
    assert!(params.has_limit,    "limit should default to true");
    assert!(params.has_offset,   "offset should default to true");
}

#[test]
fn test_toml_defaults_applied_to_list_query() {
    // TOML: {where:false, limit:false}; no per-query override
    let defaults = IntermediateQueryDefaults {
        where_clause: false,
        order_by:     true,
        limit:        false,
        offset:       true,
    };
    let schema = base_schema_with_query(list_query("items", None), Some(defaults));
    let compiled = SchemaConverter::convert(schema).unwrap();
    let params = &compiled.queries[0].auto_params;
    assert!(!params.has_where,    "where should come from TOML default (false)");
    assert!(params.has_order_by,  "order_by should come from TOML default (true)");
    assert!(!params.has_limit,    "limit should come from TOML default (false)");
    assert!(params.has_offset,    "offset should come from TOML default (true)");
}

#[test]
fn test_per_query_partial_overrides_toml() {
    // TOML: {where:false, limit:false, order_by:true, offset:true}
    // Per-query: {where: Some(true)} — only `where` is overridden
    let defaults = IntermediateQueryDefaults {
        where_clause: false,
        order_by:     true,
        limit:        false,
        offset:       true,
    };
    let per_query = IntermediateAutoParams {
        where_clause: Some(true),
        order_by:     None,
        limit:        None,
        offset:       None,
    };
    let schema = base_schema_with_query(list_query("items", Some(per_query)), Some(defaults));
    let compiled = SchemaConverter::convert(schema).unwrap();
    let params = &compiled.queries[0].auto_params;
    assert!(params.has_where,    "per-query where=true should win over TOML false");
    assert!(params.has_order_by, "order_by inherits TOML true");
    assert!(!params.has_limit,   "limit inherits TOML false");
    assert!(params.has_offset,   "offset inherits TOML true");
}

#[test]
fn test_per_query_full_override_ignores_toml() {
    // All 4 flags explicitly set per-query → TOML completely bypassed
    let defaults = IntermediateQueryDefaults {
        where_clause: false,
        order_by:     false,
        limit:        false,
        offset:       false,
    };
    let per_query = IntermediateAutoParams {
        where_clause: Some(true),
        order_by:     Some(true),
        limit:        Some(true),
        offset:       Some(true),
    };
    let schema = base_schema_with_query(list_query("items", Some(per_query)), Some(defaults));
    let compiled = SchemaConverter::convert(schema).unwrap();
    let params = &compiled.queries[0].auto_params;
    assert!(params.has_where,    "per-query overrides TOML false");
    assert!(params.has_order_by, "per-query overrides TOML false");
    assert!(params.has_limit,    "per-query overrides TOML false");
    assert!(params.has_offset,   "per-query overrides TOML false");
}

#[test]
fn test_single_item_always_none_regardless_of_toml() {
    // Single-item query with TOML defaults all-true → still all-false
    let defaults = IntermediateQueryDefaults {
        where_clause: true,
        order_by:     true,
        limit:        true,
        offset:       true,
    };
    let schema = base_schema_with_query(single_query("item"), Some(defaults));
    let compiled = SchemaConverter::convert(schema).unwrap();
    let params = &compiled.queries[0].auto_params;
    assert!(!params.has_where,    "single-item: where always false");
    assert!(!params.has_order_by, "single-item: order_by always false");
    assert!(!params.has_limit,    "single-item: limit always false");
    assert!(!params.has_offset,   "single-item: offset always false");
}

#[test]
fn test_relay_hardcoded_regardless_of_toml() {
    // Relay query with TOML limit=true, offset=true → still limit:false, offset:false
    let defaults = IntermediateQueryDefaults {
        where_clause: false,
        order_by:     false,
        limit:        true,
        offset:       true,
    };
    let schema = base_schema_with_query(relay_query("itemsConnection"), Some(defaults));
    let compiled = SchemaConverter::convert(schema).unwrap();
    let params = &compiled.queries[0].auto_params;
    assert!(params.has_where,    "relay: where always true");
    assert!(params.has_order_by, "relay: order_by always true");
    assert!(!params.has_limit,   "relay: limit always false");
    assert!(!params.has_offset,  "relay: offset always false");
}

#[test]
fn test_empty_auto_params_dict_inherits_toml() {
    // Empty per_query (all None) → TOML defaults apply for every field
    let defaults = IntermediateQueryDefaults {
        where_clause: false,
        order_by:     false,
        limit:        false,
        offset:       false,
    };
    let per_query = IntermediateAutoParams {
        where_clause: None,
        order_by:     None,
        limit:        None,
        offset:       None,
    };
    let schema = base_schema_with_query(list_query("items", Some(per_query)), Some(defaults));
    let compiled = SchemaConverter::convert(schema).unwrap();
    let params = &compiled.queries[0].auto_params;
    assert!(!params.has_where,    "empty per_query → inherit TOML false");
    assert!(!params.has_order_by, "empty per_query → inherit TOML false");
    assert!(!params.has_limit,    "empty per_query → inherit TOML false");
    assert!(!params.has_offset,   "empty per_query → inherit TOML false");
}

// =============================================================================
// Cross-concern priority tests — explicit priority chain interactions
//
// These tests pin the three-tier resolution order:
//   1. Per-query Some(v)  — highest priority (explicit decorator flag)
//   2. Project defaults   — middle priority ([query_defaults] in fraiseql.toml)
//   3. Hardcoded all-true — lowest priority (no [query_defaults] section)
//
// Each test targets a specific cross-tier interaction so that regressions in
// the priority chain are caught with a minimal, named failing test.
// =============================================================================

#[test]
fn test_cross_concern_per_query_some_true_wins_over_project_false() {
    // Scenario: project disables where and limit; one query re-enables where
    // while inheriting the limit=false default from the project.
    let defaults = IntermediateQueryDefaults {
        where_clause: false,
        order_by:     false,
        limit:        false,
        offset:       true,
    };
    let per_query = IntermediateAutoParams {
        where_clause: Some(true), // explicit per-query enable
        order_by:     None,       // inherits project false
        limit:        None,       // inherits project false
        offset:       None,       // inherits project true
    };
    let schema = base_schema_with_query(list_query("items", Some(per_query)), Some(defaults));
    let compiled = SchemaConverter::convert(schema).unwrap();
    let params = &compiled.queries[0].auto_params;

    assert!(params.has_where,    "Some(true) per-query must win over project false");
    assert!(!params.has_order_by,"None per-query must inherit project false");
    assert!(!params.has_limit,   "None per-query must inherit project false");
    assert!(params.has_offset,   "None per-query must inherit project true");
}

#[test]
fn test_cross_concern_per_query_none_inherits_project_default() {
    // Scenario: per-query has no overrides at all (all None); project controls
    // the outcome for every field independently.
    let defaults = IntermediateQueryDefaults {
        where_clause: true,
        order_by:     false,
        limit:        true,
        offset:       false,
    };
    let per_query = IntermediateAutoParams {
        where_clause: None,
        order_by:     None,
        limit:        None,
        offset:       None,
    };
    let schema = base_schema_with_query(list_query("items", Some(per_query)), Some(defaults));
    let compiled = SchemaConverter::convert(schema).unwrap();
    let params = &compiled.queries[0].auto_params;

    assert!(params.has_where,    "None → inherits project true");
    assert!(!params.has_order_by,"None → inherits project false");
    assert!(params.has_limit,    "None → inherits project true");
    assert!(!params.has_offset,  "None → inherits project false");
}

#[test]
fn test_cross_concern_hardcoded_fallback_when_no_project_defaults() {
    // Scenario: no [query_defaults] in the compiled schema at all.
    // Every field must fall back to the hardcoded all-true default.
    let schema = base_schema_with_query(list_query("items", None), None);
    let compiled = SchemaConverter::convert(schema).unwrap();
    let params = &compiled.queries[0].auto_params;

    assert!(params.has_where,    "hardcoded fallback: where = true");
    assert!(params.has_order_by, "hardcoded fallback: order_by = true");
    assert!(params.has_limit,    "hardcoded fallback: limit = true");
    assert!(params.has_offset,   "hardcoded fallback: offset = true");
}

#[test]
fn test_cross_concern_per_query_some_false_wins_over_project_true() {
    // Scenario: project enables everything; a specific query disables some
    // fields with explicit Some(false) decorators.
    //
    // This is the most common production pattern: a security-sensitive query
    // opts out of automatic where/limit while inheriting the rest.
    let defaults = IntermediateQueryDefaults {
        where_clause: true,
        order_by:     true,
        limit:        true,
        offset:       true,
    };
    let per_query = IntermediateAutoParams {
        where_clause: Some(false), // explicit disable
        order_by:     Some(false), // explicit disable
        limit:        None,        // inherits project true
        offset:       None,        // inherits project true
    };
    let schema = base_schema_with_query(list_query("items", Some(per_query)), Some(defaults));
    let compiled = SchemaConverter::convert(schema).unwrap();
    let params = &compiled.queries[0].auto_params;

    assert!(!params.has_where,   "Some(false) per-query must win over project true");
    assert!(!params.has_order_by,"Some(false) per-query must win over project true");
    assert!(params.has_limit,    "None per-query must inherit project true");
    assert!(params.has_offset,   "None per-query must inherit project true");
}

// =============================================================================
// TOML parse tests — QueryDefaults struct
// =============================================================================

#[test]
fn test_parse_query_defaults_where_false() {
    use fraiseql_cli::config::TomlSchema;

    let toml = r#"
[schema]
name = "myapp"
version = "1.0.0"
database_target = "postgresql"

[query_defaults]
where = false
"#;
    let schema = TomlSchema::parse_toml(toml).expect("Failed to parse");
    assert!(!schema.query_defaults.where_clause, "where should be false");
    assert!(schema.query_defaults.order_by,      "order_by defaults to true");
    assert!(schema.query_defaults.limit,          "limit defaults to true");
    assert!(schema.query_defaults.offset,         "offset defaults to true");
}

#[test]
fn test_parse_no_query_defaults_gives_all_true() {
    use fraiseql_cli::config::TomlSchema;

    let toml = r#"
[schema]
name = "myapp"
version = "1.0.0"
database_target = "postgresql"
"#;
    let schema = TomlSchema::parse_toml(toml).expect("Failed to parse");
    assert!(schema.query_defaults.where_clause, "default where_clause");
    assert!(schema.query_defaults.order_by,     "default order_by");
    assert!(schema.query_defaults.limit,        "default limit");
    assert!(schema.query_defaults.offset,       "default offset");
}

#[test]
fn test_parse_partial_query_defaults() {
    use fraiseql_cli::config::TomlSchema;

    let toml = r#"
[schema]
name = "myapp"
version = "1.0.0"
database_target = "postgresql"

[query_defaults]
limit = false
"#;
    let schema = TomlSchema::parse_toml(toml).expect("Failed to parse");
    assert!(schema.query_defaults.where_clause, "where_clause still true");
    assert!(schema.query_defaults.order_by,     "order_by still true");
    assert!(!schema.query_defaults.limit,       "limit set to false");
    assert!(schema.query_defaults.offset,       "offset still true");
}

// =============================================================================
// Integration tests — end-to-end via SchemaMerger
// =============================================================================

#[test]
fn test_end_to_end_relay_first_defaults() -> anyhow::Result<()> {
    use std::fs;
    use tempfile::TempDir;
    use fraiseql_cli::schema::SchemaMerger;

    let temp = TempDir::new()?;

    // Relay-first project: limit=false, offset=false in [query_defaults]
    let toml = r#"
[schema]
name = "relay_app"
version = "1.0.0"
database_target = "postgresql"

[query_defaults]
limit  = false
offset = false
"#;
    let toml_path = temp.path().join("fraiseql.toml");
    fs::write(&toml_path, toml)?;

    // schema.json: list query + single query + relay query
    let schema_json = serde_json::json!({
        "types": [{"name": "Post", "fields": [], "sql_source": "v_post"}],
        "queries": [
            {
                "name": "posts",
                "return_type": "Post",
                "returns_list": true,
                "nullable": false,
                "arguments": [],
                "sql_source": "v_post"
            },
            {
                "name": "post",
                "return_type": "Post",
                "returns_list": false,
                "nullable": true,
                "arguments": [],
                "sql_source": "v_post"
            },
            {
                "name": "postsConnection",
                "return_type": "Post",
                "returns_list": true,
                "nullable": false,
                "arguments": [],
                "sql_source": "v_post",
                "relay": true
            }
        ],
        "mutations": []
    });
    let types_path = temp.path().join("schema.json");
    fs::write(&types_path, schema_json.to_string())?;

    let intermediate = SchemaMerger::merge_files(
        types_path.to_str().unwrap(),
        toml_path.to_str().unwrap(),
    )?;
    let compiled = SchemaConverter::convert(intermediate)?;

    // List query: inherits limit=false, offset=false from TOML
    let posts = compiled.queries.iter().find(|q| q.name == "posts").unwrap();
    assert!(posts.auto_params.has_where,    "list: where true (TOML default)");
    assert!(posts.auto_params.has_order_by, "list: order_by true (TOML default)");
    assert!(!posts.auto_params.has_limit,   "list: limit false (TOML default)");
    assert!(!posts.auto_params.has_offset,  "list: offset false (TOML default)");

    // Single-item query: always all-false
    let post = compiled.queries.iter().find(|q| q.name == "post").unwrap();
    assert!(!post.auto_params.has_where,    "single: always false");
    assert!(!post.auto_params.has_limit,    "single: always false");

    // Relay query: always {where:T, order_by:T, limit:F, offset:F}
    let conn = compiled.queries.iter().find(|q| q.name == "postsConnection").unwrap();
    assert!(conn.auto_params.has_where,    "relay: where=true");
    assert!(conn.auto_params.has_order_by, "relay: order_by=true");
    assert!(!conn.auto_params.has_limit,   "relay: limit=false");
    assert!(!conn.auto_params.has_offset,  "relay: offset=false");

    Ok(())
}

#[test]
fn test_end_to_end_partial_per_query_override() -> anyhow::Result<()> {
    use std::fs;
    use tempfile::TempDir;
    use fraiseql_cli::schema::SchemaMerger;

    let temp = TempDir::new()?;

    // Security-first defaults: where=false
    let toml = r#"
[schema]
name = "secure_app"
version = "1.0.0"
database_target = "postgresql"

[query_defaults]
where = false
"#;
    let toml_path = temp.path().join("fraiseql.toml");
    fs::write(&toml_path, toml)?;

    // One query inherits defaults (no auto_params), one explicitly re-enables where
    let schema_json = serde_json::json!({
        "types": [{"name": "Log", "fields": [], "sql_source": "v_log"}],
        "queries": [
            {
                "name": "public_logs",
                "return_type": "Log",
                "returns_list": true,
                "nullable": false,
                "arguments": [],
                "sql_source": "v_log"
            },
            {
                "name": "admin_logs",
                "return_type": "Log",
                "returns_list": true,
                "nullable": false,
                "arguments": [],
                "sql_source": "v_admin_log",
                "auto_params": {"where": true}
            }
        ],
        "mutations": []
    });
    let types_path = temp.path().join("schema.json");
    fs::write(&types_path, schema_json.to_string())?;

    let intermediate = SchemaMerger::merge_files(
        types_path.to_str().unwrap(),
        toml_path.to_str().unwrap(),
    )?;
    let compiled = SchemaConverter::convert(intermediate)?;

    // public_logs inherits where=false from TOML
    let public = compiled.queries.iter().find(|q| q.name == "public_logs").unwrap();
    assert!(!public.auto_params.has_where, "inherits TOML where=false");
    assert!(public.auto_params.has_limit,  "limit defaults to true");

    // admin_logs overrides where=true
    let admin = compiled.queries.iter().find(|q| q.name == "admin_logs").unwrap();
    assert!(admin.auto_params.has_where,  "per-query where=true wins over TOML false");
    assert!(admin.auto_params.has_limit,  "limit inherits TOML default (true)");

    Ok(())
}

#[test]
fn test_typo_guard_queries_defaults_key() -> anyhow::Result<()> {
    use std::fs;
    use tempfile::TempDir;
    use fraiseql_cli::schema::SchemaMerger;

    let temp = TempDir::new()?;

    // [queries.defaults] is a common typo for [query_defaults]
    let toml = r#"
[schema]
name = "myapp"
version = "1.0.0"
database_target = "postgresql"

[types.User]
sql_source = "v_user"

[queries.defaults]
return_type = "User"
sql_source = "v_users"
"#;
    let toml_path = temp.path().join("fraiseql.toml");
    fs::write(&toml_path, toml)?;

    let schema_json = serde_json::json!({
        "types": [],
        "queries": [],
        "mutations": []
    });
    let types_path = temp.path().join("schema.json");
    fs::write(&types_path, schema_json.to_string())?;

    let err = SchemaMerger::merge_files(
        types_path.to_str().unwrap(),
        toml_path.to_str().unwrap(),
    ).unwrap_err();

    let msg = err.to_string();
    assert!(msg.contains("defaults"), "error should mention 'defaults': {msg}");
    assert!(msg.contains("query_defaults"), "error should hint at [query_defaults]: {msg}");

    Ok(())
}

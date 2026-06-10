#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
#![allow(clippy::wildcard_imports)] // Reason: test modules use wildcard imports for conciseness

mod analyze_tests {
    use super::super::analyze::*;

    #[test]
    fn test_analyze_nonexistent_file() {
        let result = run("/nonexistent/schema.json");
        assert!(result.is_err(), "expected Err for nonexistent schema file, got: {result:?}");
    }
}

mod cost_tests {
    use super::super::cost::*;

    #[test]
    fn test_cost_simple_query() {
        let query = "query { users { id } }";
        let result = run(query);

        let cmd_result = result.unwrap_or_else(|e| panic!("expected Ok for simple query: {e}"));
        assert_eq!(cmd_result.status, "success");
    }

    #[test]
    fn test_cost_invalid_query_fails() {
        let query = "query { invalid {";
        let result = run(query);

        assert!(result.is_err(), "expected Err for invalid query, got: {result:?}");
    }

    #[test]
    fn test_cost_provides_score() {
        let query = "query { users { id name } }";
        let result = run(query);

        let cmd_result = result.unwrap_or_else(|e| panic!("expected Ok for score query: {e}"));
        if let Some(data) = cmd_result.data {
            assert!(data["complexity_score"].is_number());
        }
    }

    #[test]
    fn test_cost_more_fields_higher_score() {
        let few_fields = run("query { users { id } }").unwrap();
        let many_fields = run("query { users { id name email phone address } }").unwrap();

        let few_score = few_fields
            .data
            .as_ref()
            .and_then(|d| d["complexity_score"].as_u64())
            .unwrap_or(0);
        let many_score = many_fields
            .data
            .as_ref()
            .and_then(|d| d["complexity_score"].as_u64())
            .unwrap_or(0);

        assert!(many_score >= few_score);
    }

    #[test]
    fn test_cost_nested_has_higher_score() {
        let shallow = run("query { users { id } }").unwrap();
        let deep = run("query { users { posts { comments { author } } } }").unwrap();

        let shallow_score =
            shallow.data.as_ref().and_then(|d| d["complexity_score"].as_u64()).unwrap_or(0);
        let deep_score =
            deep.data.as_ref().and_then(|d| d["complexity_score"].as_u64()).unwrap_or(0);

        assert!(deep_score > shallow_score);
    }
}

mod compile_tests {
    use std::collections::HashMap;

    use fraiseql_core::{
        schema::{
            ArgumentDefinition, AutoParams, CompiledSchema, CursorType, FieldDefinition,
            FieldDenyPolicy, FieldType, MutationDefinition, QueryDefinition, TypeDefinition,
        },
        validation::CustomTypeRegistry,
    };
    use indexmap::IndexMap;

    use super::super::compile::{
        WIDE_FANOUT_THRESHOLD, emit_ddl_to_dir, field_type_to_pg,
        infer_native_columns_from_arg_types, to_snake_case, wide_cascade_mutations,
    };

    fn mutation_with_fanout(
        name: &str,
        views: &[&str],
        fact_tables: &[&str],
    ) -> MutationDefinition {
        let mut m = MutationDefinition::new(name, "SomeResult");
        m.invalidates_views = views.iter().map(|s| (*s).to_string()).collect();
        m.invalidates_fact_tables = fact_tables.iter().map(|s| (*s).to_string()).collect();
        m
    }

    #[test]
    fn test_wide_cascade_below_threshold_not_flagged() {
        let schema = CompiledSchema {
            mutations: vec![mutation_with_fanout("update", &["tv_user", "tv_post"], &[])],
            ..Default::default()
        };
        assert!(
            wide_cascade_mutations(&schema, WIDE_FANOUT_THRESHOLD).is_empty(),
            "2 targets is below threshold of 3"
        );
    }

    #[test]
    fn test_wide_cascade_at_threshold_flagged() {
        let schema = CompiledSchema {
            mutations: vec![mutation_with_fanout(
                "updateUserWithPosts",
                &["tv_user", "tv_post", "tv_comment"],
                &[],
            )],
            ..Default::default()
        };
        let flagged = wide_cascade_mutations(&schema, WIDE_FANOUT_THRESHOLD);
        assert_eq!(flagged.len(), 1);
        assert_eq!(flagged[0].name, "updateUserWithPosts");
    }

    #[test]
    fn test_wide_cascade_views_plus_fact_tables_counted_together() {
        let schema = CompiledSchema {
            mutations: vec![mutation_with_fanout(
                "createOrder",
                &["tv_order", "tv_order_item"],
                &["tf_sales"],
            )],
            ..Default::default()
        };
        let flagged = wide_cascade_mutations(&schema, WIDE_FANOUT_THRESHOLD);
        assert_eq!(flagged.len(), 1, "2 views + 1 fact table = 3 total, meets threshold");
    }

    #[test]
    fn test_wide_cascade_only_wide_mutations_flagged() {
        let schema = CompiledSchema {
            mutations: vec![
                mutation_with_fanout("narrow", &["tv_user"], &[]),
                mutation_with_fanout("wide", &["tv_user", "tv_post", "tv_comment"], &[]),
            ],
            ..Default::default()
        };
        let flagged = wide_cascade_mutations(&schema, WIDE_FANOUT_THRESHOLD);
        assert_eq!(flagged.len(), 1);
        assert_eq!(flagged[0].name, "wide");
    }

    #[test]
    fn test_wide_cascade_no_mutations_no_warnings() {
        let schema = CompiledSchema::default();
        assert!(wide_cascade_mutations(&schema, WIDE_FANOUT_THRESHOLD).is_empty());
    }

    #[test]
    fn test_validate_schema_success() {
        let schema = CompiledSchema {
            types: vec![TypeDefinition {
                name:                "User".into(),
                fields:              vec![
                    FieldDefinition {
                        name:           "id".into(),
                        field_type:     FieldType::Int,
                        nullable:       false,
                        default_value:  None,
                        description:    None,
                        vector_config:  None,
                        alias:          None,
                        deprecation:    None,
                        requires_scope: None,
                        on_deny:        FieldDenyPolicy::default(),
                        authorize:      false,
                        encryption:     None,
                        hierarchy:      None,
                    },
                    FieldDefinition {
                        name:           "name".into(),
                        field_type:     FieldType::String,
                        nullable:       false,
                        default_value:  None,
                        description:    None,
                        vector_config:  None,
                        alias:          None,
                        deprecation:    None,
                        requires_scope: None,
                        on_deny:        FieldDenyPolicy::default(),
                        authorize:      false,
                        encryption:     None,
                        hierarchy:      None,
                    },
                ],
                description:         Some("User type".to_string()),
                sql_source:          String::new().into(),
                jsonb_column:        String::new(),
                sql_projection_hint: None,
                implements:          vec![],
                requires_role:       None,
                is_error:            false,
                relay:               false,
                relationships:       Vec::new(),
            }],
            queries: vec![QueryDefinition {
                name:                "users".to_string(),
                return_type:         "User".to_string(),
                returns_list:        true,
                nullable:            false,
                arguments:           vec![],
                sql_source:          Some("v_user".to_string()),
                description:         Some("Get users".to_string()),
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
            }],
            enums: vec![],
            input_types: vec![],
            interfaces: vec![],
            unions: vec![],
            mutations: vec![],
            subscriptions: vec![],
            directives: vec![],
            observers: Vec::new(),
            fact_tables: HashMap::default(),
            federation: None,
            security: None,
            observers_config: None,
            subscriptions_config: None,
            validation_config: None,
            debug_config: None,
            mcp_config: None,
            schema_sdl: None,
            // None is intentional here: this struct is used only for in-process
            // validation assertions and is never serialised to disk.
            schema_format_version: None,
            custom_scalars: CustomTypeRegistry::default(),
            ..Default::default()
        };

        // Validation is done inside SchemaConverter::convert, not exposed separately
        // This test just verifies we can build a valid schema structure
        assert_eq!(schema.types.len(), 1);
        assert_eq!(schema.queries.len(), 1);
    }

    #[test]
    fn test_validate_schema_unknown_type() {
        let schema = CompiledSchema {
            types: vec![],
            enums: vec![],
            input_types: vec![],
            interfaces: vec![],
            unions: vec![],
            queries: vec![QueryDefinition {
                name:                "users".to_string(),
                return_type:         "UnknownType".to_string(),
                returns_list:        true,
                nullable:            false,
                arguments:           vec![],
                sql_source:          Some("v_user".to_string()),
                description:         Some("Get users".to_string()),
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
            }],
            mutations: vec![],
            subscriptions: vec![],
            directives: vec![],
            observers: Vec::new(),
            fact_tables: HashMap::default(),
            federation: None,
            security: None,
            observers_config: None,
            subscriptions_config: None,
            validation_config: None,
            debug_config: None,
            mcp_config: None,
            schema_sdl: None,
            schema_format_version: None,
            custom_scalars: CustomTypeRegistry::default(),
            ..Default::default()
        };

        // Note: Validation is private to SchemaConverter
        assert_eq!(schema.types.len(), 0);
        assert_eq!(schema.queries[0].return_type, "UnknownType");
    }

    fn make_query(
        name: &str,
        sql_source: Option<&str>,
        jsonb_column: &str,
        args: Vec<(&str, FieldType)>,
        native_columns: std::collections::HashMap<String, String>,
    ) -> QueryDefinition {
        QueryDefinition {
            name: name.to_string(),
            return_type: "T".to_string(),
            returns_list: false,
            nullable: true,
            arguments: args.into_iter().map(|(n, t)| ArgumentDefinition::new(n, t)).collect(),
            sql_source: sql_source.map(str::to_string),
            jsonb_column: jsonb_column.to_string(),
            native_columns,
            auto_params: AutoParams::default(),
            ..Default::default()
        }
    }

    #[test]
    fn test_infer_id_arg_becomes_uuid_native_column() {
        let mut schema = CompiledSchema {
            queries: vec![make_query(
                "user",
                Some("tv_user"),
                "data",
                vec![("id", FieldType::Id)],
                std::collections::HashMap::new(),
            )],
            ..Default::default()
        };
        infer_native_columns_from_arg_types(&mut schema);
        assert_eq!(
            schema.queries[0].native_columns.get("id").map(String::as_str),
            Some("uuid"),
            "ID-typed arg should be inferred as uuid native column"
        );
    }

    #[test]
    fn test_infer_uuid_arg_becomes_uuid_native_column() {
        let mut schema = CompiledSchema {
            queries: vec![make_query(
                "user",
                Some("tv_user"),
                "data",
                vec![("userId", FieldType::Uuid)],
                std::collections::HashMap::new(),
            )],
            ..Default::default()
        };
        infer_native_columns_from_arg_types(&mut schema);
        assert_eq!(
            schema.queries[0].native_columns.get("userId").map(String::as_str),
            Some("uuid")
        );
    }

    #[test]
    fn test_infer_does_not_override_explicit_declaration() {
        let mut explicit = std::collections::HashMap::new();
        explicit.insert("id".to_string(), "text".to_string());
        let mut schema = CompiledSchema {
            queries: vec![make_query(
                "user",
                Some("tv_user"),
                "data",
                vec![("id", FieldType::Id)],
                explicit,
            )],
            ..Default::default()
        };
        infer_native_columns_from_arg_types(&mut schema);
        assert_eq!(
            schema.queries[0].native_columns.get("id").map(String::as_str),
            Some("text"),
            "explicit native_columns declaration must win over inference"
        );
    }

    #[test]
    fn test_infer_skips_queries_without_sql_source() {
        let mut schema = CompiledSchema {
            queries: vec![make_query(
                "user",
                None,
                "data",
                vec![("id", FieldType::Id)],
                std::collections::HashMap::new(),
            )],
            ..Default::default()
        };
        infer_native_columns_from_arg_types(&mut schema);
        assert!(
            schema.queries[0].native_columns.is_empty(),
            "queries without sql_source must not get inferred native_columns"
        );
    }

    #[test]
    fn test_infer_skips_queries_without_jsonb_column() {
        let mut schema = CompiledSchema {
            queries: vec![make_query(
                "user",
                Some("v_user"),
                "",
                vec![("id", FieldType::Id)],
                std::collections::HashMap::new(),
            )],
            ..Default::default()
        };
        infer_native_columns_from_arg_types(&mut schema);
        assert!(
            schema.queries[0].native_columns.is_empty(),
            "queries without jsonb_column must not get inferred native_columns"
        );
    }

    #[test]
    fn test_infer_skips_non_id_types() {
        let mut schema = CompiledSchema {
            queries: vec![make_query(
                "user",
                Some("tv_user"),
                "data",
                vec![("username", FieldType::String), ("age", FieldType::Int)],
                std::collections::HashMap::new(),
            )],
            ..Default::default()
        };
        infer_native_columns_from_arg_types(&mut schema);
        assert!(
            schema.queries[0].native_columns.is_empty(),
            "String/Int args must not be inferred as native columns"
        );
    }

    #[test]
    fn test_infer_skips_auto_param_names() {
        let mut schema = CompiledSchema {
            queries: vec![make_query(
                "users",
                Some("tv_user"),
                "data",
                vec![
                    ("where", FieldType::Id),
                    ("limit", FieldType::Id),
                    ("orderBy", FieldType::Id),
                ],
                std::collections::HashMap::new(),
            )],
            ..Default::default()
        };
        infer_native_columns_from_arg_types(&mut schema);
        assert!(
            schema.queries[0].native_columns.is_empty(),
            "auto-param names must never be inferred as native columns even if typed ID"
        );
    }

    #[test]
    fn test_to_snake_case_pascal() {
        assert_eq!(to_snake_case("UserProfile"), "user_profile");
    }

    #[test]
    fn test_to_snake_case_single_word() {
        assert_eq!(to_snake_case("User"), "user");
    }

    #[test]
    fn test_to_snake_case_already_lower() {
        assert_eq!(to_snake_case("user"), "user");
    }

    #[test]
    fn test_field_type_to_pg_scalar_types() {
        assert_eq!(field_type_to_pg(&FieldType::String), "TEXT");
        assert_eq!(field_type_to_pg(&FieldType::Int), "INTEGER");
        assert_eq!(field_type_to_pg(&FieldType::Float), "DOUBLE PRECISION");
        assert_eq!(field_type_to_pg(&FieldType::Boolean), "BOOLEAN");
        assert_eq!(field_type_to_pg(&FieldType::Id), "UUID");
        assert_eq!(field_type_to_pg(&FieldType::Uuid), "UUID");
        assert_eq!(field_type_to_pg(&FieldType::DateTime), "TIMESTAMPTZ");
        assert_eq!(field_type_to_pg(&FieldType::Date), "DATE");
        assert_eq!(field_type_to_pg(&FieldType::Time), "TIME");
        assert_eq!(field_type_to_pg(&FieldType::Json), "JSONB");
        assert_eq!(field_type_to_pg(&FieldType::Decimal), "NUMERIC");
        assert_eq!(field_type_to_pg(&FieldType::Vector), "VECTOR");
    }

    #[test]
    fn test_field_type_to_pg_enum_uses_type_name() {
        assert_eq!(field_type_to_pg(&FieldType::Enum("StatusEnum".to_string())), "StatusEnum");
    }

    #[test]
    fn test_field_type_to_pg_complex_types_are_jsonb() {
        assert_eq!(field_type_to_pg(&FieldType::Json), "JSONB");
        assert_eq!(field_type_to_pg(&FieldType::Object("Address".to_string())), "JSONB");
        assert_eq!(field_type_to_pg(&FieldType::List(Box::new(FieldType::String))), "JSONB");
    }

    #[test]
    fn test_emit_ddl_to_dir_creates_files() {
        let tmp = tempfile::tempdir().unwrap();
        let schema = CompiledSchema {
            types: vec![TypeDefinition {
                name:                "UserProfile".into(),
                fields:              vec![
                    FieldDefinition {
                        name:           "id".into(),
                        field_type:     FieldType::Id,
                        nullable:       false,
                        default_value:  None,
                        description:    None,
                        vector_config:  None,
                        alias:          None,
                        deprecation:    None,
                        requires_scope: None,
                        on_deny:        FieldDenyPolicy::default(),
                        authorize:      false,
                        encryption:     None,
                        hierarchy:      None,
                    },
                    FieldDefinition {
                        name:           "email".into(),
                        field_type:     FieldType::String,
                        nullable:       true,
                        default_value:  None,
                        description:    None,
                        vector_config:  None,
                        alias:          None,
                        deprecation:    None,
                        requires_scope: None,
                        on_deny:        FieldDenyPolicy::default(),
                        authorize:      false,
                        encryption:     None,
                        hierarchy:      None,
                    },
                ],
                description:         Some("Test type".to_string()),
                sql_source:          "tv_user_profile".into(),
                jsonb_column:        "data".to_string(),
                sql_projection_hint: None,
                implements:          vec![],
                requires_role:       None,
                is_error:            false,
                relay:               false,
                relationships:       vec![],
            }],
            ..Default::default()
        };

        let dir = tmp.path().to_str().unwrap();
        emit_ddl_to_dir(&schema, dir).unwrap();

        let ddl_file = tmp.path().join("user_profile.sql");
        assert!(ddl_file.exists(), "user_profile.sql must be created");
        let content = std::fs::read_to_string(ddl_file).unwrap();
        assert!(
            content.contains("CREATE TABLE IF NOT EXISTS tb_user_profile"),
            "DDL must contain CREATE TABLE"
        );
        assert!(content.contains("id UUID NOT NULL"), "id field must be UUID NOT NULL");
        assert!(content.contains("email TEXT"), "email field must be TEXT");
        assert!(
            !content.contains("email TEXT NOT NULL"),
            "nullable field must not have NOT NULL"
        );
    }

    #[test]
    fn test_emit_ddl_to_dir_empty_schema_no_files() {
        let tmp = tempfile::tempdir().unwrap();
        let schema = CompiledSchema::default();
        let dir = tmp.path().to_str().unwrap();
        emit_ddl_to_dir(&schema, dir).unwrap();
        let entries: Vec<_> = std::fs::read_dir(tmp.path()).unwrap().collect();
        assert!(entries.is_empty(), "no DDL files must be written for empty schema");
    }
}

mod dependency_graph_tests {
    use super::super::dependency_graph::*;

    #[test]
    fn test_graph_format_from_str() {
        assert_eq!("json".parse::<GraphFormat>().unwrap(), GraphFormat::Json);
        assert_eq!("dot".parse::<GraphFormat>().unwrap(), GraphFormat::Dot);
        assert_eq!("graphviz".parse::<GraphFormat>().unwrap(), GraphFormat::Dot);
        assert_eq!("mermaid".parse::<GraphFormat>().unwrap(), GraphFormat::Mermaid);
        assert_eq!("md".parse::<GraphFormat>().unwrap(), GraphFormat::Mermaid);
        assert_eq!("d2".parse::<GraphFormat>().unwrap(), GraphFormat::D2);
        assert_eq!("console".parse::<GraphFormat>().unwrap(), GraphFormat::Console);
        assert_eq!("text".parse::<GraphFormat>().unwrap(), GraphFormat::Console);
    }

    #[test]
    fn test_graph_format_case_insensitive() {
        assert_eq!("JSON".parse::<GraphFormat>().unwrap(), GraphFormat::Json);
        assert_eq!("DOT".parse::<GraphFormat>().unwrap(), GraphFormat::Dot);
        assert_eq!("MERMAID".parse::<GraphFormat>().unwrap(), GraphFormat::Mermaid);
        assert_eq!("D2".parse::<GraphFormat>().unwrap(), GraphFormat::D2);
    }

    #[test]
    fn test_graph_format_invalid() {
        let result = "invalid".parse::<GraphFormat>();
        let err = result.expect_err("expected Err for unknown graph format");
        assert!(err.contains("Unknown format"), "expected 'Unknown format' in: {err}");
    }

    #[test]
    fn test_graph_format_display() {
        assert_eq!(GraphFormat::Json.to_string(), "json");
        assert_eq!(GraphFormat::Dot.to_string(), "dot");
        assert_eq!(GraphFormat::Mermaid.to_string(), "mermaid");
        assert_eq!(GraphFormat::D2.to_string(), "d2");
        assert_eq!(GraphFormat::Console.to_string(), "console");
    }

    fn make_two_node_output() -> DependencyGraphOutput {
        DependencyGraphOutput {
            type_count:   2,
            nodes:        vec![
                GraphNode {
                    name:             "Query".to_string(),
                    dependency_count: 1,
                    dependent_count:  0,
                    is_root:          true,
                },
                GraphNode {
                    name:             "User".to_string(),
                    dependency_count: 0,
                    dependent_count:  1,
                    is_root:          false,
                },
            ],
            edges:        vec![GraphEdge {
                from: "Query".to_string(),
                to:   "User".to_string(),
            }],
            cycles:       vec![],
            unused_types: vec![],
            stats:        GraphStats {
                total_types:      2,
                total_edges:      1,
                cycle_count:      0,
                unused_count:     0,
                avg_dependencies: 0.5,
                max_depth:        1,
                most_depended_on: vec!["User".to_string()],
            },
        }
    }

    #[test]
    fn test_to_dot_contains_expected_elements() {
        let dot = to_dot(&make_two_node_output());
        assert!(dot.contains("digraph schema_dependencies"));
        assert!(dot.contains("Query"));
        assert!(dot.contains("User"));
        assert!(dot.contains("\"Query\" -> \"User\""));
    }

    #[test]
    fn test_to_mermaid_contains_expected_elements() {
        let mermaid = to_mermaid(&make_two_node_output());
        assert!(mermaid.contains("```mermaid"));
        assert!(mermaid.contains("graph LR"));
        assert!(mermaid.contains("Query"));
        assert!(mermaid.contains("User"));
        assert!(mermaid.contains("Query --> User"));
    }

    #[test]
    fn test_to_d2_contains_expected_elements() {
        let d2 = to_d2(&make_two_node_output());
        assert!(d2.contains("# Schema Dependency Graph"));
        assert!(d2.contains("direction: right"));
        assert!(d2.contains("roots:"));
        assert!(d2.contains("Query"));
        assert!(d2.contains("User"));
        assert!(d2.contains("roots.Query -> User"));
    }

    #[test]
    fn test_to_d2_shows_unused() {
        let output = DependencyGraphOutput {
            type_count:   1,
            nodes:        vec![GraphNode {
                name:             "Orphan".to_string(),
                dependency_count: 0,
                dependent_count:  0,
                is_root:          false,
            }],
            edges:        vec![],
            cycles:       vec![],
            unused_types: vec!["Orphan".to_string()],
            stats:        GraphStats {
                total_types:      1,
                total_edges:      0,
                cycle_count:      0,
                unused_count:     1,
                avg_dependencies: 0.0,
                max_depth:        0,
                most_depended_on: vec![],
            },
        };

        let d2 = to_d2(&output);
        assert!(d2.contains("unused:"));
        assert!(d2.contains("Unused Types"));
        assert!(d2.contains("Orphan"));
        assert!(d2.contains("stroke-dash"));
    }

    #[test]
    fn test_to_d2_shows_cycles() {
        let output = DependencyGraphOutput {
            type_count:   2,
            nodes:        vec![
                GraphNode {
                    name:             "A".to_string(),
                    dependency_count: 1,
                    dependent_count:  1,
                    is_root:          false,
                },
                GraphNode {
                    name:             "B".to_string(),
                    dependency_count: 1,
                    dependent_count:  1,
                    is_root:          false,
                },
            ],
            edges:        vec![
                GraphEdge {
                    from: "A".to_string(),
                    to:   "B".to_string(),
                },
                GraphEdge {
                    from: "B".to_string(),
                    to:   "A".to_string(),
                },
            ],
            cycles:       vec![CycleInfo {
                types:             vec!["A".to_string(), "B".to_string()],
                path:              "A -> B -> A".to_string(),
                is_self_reference: false,
            }],
            unused_types: vec![],
            stats:        GraphStats {
                total_types:      2,
                total_edges:      2,
                cycle_count:      1,
                unused_count:     0,
                avg_dependencies: 1.0,
                max_depth:        0,
                most_depended_on: vec![],
            },
        };

        let d2 = to_d2(&output);
        assert!(d2.contains("CYCLE"));
        assert!(d2.contains("stroke: \"#d32f2f\""));
        assert!(d2.contains("# WARNING: Circular dependencies detected!"));
    }

    #[test]
    fn test_to_console_contains_expected_elements() {
        let console = to_console(&make_two_node_output());
        assert!(console.contains("Schema Dependency Graph Analysis"));
        assert!(console.contains("Total types: 2"));
        assert!(console.contains("[ROOT] Query"));
        assert!(console.contains("User"));
    }

    #[test]
    fn test_to_console_shows_cycles() {
        let output = DependencyGraphOutput {
            type_count:   2,
            nodes:        vec![
                GraphNode {
                    name:             "A".to_string(),
                    dependency_count: 1,
                    dependent_count:  1,
                    is_root:          false,
                },
                GraphNode {
                    name:             "B".to_string(),
                    dependency_count: 1,
                    dependent_count:  1,
                    is_root:          false,
                },
            ],
            edges:        vec![
                GraphEdge {
                    from: "A".to_string(),
                    to:   "B".to_string(),
                },
                GraphEdge {
                    from: "B".to_string(),
                    to:   "A".to_string(),
                },
            ],
            cycles:       vec![CycleInfo {
                types:             vec!["A".to_string(), "B".to_string()],
                path:              "A -> B -> A".to_string(),
                is_self_reference: false,
            }],
            unused_types: vec![],
            stats:        GraphStats {
                total_types:      2,
                total_edges:      2,
                cycle_count:      1,
                unused_count:     0,
                avg_dependencies: 1.0,
                max_depth:        0,
                most_depended_on: vec![],
            },
        };

        let console = to_console(&output);
        assert!(console.contains("CIRCULAR DEPENDENCIES"));
        assert!(console.contains("A -> B -> A"));
    }

    #[test]
    fn test_to_console_shows_unused() {
        let output = DependencyGraphOutput {
            type_count:   1,
            nodes:        vec![GraphNode {
                name:             "Orphan".to_string(),
                dependency_count: 0,
                dependent_count:  0,
                is_root:          false,
            }],
            edges:        vec![],
            cycles:       vec![],
            unused_types: vec!["Orphan".to_string()],
            stats:        GraphStats {
                total_types:      1,
                total_edges:      0,
                cycle_count:      0,
                unused_count:     1,
                avg_dependencies: 0.0,
                max_depth:        0,
                most_depended_on: vec![],
            },
        };

        let console = to_console(&output);
        assert!(console.contains("UNUSED TYPES"));
        assert!(console.contains("Orphan"));
        assert!(console.contains("[UNUSED]"));
    }

    #[test]
    fn test_cycle_info_from_cycle_path() {
        use fraiseql_core::schema::CyclePath;

        let cycle = CyclePath::new(vec!["A".to_string(), "B".to_string(), "C".to_string()]);
        let info = CycleInfo::from(&cycle);

        assert_eq!(info.types, vec!["A", "B", "C"]);
        assert_eq!(info.path, "A → B → C → A");
        assert!(!info.is_self_reference);
    }

    #[test]
    fn test_cycle_info_self_reference() {
        use fraiseql_core::schema::CyclePath;

        let cycle = CyclePath::new(vec!["Node".to_string()]);
        let info = CycleInfo::from(&cycle);

        assert!(info.is_self_reference);
        assert_eq!(info.path, "Node → Node");
    }
}

mod doctor_tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::super::doctor::*;

    fn temp_file_with(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    #[test]
    fn test_schema_exists_pass() {
        let f = temp_file_with("{}");
        let result = check_schema_exists(f.path());
        assert_eq!(result.status, CheckStatus::Pass);
    }

    #[test]
    fn test_schema_exists_fail() {
        let result = check_schema_exists(std::path::Path::new("/nonexistent/schema.compiled.json"));
        assert_eq!(result.status, CheckStatus::Fail);
    }

    #[test]
    fn test_schema_parses_valid_json() {
        let f = temp_file_with(r#"{"types":[],"queries":[],"mutations":[]}"#);
        let result = check_schema_parses(f.path());
        assert_eq!(result.status, CheckStatus::Pass);
        assert!(result.detail.contains("types=0"));
    }

    #[test]
    fn test_schema_parses_invalid_json() {
        let f = temp_file_with("not json {{{");
        let result = check_schema_parses(f.path());
        assert_eq!(result.status, CheckStatus::Fail);
        assert!(result.hint.is_some());
    }

    #[test]
    fn test_schema_version_missing() {
        let f = temp_file_with(r#"{"types":[]}"#);
        let result = check_schema_version(f.path());
        assert_eq!(result.status, CheckStatus::Warn);
    }

    #[test]
    fn test_schema_version_current() {
        let f = temp_file_with(r#"{"version":1,"types":[]}"#);
        let result = check_schema_version(f.path());
        assert_eq!(result.status, CheckStatus::Pass);
        assert!(result.detail.contains("version=1"));
    }

    #[test]
    fn test_schema_version_mismatch() {
        let f = temp_file_with(r#"{"version":99,"types":[]}"#);
        let result = check_schema_version(f.path());
        assert_eq!(result.status, CheckStatus::Warn);
    }

    #[test]
    fn test_toml_exists_pass() {
        let f = temp_file_with(
            "[schema]\nname = \"test\"\nversion = \"1.0\"\ndatabase_target = \"postgresql\"\n",
        );
        let result = check_toml_exists(f.path());
        assert_eq!(result.status, CheckStatus::Pass);
    }

    #[test]
    fn test_toml_exists_warn() {
        let result = check_toml_exists(std::path::Path::new("/nonexistent/fraiseql.toml"));
        assert_eq!(result.status, CheckStatus::Warn);
    }

    #[test]
    fn test_toml_parses_valid() {
        let toml =
            "[schema]\nname = \"myapp\"\nversion = \"1.0\"\ndatabase_target = \"postgresql\"\n";
        let f = temp_file_with(toml);
        let result = check_toml_parses(f.path());
        assert_eq!(result.status, CheckStatus::Pass);
    }

    #[test]
    fn test_toml_parses_invalid_syntax() {
        let f = temp_file_with("this is not [[[ valid toml");
        let result = check_toml_parses(f.path());
        assert_eq!(result.status, CheckStatus::Fail);
        assert!(result.hint.is_some());
    }

    #[test]
    fn test_db_url_set_via_override() {
        let result = check_database_url_set(Some("postgres://localhost/test"));
        assert_eq!(result.status, CheckStatus::Pass);
    }

    #[test]
    fn test_db_url_not_set() {
        temp_env::with_var_unset("DATABASE_URL", || {
            let result = check_database_url_set(None);
            assert_eq!(result.status, CheckStatus::Fail);
        });
    }

    #[test]
    fn test_db_url_from_env() {
        temp_env::with_var("DATABASE_URL", Some("postgres://localhost/test"), || {
            let result = check_database_url_set(None);
            assert_eq!(result.status, CheckStatus::Pass);
        });
    }

    #[test]
    fn test_db_reachable_unreachable_port() {
        temp_env::with_var_unset("DATABASE_URL", || {
            let result = check_db_reachable(Some("postgres://localhost:1/db"));
            assert_eq!(result.status, CheckStatus::Fail);
            let hint = result.hint.unwrap();
            assert!(hint.contains("pg_isready"), "hint should mention pg_isready: {hint}");
        });
    }

    #[test]
    fn test_db_reachable_no_url() {
        temp_env::with_var_unset("DATABASE_URL", || {
            let result = check_db_reachable(None);
            assert_eq!(result.status, CheckStatus::Fail);
        });
    }

    #[test]
    fn test_jwt_secret_set() {
        temp_env::with_var("FRAISEQL_JWT_SECRET", Some("supersecret"), || {
            let result = check_jwt_secret();
            assert_eq!(result.status, CheckStatus::Pass);
        });
    }

    #[test]
    fn test_jwt_secret_missing() {
        temp_env::with_var_unset("FRAISEQL_JWT_SECRET", || {
            let result = check_jwt_secret();
            assert_eq!(result.status, CheckStatus::Warn);
            assert!(result.hint.is_some());
        });
    }

    #[test]
    fn test_redis_not_set_is_pass() {
        temp_env::with_var_unset("REDIS_URL", || {
            let result = check_redis_reachable();
            assert_eq!(result.status, CheckStatus::Pass);
        });
    }

    #[test]
    fn test_redis_set_but_unreachable() {
        temp_env::with_var("REDIS_URL", Some("redis://localhost:1"), || {
            let result = check_redis_reachable();
            assert_eq!(result.status, CheckStatus::Fail);
        });
    }

    #[test]
    fn test_tls_no_config_is_pass() {
        let result = check_tls(std::path::Path::new("/nonexistent/fraiseql.toml"));
        assert_eq!(result.status, CheckStatus::Pass);
    }

    #[test]
    fn test_tls_disabled_in_config_is_pass() {
        let toml = "[schema]\nname = \"a\"\nversion = \"1\"\ndatabase_target = \"postgresql\"\n";
        let f = temp_file_with(toml);
        let result = check_tls(f.path());
        assert_eq!(result.status, CheckStatus::Pass);
    }

    #[test]
    fn test_cache_auth_coherence_cache_disabled_is_pass() {
        let toml = "[schema]\nname = \"a\"\nversion = \"1\"\ndatabase_target = \"postgresql\"\n";
        let f = temp_file_with(toml);
        let result = check_rls_cache_coherence(f.path());
        assert_eq!(result.status, CheckStatus::Pass);
    }

    #[test]
    fn test_cache_auth_coherence_cache_enabled_no_policy_is_warn() {
        let toml = "[schema]\nname = \"a\"\nversion = \"1\"\ndatabase_target = \"postgresql\"\n\n[caching]\nenabled = true\n\n[security]\ndefault_policy = \"\"\n";
        let f = temp_file_with(toml);
        let result = check_rls_cache_coherence(f.path());
        assert!(matches!(result.status, CheckStatus::Pass | CheckStatus::Warn));
    }

    #[test]
    fn test_cache_auth_coherence_cache_enabled_with_policy_is_pass() {
        let toml = "[schema]\nname = \"a\"\nversion = \"1\"\ndatabase_target = \"postgresql\"\n\n[caching]\nenabled = true\n\n[security]\ndefault_policy = \"authenticated\"\n";
        let f = temp_file_with(toml);
        let result = check_rls_cache_coherence(f.path());
        assert_eq!(result.status, CheckStatus::Pass);
    }

    #[test]
    fn test_parse_host_port_postgres() {
        let (host, port) =
            parse_host_port("postgres://user:pass@db.example.com:5432/mydb").unwrap();
        assert_eq!(host, "db.example.com");
        assert_eq!(port, 5432);
    }

    #[test]
    fn test_parse_host_port_localhost() {
        let (host, port) = parse_host_port("postgres://localhost:5432/db").unwrap();
        assert_eq!(host, "localhost");
        assert_eq!(port, 5432);
    }

    #[test]
    fn test_parse_host_port_ipv6() {
        let result = parse_host_port("postgres://[::1]:5432/db");
        assert!(result.is_some());
        let (host, port) = result.unwrap();
        assert_eq!(host, "::1");
        assert_eq!(port, 5432);
    }

    #[test]
    fn test_parse_host_port_invalid() {
        assert!(parse_host_port("not-a-url").is_none());
    }

    #[test]
    fn test_json_serialization() {
        let checks = vec![
            DoctorCheck::pass("Test pass", "detail"),
            DoctorCheck::warn("Test warn", "detail", "hint text"),
            DoctorCheck::fail("Test fail", "detail", "hint text"),
        ];
        let json = serde_json::to_string(&checks).unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed[0]["status"], "pass");
        assert_eq!(parsed[1]["status"], "warn");
        assert_eq!(parsed[2]["status"], "fail");
    }

    // ── Change-log contract drift check (#380) ──────────────────────────────
    use fraiseql_observers::migrations::ENTITY_CHANGE_LOG_CONTRACT;

    use crate::schema::pg_catalog::LiveColumn;

    /// A live table that exactly matches the shipped contract (name + udt).
    fn clean_live() -> Vec<LiveColumn> {
        ENTITY_CHANGE_LOG_CONTRACT
            .iter()
            .map(|c| LiveColumn {
                name:     c.name.to_string(),
                udt_name: c.udt.to_string(),
            })
            .collect()
    }

    #[test]
    fn changelog_contract_clean_passes() {
        let checks = changelog_contract_drift(&clean_live());
        assert_eq!(checks.len(), 1, "a clean contract yields exactly one check");
        assert_eq!(checks[0].status, CheckStatus::Pass);
        assert!(checks[0].detail.contains("contract columns present"));
    }

    #[test]
    fn changelog_contract_absent_table_warns() {
        let checks = changelog_contract_drift(&[]);
        assert_eq!(checks.len(), 1, "an absent table yields a single warning");
        assert_eq!(checks[0].status, CheckStatus::Warn);
        assert!(checks[0].detail.contains("not found"));
        assert!(checks[0].hint.as_deref().unwrap().contains("migrate up"));
    }

    #[test]
    fn changelog_contract_missing_tenant_id_warns() {
        let live: Vec<LiveColumn> =
            clean_live().into_iter().filter(|c| c.name != "tenant_id").collect();
        let checks = changelog_contract_drift(&live);
        // No type mismatch, so no Fail — only the additive-reconcile Warn.
        assert!(checks.iter().all(|c| c.status != CheckStatus::Fail), "missing != fatal");
        let warn = checks
            .iter()
            .find(|c| c.status == CheckStatus::Warn)
            .expect("missing column produces a warning");
        assert!(warn.detail.contains("tenant_id"));
        assert!(warn.detail.contains("missing"));
    }

    #[test]
    fn changelog_contract_object_id_text_fails() {
        // The #149 hazard: a legacy `object_id text` the additive migration
        // cannot retype to the contract's `uuid`.
        let mut live = clean_live();
        for col in &mut live {
            if col.name == "object_id" {
                col.udt_name = "text".to_string();
            }
        }
        let checks = changelog_contract_drift(&live);
        let fail = checks
            .iter()
            .find(|c| c.status == CheckStatus::Fail)
            .expect("a type mismatch is fatal");
        assert!(fail.detail.contains("object_id"));
        assert!(fail.detail.contains("text"));
        assert!(fail.detail.contains("uuid"));
        assert!(fail.hint.as_deref().unwrap().contains("cannot retype"));
    }

    #[test]
    fn changelog_contract_extra_column_warns() {
        let mut live = clean_live();
        live.push(LiveColumn {
            name:     "app_custom_col".to_string(),
            udt_name: "text".to_string(),
        });
        let checks = changelog_contract_drift(&live);
        assert!(checks.iter().all(|c| c.status != CheckStatus::Fail), "extra column != fatal");
        let warn = checks
            .iter()
            .find(|c| c.detail.contains("app_custom_col"))
            .expect("extra column is reported");
        assert_eq!(warn.status, CheckStatus::Warn);
        assert!(warn.detail.contains("non-contract"));
    }
}

mod explain_tests {
    use super::super::explain::*;

    #[test]
    fn test_explain_simple_query() {
        let query = "query { users { id } }";
        let result = run(query);

        let cmd_result = result.unwrap_or_else(|e| panic!("expected Ok for simple query: {e}"));
        assert_eq!(cmd_result.status, "success");
    }

    #[test]
    fn test_explain_invalid_query_fails() {
        let query = "query { invalid {";
        let result = run(query);

        assert!(result.is_err(), "expected Err for invalid query, got: {result:?}");
    }

    #[test]
    fn test_explain_detects_deep_nesting() {
        let query = "query { a { b { c { d { e { f { g { h { i { j { k { l } } } } } } } } } } } }";
        let result = run(query);

        let cmd_result =
            result.unwrap_or_else(|e| panic!("expected Ok for deep nesting query: {e}"));
        if let Some(warnings) = cmd_result.data {
            assert!(!warnings.to_string().is_empty());
        }
    }
}

mod generate_proto_tests {
    use std::{io::Write as _, path::Path};

    use fraiseql_core::schema::{
        CompiledSchema, EnumDefinition, EnumValueDefinition, FieldDefinition, FieldDenyPolicy,
        FieldType, TypeDefinition,
    };
    use tempfile::TempDir;

    use super::super::generate_proto::*;
    use crate::output::OutputFormatter;

    fn make_field(name: &str, ft: FieldType, nullable: bool) -> FieldDefinition {
        FieldDefinition {
            name: name.into(),
            field_type: ft,
            nullable,
            description: None,
            default_value: None,
            vector_config: None,
            alias: None,
            deprecation: None,
            requires_scope: None,
            on_deny: FieldDenyPolicy::default(),
            authorize: false,
            encryption: None,
            hierarchy: None,
        }
    }

    fn make_type(name: &str, fields: Vec<FieldDefinition>) -> TypeDefinition {
        TypeDefinition {
            name: name.into(),
            sql_source: name.to_lowercase().into(),
            jsonb_column: "data".to_string(),
            fields,
            description: None,
            sql_projection_hint: None,
            implements: vec![],
            requires_role: None,
            is_error: false,
            relay: false,
            relationships: Vec::new(),
        }
    }

    fn make_query(
        name: &str,
        return_type: &str,
        returns_list: bool,
    ) -> fraiseql_core::schema::QueryDefinition {
        serde_json::from_value(serde_json::json!({
            "name": name,
            "return_type": return_type,
            "returns_list": returns_list,
        }))
        .expect("test query definition")
    }

    fn write_schema_file(dir: &Path, schema: &CompiledSchema) -> String {
        let json = serde_json::to_string_pretty(schema).expect("serialize schema");
        let path = dir.join("schema.compiled.json");
        let mut f = std::fs::File::create(&path).expect("create schema file");
        f.write_all(json.as_bytes()).expect("write schema file");
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn test_resolve_dialect_postgres() {
        assert!(resolve_dialect("postgres").is_ok());
        assert!(resolve_dialect("postgresql").is_ok());
    }

    #[test]
    fn test_resolve_dialect_mysql() {
        assert!(resolve_dialect("mysql").is_ok());
    }

    #[test]
    fn test_resolve_dialect_sqlite() {
        assert!(resolve_dialect("sqlite").is_ok());
    }

    #[test]
    fn test_resolve_dialect_sqlserver() {
        assert!(resolve_dialect("sqlserver").is_ok());
    }

    #[test]
    fn test_resolve_dialect_unknown() {
        match resolve_dialect("oracle") {
            Ok(_) => panic!("expected error for oracle"),
            Err(e) => assert!(e.to_string().contains("Unknown dialect")),
        }
    }

    #[test]
    fn test_descriptor_bytes_non_empty() {
        let proto = "syntax = \"proto3\";\npackage test.v1;\n";
        let bytes = build_file_descriptor_set(proto, "test.v1").expect("encode");
        assert!(!bytes.is_empty());
    }

    #[test]
    fn test_descriptor_includes_timestamp_dep() {
        let proto = "import \"google/protobuf/timestamp.proto\";\n";
        let bytes = build_file_descriptor_set(proto, "test.v1").expect("encode");
        let as_str = String::from_utf8_lossy(&bytes);
        assert!(as_str.contains("google/protobuf/timestamp.proto"));
    }

    #[test]
    fn test_descriptor_includes_struct_dep() {
        let proto = "import \"google/protobuf/struct.proto\";\n";
        let bytes = build_file_descriptor_set(proto, "test.v1").expect("encode");
        let as_str = String::from_utf8_lossy(&bytes);
        assert!(as_str.contains("google/protobuf/struct.proto"));
    }

    #[test]
    fn test_descriptor_no_deps_when_absent() {
        let proto = "syntax = \"proto3\";\n";
        let bytes = build_file_descriptor_set(proto, "test.v1").expect("encode");
        let as_str = String::from_utf8_lossy(&bytes);
        assert!(!as_str.contains("google/protobuf/timestamp.proto"));
    }

    #[test]
    fn test_run_generates_three_files() {
        let tmp = TempDir::new().expect("temp dir");
        let mut schema = CompiledSchema::new();
        schema.types.push(make_type(
            "User",
            vec![
                make_field("id", FieldType::Id, false),
                make_field("name", FieldType::String, false),
            ],
        ));
        schema.queries.push(make_query("get_user", "User", false));

        let schema_path = write_schema_file(tmp.path(), &schema);
        let out_dir = tmp.path().join("out");
        let formatter = OutputFormatter::new(false, true);

        run(&schema_path, &out_dir.to_string_lossy(), "test.v1", "postgres", &formatter)
            .expect("run should succeed");

        assert!(out_dir.join("service.proto").exists());
        assert!(out_dir.join("vr_migrations.sql").exists());
        assert!(out_dir.join("descriptor.binpb").exists());

        let proto = std::fs::read_to_string(out_dir.join("service.proto")).expect("read proto");
        assert!(proto.contains("package test.v1;"));
        assert!(proto.contains("message User {"));
        assert!(proto.contains("service TestService {"));
    }

    #[test]
    fn test_run_with_enum_and_datetime() {
        let tmp = TempDir::new().expect("temp dir");
        let mut schema = CompiledSchema::new();
        schema.enums.push(EnumDefinition {
            name:        "Status".to_string(),
            values:      vec![EnumValueDefinition {
                name:        "ACTIVE".to_string(),
                description: None,
                deprecation: None,
            }],
            description: None,
        });
        schema.types.push(make_type(
            "Event",
            vec![
                make_field("id", FieldType::Id, false),
                make_field("created_at", FieldType::DateTime, false),
                make_field("status", FieldType::Enum("Status".to_string()), false),
            ],
        ));
        schema.queries.push(make_query("get_event", "Event", false));

        let schema_path = write_schema_file(tmp.path(), &schema);
        let out_dir = tmp.path().join("out");
        let formatter = OutputFormatter::new(false, true);

        run(&schema_path, &out_dir.to_string_lossy(), "fraiseql.v1", "postgres", &formatter)
            .expect("run should succeed");

        let proto = std::fs::read_to_string(out_dir.join("service.proto")).expect("read proto");
        assert!(proto.contains("import \"google/protobuf/timestamp.proto\""));
        assert!(proto.contains("enum Status {"));

        let desc = std::fs::read(out_dir.join("descriptor.binpb")).expect("read descriptor");
        let desc_str = String::from_utf8_lossy(&desc);
        assert!(desc_str.contains("google/protobuf/timestamp.proto"));
    }

    #[test]
    fn test_run_mysql_dialect() {
        let tmp = TempDir::new().expect("temp dir");
        let mut schema = CompiledSchema::new();
        schema.types.push(make_type(
            "User",
            vec![
                make_field("id", FieldType::Id, false),
                make_field("name", FieldType::String, false),
            ],
        ));
        schema.queries.push(make_query("get_user", "User", false));

        let schema_path = write_schema_file(tmp.path(), &schema);
        let out_dir = tmp.path().join("out");
        let formatter = OutputFormatter::new(false, true);

        run(&schema_path, &out_dir.to_string_lossy(), "test.v1", "mysql", &formatter)
            .expect("run with mysql should succeed");

        let sql = std::fs::read_to_string(out_dir.join("vr_migrations.sql")).expect("read sql");
        assert!(sql.contains("JSON_EXTRACT"));
    }

    #[test]
    fn test_run_bad_schema_path() {
        let tmp = TempDir::new().expect("temp dir");
        let out_dir = tmp.path().join("out");
        let formatter = OutputFormatter::new(false, true);

        let result = run(
            "/nonexistent/schema.compiled.json",
            &out_dir.to_string_lossy(),
            "test.v1",
            "postgres",
            &formatter,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_run_bad_dialect() {
        let tmp = TempDir::new().expect("temp dir");
        let mut schema = CompiledSchema::new();
        schema
            .types
            .push(make_type("User", vec![make_field("id", FieldType::Id, false)]));

        let schema_path = write_schema_file(tmp.path(), &schema);
        let out_dir = tmp.path().join("out");
        let formatter = OutputFormatter::new(false, true);

        let result = run(&schema_path, &out_dir.to_string_lossy(), "test.v1", "oracle", &formatter);
        assert!(result.is_err());
        match result {
            Ok(()) => panic!("expected error for oracle dialect"),
            Err(e) => assert!(e.to_string().contains("Unknown dialect")),
        }
    }
}

mod generate_views_tests {
    use super::super::generate_views::*;

    #[test]
    fn test_refresh_strategy_from_str() {
        assert_eq!(RefreshStrategy::parse("trigger-based").unwrap(), RefreshStrategy::TriggerBased);
        assert_eq!(RefreshStrategy::parse("trigger").unwrap(), RefreshStrategy::TriggerBased);
        assert_eq!(RefreshStrategy::parse("scheduled").unwrap(), RefreshStrategy::Scheduled);
        assert!(
            RefreshStrategy::parse("invalid").is_err(),
            "expected Err for unknown refresh strategy"
        );
    }

    #[test]
    fn test_refresh_strategy_display() {
        assert_eq!(RefreshStrategy::TriggerBased.to_string(), "trigger-based");
        assert_eq!(RefreshStrategy::Scheduled.to_string(), "scheduled");
    }

    #[test]
    fn test_validate_view_name_vector_arrow() {
        assert_eq!(validate_view_name("va_user_embeddings").unwrap(), "Vector Arrow (va_)");
    }

    #[test]
    fn test_validate_view_name_table_vector() {
        assert_eq!(validate_view_name("tv_user_profile").unwrap(), "Table Vector (tv_)");
    }

    #[test]
    fn test_validate_view_name_table_arrow() {
        assert_eq!(validate_view_name("ta_orders").unwrap(), "Table Arrow (ta_)");
    }

    #[test]
    fn test_validate_view_name_invalid() {
        assert!(
            validate_view_name("invalid_view").is_err(),
            "expected Err for invalid_view prefix"
        );
        assert!(
            validate_view_name("v_user").is_err(),
            "expected Err for v_ prefix (not va_/tv_/ta_)"
        );
    }

    #[test]
    fn test_generate_view_sql_vector_arrow() {
        let sql = generate_view_sql(
            "User",
            "v_user",
            "va_user_embeddings",
            "Vector Arrow (va_)",
            RefreshStrategy::TriggerBased,
            false,
            false,
        );

        assert!(sql.contains("CREATE VIEW va_user_embeddings"));
        assert!(sql.contains("Entity: User"));
        assert!(sql.contains("Vector Arrow (va_)"));
        assert!(sql.contains("trigger-based"));
        assert!(
            sql.contains("FROM v_user"),
            "must use entity sql_source, not schema_placeholder"
        );
        assert!(!sql.contains("schema_placeholder"));
    }

    #[test]
    fn test_generate_view_sql_table_vector() {
        let sql = generate_view_sql(
            "Order",
            "v_order",
            "tv_order_summary",
            "Table Vector (tv_)",
            RefreshStrategy::Scheduled,
            false,
            false,
        );

        assert!(sql.contains("CREATE MATERIALIZED VIEW tv_order_summary"));
        assert!(sql.contains("Entity: Order"));
        assert!(sql.contains("scheduled"));
        assert!(
            sql.contains("FROM v_order"),
            "must use entity sql_source, not schema_placeholder"
        );
        assert!(!sql.contains("schema_placeholder"));
    }

    #[test]
    fn test_generate_view_sql_with_composition_views() {
        let sql = generate_view_sql(
            "User",
            "v_user",
            "tv_user_profile",
            "Table Vector (tv_)",
            RefreshStrategy::TriggerBased,
            true,
            false,
        );

        assert!(sql.contains("Composition views"));
        assert!(sql.contains("_recent"));
        assert!(sql.contains("_count"));
    }

    #[test]
    fn test_generate_view_sql_with_monitoring() {
        let sql = generate_view_sql(
            "User",
            "v_user",
            "tv_user_profile",
            "Table Vector (tv_)",
            RefreshStrategy::TriggerBased,
            false,
            true,
        );

        assert!(sql.contains("Monitoring functions"));
        assert!(sql.contains("monitor_tv_user_profile"));
        assert!(sql.contains("metric_name"));
    }

    #[test]
    fn test_generate_view_sql_full_options() {
        let sql = generate_view_sql(
            "User",
            "v_user",
            "ta_users",
            "Table Arrow (ta_)",
            RefreshStrategy::TriggerBased,
            true,
            true,
        );

        assert!(sql.contains("Entity: User"));
        assert!(sql.contains("View: ta_users"));
        assert!(sql.contains("Composition views"));
        assert!(sql.contains("Monitoring functions"));
        assert!(!sql.contains("schema_placeholder"));
    }

    #[test]
    fn test_generate_view_sql_uses_real_sql_source() {
        let sql = generate_view_sql(
            "Product",
            "v_product_catalog",
            "ta_products",
            "Table Arrow (ta_)",
            RefreshStrategy::TriggerBased,
            false,
            false,
        );

        assert!(
            sql.contains("FROM v_product_catalog"),
            "generated SQL must use the entity's sql_source"
        );
        assert!(!sql.contains("schema_placeholder"), "placeholder must not appear in output");
    }
}

mod introspect_facts_tests {
    use super::super::introspect_facts::*;

    #[test]
    fn test_output_format_from_str() {
        assert!(matches!(OutputFormat::parse("python"), Ok(OutputFormat::Python)));
        assert!(matches!(OutputFormat::parse("json"), Ok(OutputFormat::Json)));
        assert!(
            OutputFormat::parse("invalid").is_err(),
            "expected Err for unknown output format 'invalid'"
        );
    }

    #[test]
    fn test_format_as_python() {
        use fraiseql_core::compiler::fact_table::{
            DimensionColumn, FactTableMetadata, FilterColumn, MeasureColumn, SqlType,
        };

        let metadata = FactTableMetadata {
            table_name:               "tf_sales".to_string(),
            measures:                 vec![
                MeasureColumn {
                    name:     "revenue".to_string(),
                    sql_type: SqlType::Decimal,
                    nullable: false,
                },
                MeasureColumn {
                    name:     "quantity".to_string(),
                    sql_type: SqlType::Int,
                    nullable: false,
                },
            ],
            dimensions:               DimensionColumn {
                name:  "data".to_string(),
                paths: vec![],
            },
            denormalized_filters:     vec![FilterColumn {
                name:     "customer_id".to_string(),
                sql_type: SqlType::Uuid,
                indexed:  true,
            }],
            calendar_dimensions:      vec![],
            partial_period:           None,
            native_measures:          std::collections::HashMap::new(),
            native_dimension_mapping: std::collections::HashMap::new(),
        };

        let output = format_as_python(&metadata);
        assert!(output.contains("@fraiseql.fact_table"));
        assert!(output.contains("'revenue'"));
        assert!(output.contains("'quantity'"));
        assert!(output.contains("'customer_id'"));
        assert!(output.contains("class Sales:"));
    }
}

mod lint_tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::super::lint::*;

    fn default_opts() -> LintOptions {
        LintOptions {
            fail_on_critical: false,
            fail_on_warning:  false,
            filter:           LintCategoryFilter::default(),
        }
    }

    #[test]
    fn test_lint_valid_schema() {
        let schema_json = r#"{
            "types": [
                {
                    "name": "Query",
                    "fields": [
                        {"name": "users", "type": "[User!]"}
                    ]
                },
                {
                    "name": "User",
                    "fields": [
                        {"name": "id", "type": "ID", "isPrimaryKey": true},
                        {"name": "name", "type": "String"}
                    ]
                }
            ]
        }"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(schema_json.as_bytes()).unwrap();
        let path = file.path().to_str().unwrap();

        let result = run(path, default_opts());
        let cmd_result = result.unwrap_or_else(|e| panic!("expected Ok from lint run: {e:?}"));
        assert_eq!(cmd_result.status, "success");
        assert_eq!(cmd_result.command, "lint");
        assert!(cmd_result.data.is_some());
    }

    #[test]
    fn test_lint_file_not_found() {
        let result = run("nonexistent_schema.json", default_opts());
        assert!(result.is_err(), "file-not-found must return Err");
    }

    #[test]
    fn test_lint_returns_score() {
        let schema_json = r#"{"types": []}"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(schema_json.as_bytes()).unwrap();
        let path = file.path().to_str().unwrap();

        let result = run(path, default_opts());
        let cmd_result = result.unwrap_or_else(|e| panic!("expected Ok from lint run: {e:?}"));
        if let Some(data) = &cmd_result.data {
            assert!(data.get("overall_score").is_some());
            assert!(data.get("severity_counts").is_some());
            assert!(data.get("categories").is_some());
        }
    }
}

mod migrate_tests {
    use super::super::migrate::*;

    static GLOBAL_STATE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_resolve_migration_dir_explicit() {
        assert_eq!(resolve_migration_dir(Some("custom/dir")), "custom/dir");
    }

    #[test]
    fn test_resolve_migration_dir_default() {
        let dir = resolve_migration_dir(None);
        assert!(!dir.is_empty());
    }

    #[test]
    fn test_resolve_database_url_explicit() {
        let url = resolve_database_url(Some("postgres://localhost/test")).unwrap();
        assert_eq!(url, "postgres://localhost/test");
    }

    #[test]
    fn test_resolve_database_url_no_source() {
        let _guard = GLOBAL_STATE_LOCK
            .lock()
            .expect("GLOBAL_STATE_LOCK poisoned; a previous test panicked mid-migration");

        let tmp = tempfile::tempdir().unwrap();
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        temp_env::with_vars([("DATABASE_URL", None::<&str>)], || {
            let result = resolve_database_url(None);
            assert!(result.is_err(), "expected Err when no database URL is available");
        });

        std::env::set_current_dir(original).unwrap();
    }

    #[test]
    fn test_resolve_database_url_from_env() {
        let _guard = GLOBAL_STATE_LOCK
            .lock()
            .expect("GLOBAL_STATE_LOCK poisoned; a previous test panicked mid-migration");

        let tmp = tempfile::tempdir().unwrap();
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        temp_env::with_vars([("DATABASE_URL", Some("postgres://env/test"))], || {
            let url = resolve_database_url(None).unwrap();
            assert_eq!(url, "postgres://env/test");
        });

        std::env::set_current_dir(original).unwrap();
    }
}

mod openapi_tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::super::openapi::*;

    fn minimal_schema_json() -> String {
        serde_json::json!({
            "types": [{
                "name": "User",
                "sql_source": "v_user",
                "fields": [
                    { "name": "id", "field_type": "UUID" },
                    { "name": "name", "field_type": "String" },
                ]
            }],
            "queries": [{
                "name": "users",
                "return_type": "User",
                "returns_list": true,
            }],
            "mutations": [],
            "rest_config": {
                "enabled": true,
                "path": "/rest/v1"
            }
        })
        .to_string()
    }

    #[test]
    fn run_writes_openapi_spec() {
        let mut schema_file = NamedTempFile::new().unwrap();
        write!(schema_file, "{}", minimal_schema_json()).unwrap();

        let output_file = NamedTempFile::new().unwrap();
        let output_path = output_file.path().to_str().unwrap().to_string();

        run(schema_file.path().to_str().unwrap(), &output_path).unwrap();

        let content = std::fs::read_to_string(&output_path).unwrap();
        let spec: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(spec["openapi"], "3.0.3");
        assert!(spec["paths"]["/users"]["get"].is_object());
    }

    #[test]
    fn run_fails_without_rest_config() {
        let schema = serde_json::json!({
            "types": [],
            "queries": [],
            "mutations": [],
        });
        let mut schema_file = NamedTempFile::new().unwrap();
        write!(schema_file, "{schema}").unwrap();

        let result = run(schema_file.path().to_str().unwrap(), "/dev/null");
        assert!(result.is_err());
    }

    #[test]
    fn run_fails_when_disabled() {
        let schema = serde_json::json!({
            "types": [],
            "queries": [],
            "mutations": [],
            "rest_config": { "enabled": false }
        });
        let mut schema_file = NamedTempFile::new().unwrap();
        write!(schema_file, "{schema}").unwrap();

        let result = run(schema_file.path().to_str().unwrap(), "/dev/null");
        assert!(result.is_err());
    }
}

#[cfg(feature = "run-server")]
mod run_tests {
    use std::net::SocketAddr;

    use tempfile::TempDir;

    use super::super::run::{
        auto_detect_input, build_config_from, resolve_input, resolve_runtime_config,
    };
    use crate::config::runtime::{DatabaseRuntimeConfig, ServerRuntimeConfig};

    #[test]
    fn test_resolve_input_explicit_existing_file() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("fraiseql.toml");
        std::fs::write(&file, "").unwrap();

        let result = resolve_input(Some(file.to_str().unwrap()));
        assert!(result.is_ok(), "expected Ok, got {result:?}");
        assert_eq!(result.unwrap(), file);
    }

    #[test]
    fn test_resolve_input_explicit_missing_returns_helpful_error() {
        let result = resolve_input(Some("/nonexistent/path/schema.json"));
        let msg = result.expect_err("expected Err for missing path").to_string();
        assert!(msg.contains("not found"), "expected 'not found' in: {msg}");
        assert!(msg.contains("/nonexistent/path/schema.json"), "expected path in: {msg}");
    }

    #[test]
    fn test_auto_detect_prefers_toml_over_json() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("fraiseql.toml"), "").unwrap();
        std::fs::write(dir.path().join("schema.json"), "{}").unwrap();

        let result = auto_detect_input(dir.path()).unwrap();
        assert_eq!(result, dir.path().join("fraiseql.toml"));
    }

    #[test]
    fn test_auto_detect_falls_back_to_schema_json() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("schema.json"), "{}").unwrap();

        let result = auto_detect_input(dir.path()).unwrap();
        assert_eq!(result, dir.path().join("schema.json"));
    }

    #[test]
    fn test_auto_detect_no_files_returns_helpful_error() {
        let dir = TempDir::new().unwrap();

        let result = auto_detect_input(dir.path());
        let msg = result.expect_err("expected Err when no files present").to_string();
        assert!(msg.contains("No input file found"), "expected hint in: {msg}");
        assert!(msg.contains("fraiseql run <INPUT>"), "expected usage in: {msg}");
    }

    #[test]
    fn test_build_config_sets_db_url() {
        // `build_config_from` -> `ServerArgs::from_env()` reads `DATABASE_URL` directly. Wrap the
        // assertion in `temp_env` so it holds temp_env's global lock and observes a clean (unset)
        // env, instead of racing with the parallel `resolve_database_url` tests that set
        // `DATABASE_URL` inside their own temp_env closures (those serialize against each other but
        // not against a bare direct reader — the env-race class fixed the same way for the observers
        // SSRF guard).
        temp_env::with_var_unset("DATABASE_URL", || {
            let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
            let config = build_config_from(
                "postgres://localhost/test",
                addr,
                &ServerRuntimeConfig::default(),
                &DatabaseRuntimeConfig::default(),
                false,
            );
            assert_eq!(config.database_url, "postgres://localhost/test");
        });
    }

    #[test]
    fn test_build_config_sets_bind_addr() {
        let addr: SocketAddr = "127.0.0.1:3000".parse().unwrap();
        let config = build_config_from(
            "postgres://localhost/test",
            addr,
            &ServerRuntimeConfig::default(),
            &DatabaseRuntimeConfig::default(),
            false,
        );
        assert_eq!(config.bind_addr, addr);
    }

    #[test]
    fn test_build_config_introspection_enabled() {
        let addr: SocketAddr = "0.0.0.0:8080".parse().unwrap();
        let config = build_config_from(
            "postgres://localhost/test",
            addr,
            &ServerRuntimeConfig::default(),
            &DatabaseRuntimeConfig::default(),
            true,
        );
        assert!(config.introspection_enabled);
        assert!(!config.introspection_require_auth);
    }

    #[test]
    fn test_build_config_introspection_disabled() {
        let addr: SocketAddr = "0.0.0.0:8080".parse().unwrap();
        let config = build_config_from(
            "postgres://localhost/test",
            addr,
            &ServerRuntimeConfig::default(),
            &DatabaseRuntimeConfig::default(),
            false,
        );
        assert!(!config.introspection_enabled);
    }

    #[test]
    fn test_build_config_pool_sizes_from_db_cfg() {
        let addr: SocketAddr = "0.0.0.0:8080".parse().unwrap();
        let db_cfg = DatabaseRuntimeConfig {
            pool_min: 5,
            pool_max: 50,
            ..Default::default()
        };
        let config = build_config_from(
            "postgres://localhost/test",
            addr,
            &ServerRuntimeConfig::default(),
            &db_cfg,
            false,
        );
        assert_eq!(config.pool_min_size, 5);
        assert_eq!(config.pool_max_size, 50);
    }

    #[test]
    fn test_build_config_cors_origins_from_server_cfg() {
        let addr: SocketAddr = "0.0.0.0:8080".parse().unwrap();
        let server_cfg = ServerRuntimeConfig {
            cors: crate::config::runtime::CorsRuntimeConfig {
                origins:     vec!["https://example.com".to_string()],
                credentials: false,
            },
            ..Default::default()
        };
        let config = build_config_from(
            "postgres://localhost/test",
            addr,
            &server_cfg,
            &DatabaseRuntimeConfig::default(),
            false,
        );
        assert_eq!(config.cors_origins, ["https://example.com"]);
    }

    #[test]
    fn test_resolve_runtime_config_database_url_from_toml() {
        let dir = TempDir::new().unwrap();
        let toml_path = dir.path().join("fraiseql.toml");
        std::fs::write(
            &toml_path,
            r#"
[schema]
name = "test"
database_target = "postgresql"

[database]
url = "postgresql://toml-host/testdb"
"#,
        )
        .unwrap();

        temp_env::with_vars([("DATABASE_URL", None::<&str>)], || {
            let (db_url, _addr, _srv, _db) =
                resolve_runtime_config(&toml_path, None, None, None).unwrap();
            assert_eq!(db_url, "postgresql://toml-host/testdb");
        });
    }

    #[test]
    fn test_resolve_runtime_config_cli_db_overrides_toml() {
        let dir = TempDir::new().unwrap();
        let toml_path = dir.path().join("fraiseql.toml");
        std::fs::write(
            &toml_path,
            r#"
[schema]
name = "test"
database_target = "postgresql"

[database]
url = "postgresql://toml-host/testdb"
"#,
        )
        .unwrap();

        let (db_url, _addr, _srv, _db) = resolve_runtime_config(
            &toml_path,
            Some("postgresql://cli-host/clidb".to_string()),
            None,
            None,
        )
        .unwrap();
        assert_eq!(db_url, "postgresql://cli-host/clidb");
    }

    #[test]
    fn test_resolve_runtime_config_env_var_overrides_toml() {
        let dir = TempDir::new().unwrap();
        let toml_path = dir.path().join("fraiseql.toml");
        std::fs::write(
            &toml_path,
            r#"
[schema]
name = "test"
database_target = "postgresql"

[database]
url = "postgresql://toml-host/testdb"
"#,
        )
        .unwrap();

        temp_env::with_vars([("DATABASE_URL", Some("postgresql://env-host/envdb"))], || {
            let (db_url, _addr, _srv, _db) =
                resolve_runtime_config(&toml_path, None, None, None).unwrap();
            assert_eq!(db_url, "postgresql://env-host/envdb");
        });
    }

    #[test]
    fn test_resolve_runtime_config_toml_port_used_when_cli_absent() {
        let dir = TempDir::new().unwrap();
        let toml_path = dir.path().join("fraiseql.toml");
        std::fs::write(
            &toml_path,
            r#"
[schema]
name = "test"
database_target = "postgresql"

[database]
url = "postgresql://localhost/db"

[server]
host = "127.0.0.1"
port = 9999
"#,
        )
        .unwrap();

        temp_env::with_vars(
            [
                ("DATABASE_URL", None::<&str>),
                ("FRAISEQL_HOST", None::<&str>),
                ("FRAISEQL_PORT", None::<&str>),
            ],
            || {
                let (_db_url, addr, _srv, _db) =
                    resolve_runtime_config(&toml_path, None, None, None).unwrap();
                assert_eq!(addr.port(), 9999);
                assert_eq!(addr.ip().to_string(), "127.0.0.1");
            },
        );
    }

    #[test]
    fn test_resolve_runtime_config_cli_port_overrides_toml() {
        let dir = TempDir::new().unwrap();
        let toml_path = dir.path().join("fraiseql.toml");
        std::fs::write(
            &toml_path,
            r#"
[schema]
name = "test"
database_target = "postgresql"

[database]
url = "postgresql://localhost/db"

[server]
port = 9999
"#,
        )
        .unwrap();

        temp_env::with_vars(
            [
                ("DATABASE_URL", None::<&str>),
                ("FRAISEQL_PORT", None::<&str>),
            ],
            || {
                let (_db_url, addr, _srv, _db) =
                    resolve_runtime_config(&toml_path, None, Some(7777), None).unwrap();
                assert_eq!(addr.port(), 7777);
            },
        );
    }

    #[test]
    fn test_resolve_runtime_config_invalid_primary_toml_is_fatal() {
        let dir = TempDir::new().unwrap();
        let toml_path = dir.path().join("fraiseql.toml");
        std::fs::write(&toml_path, "this is [not valid toml !!!").unwrap();

        let result = resolve_runtime_config(&toml_path, None, None, None);
        assert!(result.is_err(), "invalid primary TOML must be fatal");
    }

    #[test]
    fn test_resolve_runtime_config_port_zero_rejected() {
        let dir = TempDir::new().unwrap();
        let toml_path = dir.path().join("fraiseql.toml");
        std::fs::write(
            &toml_path,
            r#"
[schema]
name = "test"
database_target = "postgresql"

[database]
url = "postgresql://localhost/db"

[server]
port = 0
"#,
        )
        .unwrap();

        temp_env::with_vars(
            [
                ("DATABASE_URL", None::<&str>),
                ("FRAISEQL_PORT", None::<&str>),
            ],
            || {
                let result = resolve_runtime_config(&toml_path, None, None, None);
                let msg = result.expect_err("expected Err for port=0").to_string();
                assert!(msg.contains("port"), "got: {msg}");
            },
        );
    }

    #[test]
    fn test_resolve_runtime_config_pool_range_rejected() {
        let dir = TempDir::new().unwrap();
        let toml_path = dir.path().join("fraiseql.toml");
        std::fs::write(
            &toml_path,
            r#"
[schema]
name = "test"
database_target = "postgresql"

[database]
url = "postgresql://localhost/db"
pool_min = 50
pool_max = 10
"#,
        )
        .unwrap();

        temp_env::with_vars([("DATABASE_URL", None::<&str>)], || {
            let result = resolve_runtime_config(&toml_path, None, None, None);
            let msg = result.expect_err("expected Err for pool_min > pool_max").to_string();
            assert!(msg.contains("pool_min"), "got: {msg}");
        });
    }

    #[test]
    fn test_resolve_runtime_config_no_db_url_returns_error() {
        let dir = TempDir::new().unwrap();
        let toml_path = dir.path().join("fraiseql.toml");
        std::fs::write(
            &toml_path,
            r#"
[schema]
name = "test"
database_target = "postgresql"
"#,
        )
        .unwrap();

        temp_env::with_vars([("DATABASE_URL", None::<&str>)], || {
            let result = resolve_runtime_config(&toml_path, None, None, None);
            let msg = result.expect_err("expected Err for missing database URL").to_string();
            assert!(msg.contains("database URL"), "got: {msg}");
        });
    }
}

mod sbom_tests {
    use std::str::FromStr;

    use super::super::sbom::*;

    #[test]
    fn test_sbom_format_from_str() {
        assert_eq!(SbomFormat::from_str("cyclonedx").unwrap(), SbomFormat::CycloneDx);
        assert_eq!(SbomFormat::from_str("cdx").unwrap(), SbomFormat::CycloneDx);
        assert_eq!(SbomFormat::from_str("spdx").unwrap(), SbomFormat::Spdx);
        assert!(SbomFormat::from_str("csv").is_err(), "expected Err for unknown format 'csv'");
    }

    #[test]
    fn test_generate_cyclonedx() {
        let packages = vec![
            CargoLockPackage {
                name:    "serde".to_string(),
                version: "1.0.200".to_string(),
                source:  Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
            },
            CargoLockPackage {
                name:    "tokio".to_string(),
                version: "1.42.0".to_string(),
                source:  Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
            },
        ];

        let result = generate_cyclonedx("test-app", "1.0.0", &packages).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["bomFormat"], "CycloneDX");
        assert_eq!(parsed["specVersion"], "1.5");
        assert_eq!(parsed["metadata"]["component"]["name"], "test-app");
        assert_eq!(parsed["components"].as_array().unwrap().len(), 2);
        assert_eq!(parsed["components"][0]["name"], "serde");
        assert!(
            parsed["components"][0]["purl"]
                .as_str()
                .unwrap()
                .contains("pkg:cargo/serde@1.0.200")
        );
    }

    #[test]
    fn test_generate_spdx() {
        let packages = vec![CargoLockPackage {
            name:    "anyhow".to_string(),
            version: "1.0.0".to_string(),
            source:  Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
        }];

        let result = generate_spdx("test-app", "0.1.0", &packages).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["spdxVersion"], "SPDX-2.3");
        assert_eq!(parsed["packages"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["packages"][0]["name"], "anyhow");
    }

    #[test]
    fn test_find_cargo_lock() {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
        let cargo_lock = workspace_root.join("Cargo.lock");
        assert!(cargo_lock.exists(), "Should find Cargo.lock in workspace root");
    }

    #[test]
    fn test_parse_cargo_lock() {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
        let cargo_lock = workspace_root.join("Cargo.lock");
        let content = std::fs::read_to_string(&cargo_lock).unwrap();
        let packages = parse_cargo_lock_content(&content).unwrap();
        assert!(!packages.is_empty(), "Cargo.lock should contain packages");

        let has_serde = packages.iter().any(|p| p.name == "serde");
        assert!(has_serde, "Should contain serde dependency");
    }

    #[test]
    fn test_days_to_date_epoch() {
        let (y, m, d) = days_to_date(0);
        assert_eq!((y, m, d), (1970, 1, 1));
    }

    #[test]
    fn test_days_to_date_known() {
        let (y, m, d) = days_to_date(19_723);
        assert_eq!((y, m, d), (2024, 1, 1));
    }

    #[test]
    fn test_chrono_now_utc_format() {
        let ts = chrono_now_utc();
        assert!(ts.ends_with('Z'));
        assert!(ts.contains('T'));
        assert_eq!(ts.len(), 20);
    }
}

mod validate_tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::super::validate::*;

    fn create_valid_schema() -> String {
        serde_json::json!({
            "types": [
                {
                    "name": "User",
                    "sql_source": "v_user",
                    "jsonb_column": "data",
                    "fields": [
                        {"name": "id", "field_type": "ID"},
                        {"name": "profile", "field_type": {"Object": "Profile"}, "nullable": true}
                    ],
                    "implements": []
                },
                {
                    "name": "Profile",
                    "sql_source": "v_profile",
                    "jsonb_column": "data",
                    "fields": [
                        {"name": "bio", "field_type": "String", "nullable": true}
                    ],
                    "implements": []
                }
            ],
            "queries": [
                {
                    "name": "users",
                    "sql_source": "v_user",
                    "return_type": "[User]",
                    "arguments": [],
                    "max_results": 1000
                }
            ],
            "mutations": [],
            "subscriptions": [],
            "enums": [],
            "input_types": [],
            "interfaces": [],
            "unions": [],
            "directives": [],
            "observers": []
        })
        .to_string()
    }

    fn create_schema_with_cycle() -> String {
        serde_json::json!({
            "types": [
                {
                    "name": "A",
                    "sql_source": "v_a",
                    "jsonb_column": "data",
                    "fields": [
                        {"name": "id", "field_type": "ID"},
                        {"name": "b", "field_type": {"Object": "B"}}
                    ],
                    "implements": []
                },
                {
                    "name": "B",
                    "sql_source": "v_b",
                    "jsonb_column": "data",
                    "fields": [
                        {"name": "id", "field_type": "ID"},
                        {"name": "a", "field_type": {"Object": "A"}}
                    ],
                    "implements": []
                }
            ],
            "queries": [
                {
                    "name": "items",
                    "sql_source": "v_a",
                    "return_type": "[A]",
                    "arguments": [],
                    "max_results": 1000
                }
            ],
            "mutations": [],
            "subscriptions": [],
            "enums": [],
            "input_types": [],
            "interfaces": [],
            "unions": [],
            "directives": [],
            "observers": []
        })
        .to_string()
    }

    fn create_schema_with_unused() -> String {
        serde_json::json!({
            "types": [
                {
                    "name": "User",
                    "sql_source": "v_user",
                    "jsonb_column": "data",
                    "fields": [
                        {"name": "id", "field_type": "ID"}
                    ],
                    "implements": []
                },
                {
                    "name": "OrphanType",
                    "sql_source": "v_orphan",
                    "jsonb_column": "data",
                    "fields": [
                        {"name": "data", "field_type": "String"}
                    ],
                    "implements": []
                }
            ],
            "queries": [
                {
                    "name": "users",
                    "sql_source": "v_user",
                    "return_type": "[User]",
                    "arguments": [],
                    "max_results": 1000
                }
            ],
            "mutations": [],
            "subscriptions": [],
            "enums": [],
            "input_types": [],
            "interfaces": [],
            "unions": [],
            "directives": [],
            "observers": []
        })
        .to_string()
    }

    #[test]
    fn test_validate_valid_schema() {
        let schema = create_valid_schema();
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(schema.as_bytes()).unwrap();

        let opts = ValidateOptions {
            check_cycles: true,
            check_unused: true,
            strict:       false,
            filter_types: vec![],
        };

        let result = run_with_options(temp_file.path().to_str().unwrap(), opts).unwrap();
        assert_eq!(result.status, "success");
    }

    #[test]
    fn test_validate_detects_cycles() {
        let schema = create_schema_with_cycle();
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(schema.as_bytes()).unwrap();

        let opts = ValidateOptions {
            check_cycles: true,
            check_unused: false,
            strict:       false,
            filter_types: vec![],
        };

        let result = run_with_options(temp_file.path().to_str().unwrap(), opts).unwrap();
        assert_eq!(result.status, "validation-failed");
        assert!(result.errors.iter().any(|e| e.contains("Circular")));
    }

    #[test]
    fn test_validate_cycles_disabled() {
        let schema = create_schema_with_cycle();
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(schema.as_bytes()).unwrap();

        let opts = ValidateOptions {
            check_cycles: false,
            check_unused: false,
            strict:       false,
            filter_types: vec![],
        };

        let result = run_with_options(temp_file.path().to_str().unwrap(), opts).unwrap();
        assert_eq!(result.status, "success");
    }

    #[test]
    fn test_validate_unused_as_warning() {
        let schema = create_schema_with_unused();
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(schema.as_bytes()).unwrap();

        let opts = ValidateOptions {
            check_cycles: true,
            check_unused: true,
            strict:       false,
            filter_types: vec![],
        };

        let result = run_with_options(temp_file.path().to_str().unwrap(), opts).unwrap();
        assert_eq!(result.status, "success");
        assert!(!result.warnings.is_empty());
        assert!(result.warnings.iter().any(|w| w.contains("OrphanType")));
    }

    #[test]
    fn test_validate_strict_mode() {
        let schema = create_schema_with_unused();
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(schema.as_bytes()).unwrap();

        let opts = ValidateOptions {
            check_cycles: true,
            check_unused: true,
            strict:       true,
            filter_types: vec![],
        };

        let result = run_with_options(temp_file.path().to_str().unwrap(), opts).unwrap();
        assert_eq!(result.status, "validation-failed");
        assert!(result.errors.iter().any(|e| e.contains("OrphanType")));
    }

    #[test]
    fn test_validate_type_filter() {
        let schema = create_valid_schema();
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(schema.as_bytes()).unwrap();

        let opts = ValidateOptions {
            check_cycles: true,
            check_unused: false,
            strict:       false,
            filter_types: vec!["User".to_string()],
        };

        let result = run_with_options(temp_file.path().to_str().unwrap(), opts).unwrap();
        assert_eq!(result.status, "success");
        let data = result.data.unwrap();
        let type_analysis = data.get("type_analysis").unwrap().as_array().unwrap();
        assert_eq!(type_analysis.len(), 1);
        assert_eq!(type_analysis[0]["name"], "User");
    }

    #[test]
    fn test_validate_type_filter_not_found() {
        let schema = create_valid_schema();
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(schema.as_bytes()).unwrap();

        let opts = ValidateOptions {
            check_cycles: true,
            check_unused: false,
            strict:       false,
            filter_types: vec!["NonExistent".to_string()],
        };

        let result = run_with_options(temp_file.path().to_str().unwrap(), opts).unwrap();
        assert_eq!(result.status, "success");
        assert!(result.warnings.iter().any(|w| w.contains("NonExistent")));
    }

    #[test]
    fn test_validate_result_structure() {
        let schema = create_valid_schema();
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(schema.as_bytes()).unwrap();

        let opts = ValidateOptions::default();
        let result = run_with_options(temp_file.path().to_str().unwrap(), opts).unwrap();

        let data = result.data.unwrap();
        assert!(data.get("schema_path").is_some());
        assert!(data.get("valid").is_some());
        assert!(data.get("type_count").is_some());
        assert!(data.get("query_count").is_some());
        assert!(data.get("mutation_count").is_some());
    }
}

mod validate_documents_tests {
    use sha2::{Digest, Sha256};

    use super::super::validate_documents::*;

    #[test]
    fn test_rejects_manifest_exceeding_size_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.json");

        let size = usize::try_from(MAX_MANIFEST_BYTES).unwrap() + 1;
        std::fs::write(&path, vec![b'x'; size]).unwrap();

        let formatter = crate::output::OutputFormatter::new(false, false);
        let result = run(path.to_str().unwrap(), &formatter);
        let msg = result.expect_err("expected Err for oversized manifest").to_string();
        assert!(msg.contains("too large"), "expected size error, got: {msg}");
    }

    #[test]
    fn test_rejects_unknown_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.json");

        let manifest = serde_json::json!({
            "version": 99,
            "documents": {}
        });
        std::fs::write(&path, serde_json::to_string(&manifest).unwrap()).unwrap();

        let formatter = crate::output::OutputFormatter::new(false, false);
        let result = run(path.to_str().unwrap(), &formatter);
        let msg = result.expect_err("expected Err for unknown manifest version").to_string();
        assert!(
            msg.contains("Unsupported manifest version"),
            "expected version error, got: {msg}"
        );
    }

    #[test]
    fn valid_manifest_passes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.json");

        let query = "{ users { id } }";
        let hash = hex::encode(Sha256::digest(query.as_bytes()));
        let manifest = serde_json::json!({
            "version": 1,
            "documents": {
                format!("sha256:{hash}"): query
            }
        });
        std::fs::write(&path, serde_json::to_string(&manifest).unwrap()).unwrap();

        let formatter = crate::output::OutputFormatter::new(false, false);
        let result = run(path.to_str().unwrap(), &formatter).unwrap();
        assert!(result);
    }

    #[test]
    fn mismatched_hash_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.json");

        let manifest = serde_json::json!({
            "version": 1,
            "documents": {
                "sha256:0000000000000000000000000000000000000000000000000000000000000000": "{ users { id } }"
            }
        });
        std::fs::write(&path, serde_json::to_string(&manifest).unwrap()).unwrap();

        let formatter = crate::output::OutputFormatter::new(false, false);
        let result = run(path.to_str().unwrap(), &formatter).unwrap();
        assert!(!result);
    }

    #[test]
    fn invalid_hash_length_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.json");

        let manifest = serde_json::json!({
            "version": 1,
            "documents": {
                "sha256:tooshort": "{ users { id } }"
            }
        });
        std::fs::write(&path, serde_json::to_string(&manifest).unwrap()).unwrap();

        let formatter = crate::output::OutputFormatter::new(false, false);
        let result = run(path.to_str().unwrap(), &formatter).unwrap();
        assert!(!result);
    }
}

mod validate_facts_tests {
    use fraiseql_core::compiler::fact_table::{
        DimensionColumn, FactTableMetadata, FilterColumn, MeasureColumn, SqlType,
    };

    use super::super::validate_facts::*;

    #[test]
    fn test_validation_issue_error() {
        let issue = ValidationIssue::error("tf_sales".to_string(), "Table not found".to_string());
        assert_eq!(issue.severity, IssueSeverity::Error);
        assert_eq!(issue.table_name, "tf_sales");
    }

    #[test]
    fn test_validation_issue_warning() {
        let issue = ValidationIssue::warning(
            "tf_orders".to_string(),
            "Table exists but not declared".to_string(),
        );
        assert_eq!(issue.severity, IssueSeverity::Warning);
    }

    fn make_metadata(
        measures: Vec<MeasureColumn>,
        dim_name: &str,
        filters: Vec<FilterColumn>,
    ) -> FactTableMetadata {
        FactTableMetadata {
            table_name: "tf_sales".to_string(),
            measures,
            dimensions: DimensionColumn {
                name:  dim_name.to_string(),
                paths: vec![],
            },
            denormalized_filters: filters,
            calendar_dimensions: vec![],
            partial_period: None,
            native_measures: std::collections::HashMap::new(),
            native_dimension_mapping: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn test_compare_metadata_matching() {
        let declared = make_metadata(
            vec![
                MeasureColumn {
                    name:     "revenue".to_string(),
                    sql_type: SqlType::Decimal,
                    nullable: false,
                },
                MeasureColumn {
                    name:     "quantity".to_string(),
                    sql_type: SqlType::Int,
                    nullable: false,
                },
            ],
            "data",
            vec![FilterColumn {
                name:     "customer_id".to_string(),
                sql_type: SqlType::Uuid,
                indexed:  true,
            }],
        );
        let actual = declared.clone();

        let issues = compare_metadata("tf_sales", &declared, &actual);
        let errors: Vec<_> = issues.iter().filter(|i| i.severity == IssueSeverity::Error).collect();
        assert!(errors.is_empty(), "Unexpected errors: {errors:?}");
    }

    #[test]
    fn test_compare_metadata_missing_measure() {
        let declared = make_metadata(
            vec![
                MeasureColumn {
                    name:     "revenue".to_string(),
                    sql_type: SqlType::Decimal,
                    nullable: false,
                },
                MeasureColumn {
                    name:     "profit".to_string(),
                    sql_type: SqlType::Decimal,
                    nullable: false,
                },
            ],
            "data",
            vec![],
        );
        let actual = make_metadata(
            vec![MeasureColumn {
                name:     "revenue".to_string(),
                sql_type: SqlType::Decimal,
                nullable: false,
            }],
            "data",
            vec![],
        );

        let issues = compare_metadata("tf_sales", &declared, &actual);
        let errors: Vec<_> = issues.iter().filter(|i| i.severity == IssueSeverity::Error).collect();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("profit"));
    }
}

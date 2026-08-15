mod executor_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use fraiseql_core::schema::{
        ArgumentDefinition, CompiledSchema, FieldDefinition, FieldType, McpConfig, QueryDefinition,
        TypeDefinition,
    };

    use super::super::executor::{build_operation, is_scalar_field_type, is_valid_graphql_name};

    #[test]
    fn test_is_valid_graphql_name() {
        assert!(is_valid_graphql_name("limit"));
        assert!(is_valid_graphql_name("_private"));
        assert!(is_valid_graphql_name("field1"));
        assert!(!is_valid_graphql_name(""));
        assert!(!is_valid_graphql_name("1abc"));
        assert!(!is_valid_graphql_name("has space"));
        assert!(!is_valid_graphql_name("inject: bad"));
    }

    #[test]
    fn test_is_scalar_field_type() {
        assert!(is_scalar_field_type(&FieldType::String));
        assert!(is_scalar_field_type(&FieldType::Int));
        assert!(is_scalar_field_type(&FieldType::List(Box::new(FieldType::Int))));
        assert!(!is_scalar_field_type(&FieldType::Object("User".to_string())));
    }

    /// One query `users(filter: JSON, limit: Int)` returning `User { id name }`.
    fn schema() -> CompiledSchema {
        let mut users = QueryDefinition::new("users", "User");
        users.returns_list = true;
        users.arguments.push(ArgumentDefinition::optional("filter", FieldType::Json));
        users.arguments.push(ArgumentDefinition::optional("limit", FieldType::Int));

        let mut user_type = TypeDefinition::new("User", "v_user");
        user_type.fields.push(FieldDefinition::new("id", FieldType::Id));
        user_type.fields.push(FieldDefinition::new("name", FieldType::String));

        let mut schema = CompiledSchema {
            queries: vec![users],
            types: vec![user_type],
            ..CompiledSchema::default()
        };
        schema.build_indexes();
        schema
    }

    fn open_config() -> McpConfig {
        McpConfig {
            enabled: true,
            require_auth: false,
            ..McpConfig::default()
        }
    }

    /// Parse a document and return its root field names, so an assertion can be made
    /// about the shape of the operation rather than about the text of it.
    fn root_fields(document: &str) -> Vec<String> {
        use graphql_parser::query::{Definition, OperationDefinition, Selection};

        let doc = graphql_parser::parse_query::<String>(document)
            .map_err(|e| format!("built document must parse: {e}\n{document}"))
            .expect("valid GraphQL document");
        let mut roots = Vec::new();
        for def in &doc.definitions {
            let Definition::Operation(op) = def else {
                continue;
            };
            let selection_set = match op {
                OperationDefinition::Query(q) => &q.selection_set,
                OperationDefinition::Mutation(m) => &m.selection_set,
                OperationDefinition::Subscription(s) => &s.selection_set,
                OperationDefinition::SelectionSet(s) => s,
            };
            for selection in &selection_set.items {
                if let Selection::Field(field) = selection {
                    roots.push(field.name.clone());
                }
            }
        }
        roots
    }

    /// **The class gate for #808.** No caller-supplied value — however nested,
    /// however crafted — may change the *shape* of the built document.
    ///
    /// Each payload below closes the argument list and appends a second root field
    /// when values are spliced into the document as literals; every one of them
    /// produced a valid multi-root document under the previous implementation, and
    /// the runtime fans multi-root queries out in parallel, so the injected root
    /// executed. Values are now passed as GraphQL variables, so the document
    /// contains exactly one root field whatever the payload is.
    #[test]
    fn no_argument_value_can_change_the_shape_of_the_document() {
        let schema = schema();
        let config = open_config();

        let payloads = [
            // Flat object key — the reported repro.
            serde_json::json!({ "filter": { "a: 1}) { id } secrets { token } q2: users(filter: {b": 1 } }),
            // One level deeper: the same key interpolation, recursed into.
            serde_json::json!({ "filter": { "outer": { "a: 1}}) { id } secrets { token } x: users(filter: {y: {b": 1 } } }),
            // Inside an array, which reaches the very same object rendering.
            serde_json::json!({ "filter": [{ "a: 1}]) { id } secrets { token } x: users(filter: [{b": 1 }] }),
            // A string value carrying quote/brace/newline escapes.
            serde_json::json!({ "filter": "\"} ) { id } secrets { token } x: users(filter: \"" }),
            // A key that is not a GraphQL identifier at all.
            serde_json::json!({ "filter": { "$@#": 1 } }),
            // Deep nesting, to make sure nothing bails out and falls back to text.
            serde_json::json!({ "filter": { "a": { "b": { "c": { "d": { "e}}}}}) { id } secrets { token } x: users(filter: {f": 1 } } } } } }),
        ];

        for payload in payloads {
            let args = payload.as_object().unwrap();
            let built = build_operation("users", Some(args), &schema, &config);
            assert!(built.is_ok(), "payload must build, not be rejected: {:?}", built.err());
            let op = built.expect("checked above");

            assert_eq!(
                root_fields(&op.document),
                vec!["users".to_string()],
                "a caller-supplied value changed the document shape: {}",
                op.document,
            );
            assert_eq!(
                op.variables.get("filter"),
                args.get("filter"),
                "the value must reach the executor as a variable, unchanged",
            );
        }
    }

    /// An argument the resolved operation does not declare is refused, and the error
    /// says which arguments are accepted. The advertised input schema is built from
    /// the same list, so "advertised" and "accepted" cannot drift.
    #[test]
    fn an_undeclared_argument_is_refused() {
        let schema = schema();
        let args = serde_json::json!({ "notAnArgument": 1 });

        let err =
            build_operation("users", Some(args.as_object().unwrap()), &schema, &open_config())
                .err()
                .expect("an undeclared argument must be refused");

        assert!(err.contains("notAnArgument"), "{err}");
        assert!(err.contains("filter"), "the error should list the accepted arguments: {err}");
    }

    /// A call with no arguments emits no variable definitions and no argument list.
    #[test]
    fn a_call_with_no_arguments_declares_no_variables() {
        let schema = schema();
        let op = build_operation("users", None, &schema, &open_config()).unwrap();

        assert_eq!(root_fields(&op.document), vec!["users".to_string()]);
        assert!(op.variables.is_empty(), "{:?}", op.variables);
        assert!(!op.document.contains('$'), "no variable definitions expected: {}", op.document);
    }

    /// **The generative form of the #808 gate.** The test above pins five payloads
    /// someone thought of; this one searches the space.
    ///
    /// The original defect was that `graphql_value` rendered a JSON object by
    /// interpolating `{k}: {v}` with no validation of `k`, so a nested key could
    /// close the argument list and append a root field of the caller's choosing —
    /// reaching operations the `[mcp] include`/`exclude` allowlist excluded. Only
    /// *top-level* argument names were validated, and the comment there said
    /// explicitly that it was "to prevent injection via malformed argument names".
    ///
    /// The invariant, for any argument value whatsoever: the built document has
    /// exactly one root field, it is the resolved tool, and the value reaches the
    /// executor through `variables` unchanged rather than through the document.
    mod issue_808_no_value_reaches_the_document {
        use proptest::prelude::*;

        use super::*;

        /// Fragments chosen to terminate a GraphQL argument list and open a new
        /// selection — the shape the injection needed.
        fn hostile_fragment() -> impl Strategy<Value = String> {
            prop::sample::select(vec![
                "a".to_string(),
                "}) { id } secrets { token } x: users(filter: {a".to_string(),
                "\"} ) { id } evil { token } y: users(filter: \"".to_string(),
                "$@#".to_string(),
                "__typename".to_string(),
                "a b".to_string(),
                "}".to_string(),
                "{".to_string(),
                ")".to_string(),
                "\n".to_string(),
            ])
        }

        /// Arbitrary JSON, with hostile text reachable at every key and leaf.
        fn hostile_json() -> impl Strategy<Value = serde_json::Value> {
            let leaf = prop_oneof![
                hostile_fragment().prop_map(serde_json::Value::String),
                any::<i32>().prop_map(|n| serde_json::json!(n)),
                any::<bool>().prop_map(|b| serde_json::json!(b)),
                Just(serde_json::Value::Null),
            ];
            leaf.prop_recursive(4, 32, 4, |inner| {
                prop_oneof![
                    prop::collection::vec(inner.clone(), 0..4).prop_map(serde_json::Value::Array),
                    prop::collection::vec((hostile_fragment(), inner), 0..4).prop_map(|pairs| {
                        serde_json::Value::Object(pairs.into_iter().collect())
                    }),
                ]
            })
        }

        proptest! {
            #[test]
            fn no_argument_value_can_add_a_root_field(value in hostile_json()) {
                let schema = schema();
                let config = open_config();
                let args = serde_json::json!({ "filter": value });
                let args = args.as_object().unwrap();

                let Ok(op) = build_operation("users", Some(args), &schema, &config) else {
                    // Refusing is an acceptable outcome; executing the wrong thing is not.
                    return Ok(());
                };

                prop_assert_eq!(
                    root_fields(&op.document),
                    vec!["users".to_string()],
                    "a caller-supplied value changed the document shape: {}",
                    op.document
                );
                prop_assert_eq!(
                    op.variables.get("filter"),
                    args.get("filter"),
                    "the value must reach the executor as a variable, unchanged"
                );
            }
        }
    }

    /// Only the arguments actually supplied are declared as variables — an
    /// unsupplied optional must not become an explicit `null`.
    #[test]
    fn only_supplied_arguments_become_variables() {
        let schema = schema();
        let args = serde_json::json!({ "limit": 10 });

        let op = build_operation("users", Some(args.as_object().unwrap()), &schema, &open_config())
            .unwrap();

        assert!(op.document.contains("$limit: Int"), "{}", op.document);
        assert!(!op.document.contains("filter"), "{}", op.document);
        assert_eq!(op.variables.len(), 1);
        assert_eq!(op.variables.get("limit"), Some(&serde_json::json!(10)));
    }
}

mod tools_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use fraiseql_core::schema::{ArgumentDefinition, FieldType};

    use super::super::{
        McpConfig,
        tools::{arguments_to_json_schema, field_type_to_json_schema, should_include},
    };

    fn make_config(include: Vec<String>, exclude: Vec<String>) -> McpConfig {
        McpConfig {
            enabled: true,
            transport: "http".to_string(),
            path: "/mcp".to_string(),
            require_auth: true,
            include,
            exclude,
            read_only: false,
        }
    }

    /// #376: every advertised tool carries MCP behaviour hints — a query is
    /// `readOnlyHint: true`, a mutation is explicitly non-read-only,
    /// destructive and non-idempotent, so an agent client prompts before
    /// invoking a write (the issue's `confirmation_required` acceptance).
    #[test]
    fn tools_carry_behaviour_annotations() {
        use fraiseql_core::schema::{CompiledSchema, MutationDefinition, QueryDefinition};

        use super::super::tools::schema_to_tools;

        let mut schema = CompiledSchema::default();
        schema.queries.push(QueryDefinition::new("users", "User"));
        schema.mutations.push(MutationDefinition::new("createUser", "User"));
        schema.build_indexes();

        let tools = schema_to_tools(&schema, &make_config(vec![], vec![]));

        let query = tools.iter().find(|t| t.name == "users").expect("query tool advertised");
        let ann = query.annotations.as_ref().expect("query tool carries annotations");
        assert_eq!(ann.read_only_hint, Some(true), "a query never writes");
        assert_eq!(ann.open_world_hint, Some(false), "our world is the schema's database");

        let mutation =
            tools.iter().find(|t| t.name == "createUser").expect("mutation tool advertised");
        let ann = mutation.annotations.as_ref().expect("mutation tool carries annotations");
        assert_eq!(ann.read_only_hint, Some(false), "a mutation writes");
        assert_eq!(
            ann.destructive_hint,
            Some(true),
            "explicitly destructive — the schema cannot prove a function additive-only, and \
             this is what makes an agent client confirm before invoking"
        );
        assert_eq!(ann.idempotent_hint, Some(false), "a repeated INSERT is a second row");
    }

    /// `[mcp] read_only`: with `read_only`, no mutation is ever a tool, regardless of
    /// `include`/`exclude`, and adding a mutation to the schema changes nothing — the
    /// regression the flag exists to prevent.
    #[test]
    fn read_only_exposes_no_mutations_regardless_of_include() {
        use fraiseql_core::schema::{CompiledSchema, MutationDefinition, QueryDefinition};

        use super::super::tools::schema_to_tools;

        let mut schema = CompiledSchema::default();
        schema.queries.push(QueryDefinition::new("users", "User"));
        schema.mutations.push(MutationDefinition::new("createUser", "User"));
        schema.mutations.push(MutationDefinition::new("deleteUser", "User"));

        // Baseline (not read_only): the query + both mutations are exposed.
        let open = make_config(vec![], vec![]);
        assert_eq!(schema_to_tools(&schema, &open).len(), 3, "1 query + 2 mutations exposed");

        // read_only with no include → only the query survives (no mutation is a tool).
        let mut read_only = make_config(vec![], vec![]);
        read_only.read_only = true;
        assert_eq!(
            schema_to_tools(&schema, &read_only).len(),
            1,
            "read_only exposes only the query"
        );

        // read_only WINS over `include`: an include naming a mutation would expose it,
        // but read_only excludes every mutation → the mutation is not a tool. (The
        // query is also gated out by the non-empty include, so zero tools remain,
        // proving the mutation named in `include` was excluded by read_only.)
        let mut with_include = make_config(vec!["createUser".to_string()], vec![]);
        with_include.read_only = true;
        assert_eq!(
            schema_to_tools(&schema, &with_include).len(),
            0,
            "read_only wins over include listing a mutation"
        );

        // Adding another mutation to the schema changes nothing under read_only.
        schema.mutations.push(MutationDefinition::new("wipeAll", "User"));
        assert_eq!(
            schema_to_tools(&schema, &read_only).len(),
            1,
            "new mutation not silently exposed"
        );
    }

    #[test]
    fn test_should_include_all_when_empty() {
        let config = make_config(vec![], vec![]);
        assert!(should_include("users", &config));
        assert!(should_include("createUser", &config));
    }

    #[test]
    fn test_should_include_whitelist() {
        let config = make_config(vec!["users".to_string()], vec![]);
        assert!(should_include("users", &config));
        assert!(!should_include("createUser", &config));
    }

    #[test]
    fn test_should_include_blacklist() {
        let config = make_config(vec![], vec!["createUser".to_string()]);
        assert!(should_include("users", &config));
        assert!(!should_include("createUser", &config));
    }

    #[test]
    fn test_field_type_to_json_schema() {
        let schema = field_type_to_json_schema(&FieldType::String);
        assert_eq!(schema, serde_json::json!({ "type": "string" }));

        let schema = field_type_to_json_schema(&FieldType::Int);
        assert_eq!(schema, serde_json::json!({ "type": "integer" }));

        let schema = field_type_to_json_schema(&FieldType::Boolean);
        assert_eq!(schema, serde_json::json!({ "type": "boolean" }));

        let schema = field_type_to_json_schema(&FieldType::List(Box::new(FieldType::Int)));
        assert_eq!(schema, serde_json::json!({ "type": "array", "items": { "type": "integer" } }));
    }

    #[test]
    fn test_arguments_to_json_schema() {
        let args = vec![
            ArgumentDefinition::new("id", FieldType::Id),
            ArgumentDefinition::optional("name", FieldType::String),
        ];

        let schema = arguments_to_json_schema(&args);
        let props = schema.get("properties").unwrap().as_object().unwrap();
        assert!(props.contains_key("id"));
        assert!(props.contains_key("name"));

        let required = schema.get("required").unwrap().as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "id");
    }
}

mod handler_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::super::handler::extract_bearer;

    #[test]
    fn extract_bearer_returns_token_for_well_formed_header() {
        let mut headers = http::HeaderMap::new();
        headers.insert(http::header::AUTHORIZATION, "Bearer abc.def.ghi".parse().unwrap());
        assert_eq!(extract_bearer(&headers), Some("abc.def.ghi".to_string()));
    }

    #[test]
    fn extract_bearer_none_when_header_missing() {
        assert_eq!(extract_bearer(&http::HeaderMap::new()), None);
    }

    #[test]
    fn extract_bearer_none_for_non_bearer_scheme() {
        let mut headers = http::HeaderMap::new();
        headers.insert(http::header::AUTHORIZATION, "Basic dXNlcjpwYXNz".parse().unwrap());
        assert_eq!(extract_bearer(&headers), None);
    }

    #[test]
    fn extract_bearer_none_for_empty_token() {
        let mut headers = http::HeaderMap::new();
        headers.insert(http::header::AUTHORIZATION, "Bearer    ".parse().unwrap());
        assert_eq!(extract_bearer(&headers), None);
    }
}

/// #967: Resources and Prompts.
///
/// The load-bearing property is not the shape of the advertisement — it is that
/// all three surfaces are derived from **one** allowlist, so a Resource can never
/// name an operation `tools/call` would refuse to run, and `read_resource`'s
/// execution goes through the tool seam rather than beside it.
mod resource_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use fraiseql_core::schema::{
        ArgumentDefinition, CompiledSchema, FieldDefinition, FieldType, McpConfig,
        MutationDefinition, QueryDefinition, TypeDefinition, VectorConfig,
    };

    use super::super::resources::{
        query_name_from_uri, render_prompt, schema_to_prompts, schema_to_resource_templates,
        schema_to_resources,
    };

    /// Two queries (one vector-backed, one not) and one mutation.
    fn schema() -> CompiledSchema {
        let mut s = CompiledSchema::new();

        let mut user = TypeDefinition::new("User", "v_user");
        user.fields.push(FieldDefinition::new("id", FieldType::Id));
        user.fields.push(FieldDefinition::new("name", FieldType::String));
        s.types.push(user);

        let mut doc = TypeDefinition::new("Doc", "v_doc");
        doc.fields.push(FieldDefinition::new("id", FieldType::Id));
        let mut embedding = FieldDefinition::new("embedding", FieldType::Vector);
        embedding.vector_config = Some(VectorConfig {
            dimensions: 3,
            ..VectorConfig::default()
        });
        doc.fields.push(embedding);
        s.types.push(doc);

        let mut users = QueryDefinition::new("users", "User").returning_list();
        users.description = Some("Every registered user".to_string());
        users.sql_source = Some("v_user".to_string());
        s.queries.push(users);

        let mut docs = QueryDefinition::new("docs", "Doc").returning_list();
        docs.sql_source = Some("v_doc".to_string());
        s.queries.push(docs);

        let mut create = MutationDefinition::new("createUser", "User");
        create.description = Some("Register a user".to_string());
        create.arguments = vec![ArgumentDefinition::new("name", FieldType::String)];
        s.mutations.push(create);

        s.build_indexes();
        s
    }

    fn config() -> McpConfig {
        McpConfig {
            enabled: true,
            ..McpConfig::default()
        }
    }

    /// Queries are Resources; mutations are not.
    ///
    /// `resources/read` is a read verb in every client that speaks MCP. A mutation
    /// advertised as a Resource would be a write behind a verb that promises
    /// otherwise, whatever `read_only` is set to.
    #[test]
    fn resources_are_the_queries_and_never_the_mutations() {
        let resources = schema_to_resources(&schema(), &config());
        let names: Vec<&str> = resources.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["users", "docs"], "queries only: {names:?}");
        assert_eq!(resources[0].uri, "fraiseql://query/users");
        assert_eq!(
            resources[0].description.as_deref(),
            Some("Every registered user"),
            "the authored description is used when there is one"
        );
        assert!(
            resources[1].description.as_deref().is_some_and(|d| d.contains("docs")),
            "and a query with none still gets one: {:?}",
            resources[1].description
        );
    }

    /// The allowlist governs the Resource list, not just the tool list.
    ///
    /// A Resource naming an operation `exclude` withholds would be an existence
    /// oracle for exactly the names an operator chose to hide, and reading it
    /// would be a second door around `resolve_tool` (#808).
    #[test]
    fn the_allowlist_governs_resources_and_prompts_too() {
        let hidden = McpConfig {
            exclude: vec!["users".to_string()],
            ..config()
        };
        let resources = schema_to_resources(&schema(), &hidden);
        assert!(
            resources.iter().all(|r| r.name != "users"),
            "an excluded query is not advertised as a Resource"
        );
        let prompts = schema_to_prompts(&schema(), &hidden);
        assert!(prompts.iter().all(|p| p.name != "users"), "nor described as a Prompt");
        // …and `render_prompt` refuses it, indistinguishably from a name that
        // does not exist.
        assert!(render_prompt("users", None, &schema(), &hidden).is_none());
        assert!(render_prompt("nosuchthing", None, &schema(), &hidden).is_none());
    }

    /// `read_only` removes mutations from every surface at once, because all three
    /// read the same exposed set.
    #[test]
    fn read_only_removes_the_mutation_from_the_prompt_list() {
        let ro = McpConfig {
            read_only: true,
            ..config()
        };
        let prompts = schema_to_prompts(&schema(), &ro);
        assert!(
            prompts.iter().all(|p| p.name != "createUser"),
            "read_only withholds the mutation from Prompts as it does from tools"
        );
        assert!(render_prompt("createUser", None, &schema(), &ro).is_none());
    }

    /// Only the vector-backed query gets a similarity-search template.
    #[test]
    fn only_a_vector_backed_query_gets_a_similarity_template() {
        let templates = schema_to_resource_templates(&schema(), &config());
        assert_eq!(templates.len(), 1, "one template: {templates:?}");
        assert!(templates[0].name.starts_with("docs"), "{:?}", templates[0].name);
        assert!(
            templates[0].uri_template.contains("nearest"),
            "the template names the argument that makes it a search: {:?}",
            templates[0].uri_template
        );
    }

    /// The URI parser accepts exactly one shape.
    ///
    /// Anything else is refused rather than normalised: the returned name is
    /// looked up by exact match in the exposed set, so a parser that trimmed a
    /// path or dropped a query string would be inventing a name the caller did
    /// not send.
    #[test]
    fn the_uri_parser_accepts_one_path_segment_and_nothing_else() {
        assert_eq!(query_name_from_uri("fraiseql://query/users"), Some("users"));
        for bad in [
            "fraiseql://query/",
            "fraiseql://query/users/extra",
            "fraiseql://query/users?limit=1",
            "fraiseql://query/users#frag",
            "fraiseql://mutation/createUser",
            "file:///etc/passwd",
            "users",
        ] {
            assert!(query_name_from_uri(bad).is_none(), "must refuse {bad:?}");
        }
    }

    /// A Prompt carries the operation's arguments, with required-ness taken from
    /// nullability.
    #[test]
    fn a_prompt_describes_the_operations_arguments() {
        let prompts = schema_to_prompts(&schema(), &config());
        let create = prompts.iter().find(|p| p.name == "createUser").expect("mutation prompt");
        assert_eq!(create.description.as_deref(), Some("Register a user"));
        let args = create.arguments.as_ref().expect("createUser takes an argument");
        assert_eq!(args[0].name, "name");
        assert_eq!(args[0].required, Some(true), "a non-nullable argument is required");
    }

    /// The rendered message names the tool and its arguments, in a stable order.
    ///
    /// Stable because an agent's behaviour has to be reproducible: iterating a
    /// map would render the same request differently between runs.
    #[test]
    fn a_rendered_prompt_is_stable_and_names_the_tool() {
        let mut args = serde_json::Map::new();
        args.insert("zeta".to_string(), serde_json::json!(1));
        args.insert("alpha".to_string(), serde_json::json!("x"));

        let (_, messages) =
            render_prompt("createUser", Some(&args), &schema(), &config()).expect("renders");
        let text = format!("{:?}", messages[0]);
        assert!(text.contains("createUser"), "{text}");
        let alpha = text.find("alpha").expect("alpha present");
        let zeta = text.find("zeta").expect("zeta present");
        assert!(alpha < zeta, "arguments render in sorted order, not map order: {text}");
    }
}

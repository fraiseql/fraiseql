//! Tests for the client codegen module.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::path::PathBuf;

use fraiseql_core::schema::{
    ArgumentDefinition, CompiledSchema, EnumDefinition, EnumValueDefinition, FieldDefinition,
    FieldType, InputFieldDefinition, InputObjectDefinition, InterfaceDefinition,
    MutationDefinition, QueryDefinition, TypeDefinition, UnionDefinition,
};

use super::schema_hash;

#[test]
fn schema_hash_is_stable_and_hex_encoded() {
    let schema = CompiledSchema::default();
    let first = schema_hash(&schema).unwrap();
    let second = schema_hash(&schema).unwrap();

    assert_eq!(first, second, "hashing the same schema must be deterministic");
    assert_eq!(first.len(), 64, "sha256 hex digest is 64 characters");
    assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn schema_hash_changes_when_schema_changes() {
    let empty = CompiledSchema::default();

    let mut populated = CompiledSchema::default();
    populated.types.push(TypeDefinition::new("User", "v_user"));
    populated.queries.push(QueryDefinition::new("users", "User"));

    assert_ne!(
        schema_hash(&empty).unwrap(),
        schema_hash(&populated).unwrap(),
        "different schemas must hash differently"
    );
}

/// The load-bearing shared-core guarantee: the `GraphQL` documents inside every
/// generated client are byte-identical, so no target language can drift into a
/// different query.
///
/// This is what `client::common` exists for. It lives here rather than under one
/// language because it is a claim about all of them at once — a copy owned by
/// the Python generator would go on passing after a fifth language shipped its
/// own document renderer.
#[test]
fn documents_are_identical_across_languages() {
    let schema = document_fixture();

    let py = super::python::generate(&schema).unwrap();
    let ts = super::typescript::generate(&schema).unwrap();
    let go = super::go::generate(&schema).unwrap();
    let rs = super::rust::generate(&schema).unwrap();

    let py_docs = docs(&py, &["queries.py", "mutations.py"], "\"\"\"", "\"\"\"");
    let ts_docs = docs(&ts, &["queries.ts", "mutations.ts"], "`", "`");
    let go_docs = docs(&go, &["queries.go", "mutations.go"], "`", "`");
    let rs_docs = docs(&rs, &["queries.rs", "mutations.rs"], "r#\"", "\"#");

    assert_eq!(py_docs.len(), 4, "fixture has 3 queries + 1 mutation: {py_docs:?}");
    assert_eq!(py_docs, ts_docs, "Python and TypeScript documents drifted");
    assert_eq!(py_docs, go_docs, "Python and Go documents drifted");
    assert_eq!(py_docs, rs_docs, "Python and Rust documents drifted");
}

/// A schema exercising every document shape the renderers can produce: required
/// and optional arguments, a list return, a Relay connection, and a mutation.
fn document_fixture() -> CompiledSchema {
    let mut schema = CompiledSchema::new();

    let mut user = TypeDefinition::new("User", "v_user");
    user.fields.push(FieldDefinition::new("id", FieldType::Id));
    user.fields.push(FieldDefinition::new("email", FieldType::String));
    user.fields
        .push(FieldDefinition::new("role", FieldType::Enum("UserRole".to_string())));
    schema.types.push(user);

    schema.enums.push(EnumDefinition {
        name:        "UserRole".to_string(),
        values:      vec![EnumValueDefinition {
            name:        "ADMIN".to_string(),
            description: None,
            deprecation: None,
            value:       None,
        }],
        description: None,
    });

    let mut get_user = QueryDefinition::new("getUser", "User");
    get_user.nullable = true;
    get_user.arguments.push(ArgumentDefinition::new("id", FieldType::Id));
    schema.queries.push(get_user);

    let mut users = QueryDefinition::new("users", "User").returning_list();
    let mut limit = ArgumentDefinition::new("limit", FieldType::Int);
    limit.nullable = true;
    users.arguments.push(limit);
    schema.queries.push(users);

    let mut relay = QueryDefinition::new("usersConnection", "User");
    relay.relay = true;
    schema.queries.push(relay);

    let mut create = MutationDefinition::new("createUser", "User");
    create.arguments.push(ArgumentDefinition::new("email", FieldType::String));
    schema.mutations.push(create);

    schema
}

/// Extract every operation document from a generated client's operation files.
fn docs(files: &crate::Generated, names: &[&str], open: &str, close: &str) -> Vec<String> {
    names
        .iter()
        .flat_map(|name| {
            let content = files
                .get(&PathBuf::from(*name))
                .unwrap_or_else(|| panic!("missing generated file {name}"));
            extract_delimited(content, open, close)
        })
        .filter(|doc| doc.starts_with("query ") || doc.starts_with("mutation "))
        .collect()
}

/// Parse every generated document with the **server's own** parser.
///
/// Nothing in this crate, the snapshot test, or the `sdk-conformance` workflow
/// ever parsed a generated document — they only type-check the surrounding
/// `TypeScript`/Python. That is why an unparseable document could ship (#1032,
/// #1066): the generated client type-checks perfectly and dies on first call.
fn assert_documents_parse(files: &crate::Generated, names: &[&str], open: &str, close: &str) {
    let documents = docs(files, names, open, close);
    assert!(!documents.is_empty(), "fixture produced no documents to parse");

    for document in &documents {
        assert!(
            fraiseql_core::graphql::parse_query(document).is_ok(),
            "generated document does not parse:\n{document}"
        );
    }
}

/// #1066 — `FieldType::Vector` renders as the already-decorated `[Float!]!`, so
/// appending the non-null `!` emitted `$embedding: [Float!]!!`. Every call to a
/// vector-argument operation died at the server's parse step.
#[test]
fn a_vector_argument_produces_a_parseable_document() {
    let schema = vector_argument_fixture(false);
    let generated = super::python::generate(&schema).unwrap();

    assert_documents_parse(&generated, &["queries.py"], "\"\"\"", "\"\"\"");
}

/// The nullable half of #1066: a `Vector` argument the author made optional was
/// declared `$embedding: [Float!]!` while the wrapper made the parameter
/// optional, so the document parses but demands a variable the client need never
/// supply. Requiredness must follow `nullable`, not the type's own decoration.
#[test]
fn a_nullable_vector_argument_is_declared_nullable() {
    let schema = vector_argument_fixture(true);
    let generated = super::python::generate(&schema).unwrap();
    let documents = docs(&generated, &["queries.py"], "\"\"\"", "\"\"\"");
    let document = documents.first().expect("fixture has one query");

    assert!(
        document.contains("$embedding: [Float!]"),
        "vector argument missing from document:\n{document}"
    );
    assert!(
        !document.contains("$embedding: [Float!]!"),
        "an optional argument must not be declared non-null:\n{document}"
    );
}

/// #1065 — every generator decided input-field requiredness from a trailing `!`
/// on the type string, while the runtime enforces
/// `is_required() = !nullable && default_value.is_none()`.
///
/// The official SDKs emit bare type strings with requiredness carried only in
/// `nullable`, so every required input field generated as **optional**: the
/// consumer wrote a call omitting it, `tsc --strict` and `ty` both accepted it,
/// and the server rejected the mutation at
/// `InputFieldDefinition::is_required()`.
///
/// This is a claim about all four generators at once — the issue named two, and
/// Go and Rust parse the same `!`.
#[test]
fn input_requiredness_follows_the_flag_the_runtime_enforces() {
    let mut schema = CompiledSchema::new();

    let mut user = TypeDefinition::new("User", "v_user");
    user.fields.push(FieldDefinition::new("id", FieldType::Id));
    schema.types.push(user);

    schema
        .input_types
        .push(InputObjectDefinition::new("CreateUserInput").with_fields(vec![
        // The SDK-authored spelling: bare type, requiredness in the flag.
        InputFieldDefinition::new("email", "String").with_nullable(false),
        InputFieldDefinition::new("note", "String"),
        // Non-null but defaulted — `is_required()` is false, and no generator
        // ever read `default_value` at all.
        InputFieldDefinition::new("role", "String")
            .with_nullable(false)
            .with_default_value("\"MEMBER\""),
    ]));

    let mut create = MutationDefinition::new("createUser", "User");
    create.arguments.push(ArgumentDefinition::new(
        "input",
        FieldType::Input("CreateUserInput".to_string()),
    ));
    schema.mutations.push(create);

    let ts = super::typescript::generate(&schema).unwrap();
    let inputs_ts = ts.get(&PathBuf::from("inputs.ts")).unwrap().clone();
    assert!(inputs_ts.contains("email: string;"), "TS: email must be required: {inputs_ts}");
    assert!(
        inputs_ts.contains("note?: string | null;"),
        "TS: note must stay optional: {inputs_ts}"
    );
    assert!(
        inputs_ts.contains("role?: string | null;"),
        "TS: a defaulted field is not required: {inputs_ts}"
    );

    let py = super::python::generate(&schema).unwrap();
    let inputs_py = py.get(&PathBuf::from("inputs.py")).unwrap().clone();
    assert!(inputs_py.contains("email: str"), "Py: email must be required: {inputs_py}");
    assert!(
        inputs_py.contains("note: NotRequired[str | None]"),
        "Py: note must stay optional: {inputs_py}"
    );

    // Go and Rust are column-aligned, so match on the field's own line rather
    // than an exact-spacing substring.
    let line_for = |source: &str, needle: &str| -> String {
        source
            .lines()
            .find(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("no line containing {needle} in:\n{source}"))
            .to_string()
    };

    let go = super::go::generate(&schema).unwrap();
    let inputs_go = go.get(&PathBuf::from("inputs.go")).unwrap().clone();
    let go_email = line_for(&inputs_go, "Email");
    assert!(
        !go_email.contains('*') && !go_email.contains("omitempty"),
        "Go: email must be a required value, not an omitempty pointer: {go_email}"
    );
    assert!(
        line_for(&inputs_go, "Note").contains("omitempty"),
        "Go: note must stay optional: {inputs_go}"
    );
    assert!(
        line_for(&inputs_go, "Role").contains("omitempty"),
        "Go: a defaulted field is not required: {inputs_go}"
    );

    let rs = super::rust::generate(&schema).unwrap();
    let inputs_rs = rs.get(&PathBuf::from("inputs.rs")).unwrap().clone();
    let rs_email = line_for(&inputs_rs, "pub email");
    assert!(!rs_email.contains("Option<"), "Rust: email must be required: {rs_email}");
    assert!(
        line_for(&inputs_rs, "pub note").contains("Option<"),
        "Rust: note must stay optional: {inputs_rs}"
    );
}

/// The `TypeScript` half of #1035. The two languages reserve different words, so
/// this is two fixes rather than one: `delete`, `new` and `function` break
/// `TypeScript` and are ordinary Python identifiers, while `from`, `del` and
/// `lambda` break Python and are fine in `TypeScript`.
///
/// `delete` is the reachable case. It is a perfectly ordinary Python function
/// name, so it registers verbatim as a `GraphQL` operation through the flagship
/// Python SDK, and then the generated `TypeScript` client emits
/// `export async function delete(` — a parser error that takes down the whole
/// package through `index.ts`'s star re-exports.
///
/// Go and Rust need no equivalent: `go_export` capitalises the first letter and
/// every Go keyword is lowercase, and `rs_ident` already escapes.
#[test]
fn a_reserved_operation_name_is_escaped_in_the_typescript_client() {
    let mut schema = CompiledSchema::new();

    let mut thing = TypeDefinition::new("Thing", "v_thing");
    thing.fields.push(FieldDefinition::new("id", FieldType::Id));
    schema.types.push(thing);

    schema.mutations.push(MutationDefinition::new("delete", "Thing"));

    let generated = super::typescript::generate(&schema).unwrap();
    let mutations = generated
        .get(&PathBuf::from("mutations.ts"))
        .expect("mutations.ts is generated")
        .clone();

    assert!(
        mutations.contains("export async function delete_("),
        "a reserved operation name must be escaped: {mutations}"
    );
    assert!(
        mutations.contains("data.delete;"),
        "the response key must stay the original GraphQL name: {mutations}"
    );
    assert!(
        mutations.contains("mutation delete"),
        "the document must stay the original GraphQL name: {mutations}"
    );
}

/// #1031 — a leaf return type still got a selection set. `type_selection` wrote
/// `__typename` for every return type, while `type_name_to_py`/`type_name_to_ts`
/// map scalar return-type names to `int`/`number` — so the generator promised the
/// caller a scalar and asked the server for a sub-selection of one.
///
/// The failure is not a rejected request: FraiseQL's own validator passes a
/// selection on an unknown type, so the caller gets HTTP 200 and an object where
/// its generated type says `int`.
#[test]
fn a_scalar_return_type_gets_no_selection_set() {
    let schema = leaf_return_fixture(&FieldType::Int, "userCount");
    let generated = super::python::generate(&schema).unwrap();
    let documents = docs(&generated, &["queries.py"], "\"\"\"", "\"\"\"");
    let document = documents.first().expect("fixture has one query");

    assert!(
        !document.contains("__typename"),
        "a scalar return type takes no sub-selection:\n{document}"
    );
    assert_documents_parse(&generated, &["queries.py"], "\"\"\"", "\"\"\"");
}

/// The same path, and the reason the adversarial review widened this finding: an
/// enum return type is in neither `unions` nor `object_types`, so it took the
/// identical branch — and there the emitted client even type-checks, because the
/// enum is imported by name, making the invalid document fully invisible.
#[test]
fn an_enum_return_type_gets_no_selection_set() {
    let schema = leaf_return_fixture(&FieldType::Enum("UserRole".to_string()), "currentRole");
    let generated = super::python::generate(&schema).unwrap();
    let documents = docs(&generated, &["queries.py"], "\"\"\"", "\"\"\"");
    let document = documents.first().expect("fixture has one query");

    assert!(
        !document.contains("__typename"),
        "an enum return type takes no sub-selection:\n{document}"
    );
}

/// The third case, and a silent drop rather than a bad request: `leaf_name_lines`
/// consulted only `object_types`, so a query returning an **interface** — which
/// the compiler explicitly registers as a legal return type — selected
/// `__typename` and discarded every field the interface declares.
#[test]
fn an_interface_return_type_selects_its_declared_fields() {
    let mut schema = CompiledSchema::new();

    let mut node = InterfaceDefinition::new("Node");
    node.fields.push(FieldDefinition::new("id", FieldType::Id));
    node.fields.push(FieldDefinition::new("createdAt", FieldType::DateTime));
    schema.interfaces.push(node);

    schema.queries.push(QueryDefinition::new("node", "Node"));

    let generated = super::python::generate(&schema).unwrap();
    let documents = docs(&generated, &["queries.py"], "\"\"\"", "\"\"\"");
    let document = documents.first().expect("fixture has one query");

    for field in ["id", "createdAt"] {
        assert!(
            document.contains(field),
            "interface field {field} was dropped from the document:\n{document}"
        );
    }
}

/// One root query returning `field_type`, with nothing else in the schema.
fn leaf_return_fixture(field_type: &FieldType, query_name: &str) -> CompiledSchema {
    let mut schema = CompiledSchema::new();

    if let FieldType::Enum(name) = field_type {
        schema.enums.push(EnumDefinition {
            name:        name.clone(),
            values:      vec![EnumValueDefinition {
                name:        "ADMIN".to_string(),
                description: None,
                deprecation: None,
                value:       None,
            }],
            description: None,
        });
    }

    schema
        .queries
        .push(QueryDefinition::new(query_name, field_type.to_graphql_string()));

    schema
}

/// #1032 — a union member with no leaf fields produced `... on X {\n}`. An empty
/// selection set is a parse error, so every call to that operation failed.
///
/// Both triggers the adversarial review left standing are covered: a member type
/// whose every field is composite, and a member name that does not resolve in
/// `schema.types` at all (a typo, an interface name, or a type never registered)
/// — `leaf_name_lines` returns `""` for both.
#[test]
fn a_union_member_without_leaf_fields_produces_a_parseable_document() {
    let schema = empty_union_member_fixture();
    let generated = super::python::generate(&schema).unwrap();

    assert_documents_parse(&generated, &["mutations.py"], "\"\"\"", "\"\"\"");
}

/// The empty fragment must not merely be dropped: the member has to stay in the
/// document, or a client narrowing on `__typename` can never match it.
#[test]
fn a_union_member_without_leaf_fields_keeps_its_inline_fragment() {
    let schema = empty_union_member_fixture();
    let generated = super::python::generate(&schema).unwrap();
    let documents = docs(&generated, &["mutations.py"], "\"\"\"", "\"\"\"");
    let document = documents.first().expect("fixture has one mutation");

    for member in ["CreateUserSuccess", "NeverRegistered"] {
        let opening = format!("... on {member} {{");
        let start = document
            .find(&opening)
            .unwrap_or_else(|| panic!("member {member} lost its inline fragment:\n{document}"));
        let body = &document[start + opening.len()..];
        let end = body.find('}').expect("inline fragment is never closed");

        assert!(
            !body[..end].trim().is_empty(),
            "the inline fragment for {member} is empty, which no GraphQL parser \
             accepts:\n{document}"
        );
    }
}

/// A payload union whose members carry no leaf fields between them: one is
/// composite-only, one is never registered as a type at all.
fn empty_union_member_fixture() -> CompiledSchema {
    let mut schema = CompiledSchema::new();

    let mut user = TypeDefinition::new("User", "v_user");
    user.fields.push(FieldDefinition::new("id", FieldType::Id));
    schema.types.push(user);

    // Every field composite, so `leaf_fields` is empty.
    let mut success = TypeDefinition::new("CreateUserSuccess", "v_create_user_success");
    success
        .fields
        .push(FieldDefinition::new("user", FieldType::Object("User".to_string())));
    schema.types.push(success);

    schema.unions.push(UnionDefinition {
        name:         "CreateUserResult".to_string(),
        member_types: vec![
            "CreateUserSuccess".to_string(),
            "NeverRegistered".to_string(),
        ],
        description:  None,
    });

    schema.mutations.push(MutationDefinition::new("createUser", "CreateUserResult"));

    schema
}

/// A single vector-similarity query, whose argument is the only thing under test.
fn vector_argument_fixture(nullable: bool) -> CompiledSchema {
    let mut schema = CompiledSchema::new();

    let mut doc_type = TypeDefinition::new("Doc", "v_doc");
    doc_type.fields.push(FieldDefinition::new("id", FieldType::Id));
    schema.types.push(doc_type);

    let mut search = QueryDefinition::new("searchDocs", "Doc").returning_list();
    let mut embedding = ArgumentDefinition::new("embedding", FieldType::Vector);
    embedding.nullable = nullable;
    search.arguments.push(embedding);
    schema.queries.push(search);

    schema
}

/// Extract delimited blocks (documents) from generated source.
fn extract_delimited(source: &str, open: &str, close: &str) -> Vec<String> {
    let mut docs = Vec::new();
    let mut rest = source;
    while let Some(start) = rest.find(open) {
        let after = &rest[start + open.len()..];
        let Some(end) = after.find(close) else { break };
        docs.push(after[..end].to_string());
        rest = &after[end + close.len()..];
    }
    docs
}

/// #1614: an argument's list items are typed as declared in every generated client, as an
/// output field's are (#1586). The argument renderers ignored item nullability, so a
/// `[String]` argument was typed as a list of non-null strings.
#[test]
fn an_arguments_list_items_are_typed_as_declared_in_every_client() {
    let mut schema = CompiledSchema::new();
    let mut user = TypeDefinition::new("User", "v_user");
    user.fields.push(FieldDefinition::new("id", FieldType::Id));
    schema.types.push(user);
    let mut tagged = QueryDefinition::new("tagged", "User").returning_list();
    tagged
        .arguments
        .push(ArgumentDefinition::new("tags", FieldType::list_of_nullable(FieldType::String)));
    tagged
        .arguments
        .push(ArgumentDefinition::new("ids", FieldType::list(FieldType::Id)));
    schema.queries.push(tagged);

    let source =
        |files: &crate::Generated, name: &str| files.get(&PathBuf::from(name)).unwrap().clone();
    let ts = source(&super::typescript::generate(&schema).unwrap(), "queries.ts");
    assert!(ts.contains("tags: (string | null)[]"), "{ts}");
    assert!(ts.contains("ids: string[]"), "{ts}");
    let go = source(&super::go::generate(&schema).unwrap(), "queries.go");
    assert!(go.contains("tags []*string"), "{go}");
    assert!(go.contains("ids []string"), "{go}");
    let py = source(&super::python::generate(&schema).unwrap(), "queries.py");
    assert!(py.contains("tags: list[str | None]"), "{py}");
    assert!(py.contains("ids: list[str]"), "{py}");
    let rs = source(&super::rust::generate(&schema).unwrap(), "queries.rs");
    assert!(rs.contains("tags: Vec<Option<String>>"), "{rs}");
    assert!(rs.contains("ids: Vec<String>"), "{rs}");
}

/// A schema with one fact table, `tf_sales`, loaded the way `generate-client` loads one, so
/// its aggregate row and its two roots are built.
fn fact_table_schema() -> CompiledSchema {
    CompiledSchema::from_json(include_str!("../../tests/fixtures/fact_table_schema.json"), false)
        .unwrap()
}

#[test]
fn a_fact_tables_aggregate_row_is_an_object_type_of_the_types_module() {
    let schema = fact_table_schema();
    let ctx = super::common::SchemaCtx::new(&schema);

    assert_eq!(ctx.module_of("SalesAggregateRow"), Some("types"));
}

#[test]
fn an_aggregate_root_selects_every_field_of_its_row_and_a_window_root_none() {
    let schema = fact_table_schema();
    let ctx = super::common::SchemaCtx::new(&schema);
    let roots = super::common::fact_roots(&schema);
    let names: Vec<&str> = roots.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["sales_aggregate", "sales_window"]);

    let row = schema.derived.fact_rows.iter().find(|t| t.name == "SalesAggregateRow").unwrap();
    let expected: String = std::iter::once("__typename")
        .chain(row.fields.iter().map(|f| f.name.as_str()))
        .fold(String::new(), |mut out, name| {
            out.push_str("    ");
            out.push_str(name);
            out.push('\n');
            out
        });
    assert_eq!(super::common::fact_root_selection(&ctx, &roots[0]), expected);
    assert_eq!(super::common::fact_root_selection(&ctx, &roots[1]), "");
}

#[test]
fn fact_table_roots_are_query_roots_and_a_schema_never_built_has_none() {
    let schema = fact_table_schema();
    assert!(super::common::has_query_roots(&schema));

    let unbuilt = CompiledSchema {
        fact_tables: schema.fact_tables,
        ..CompiledSchema::default()
    };
    assert!(super::common::fact_roots(&unbuilt).is_empty());
    assert!(!super::common::has_query_roots(&unbuilt));
}

/// The `TypeScript` file `name` generated from `schema`.
fn ts_file(schema: &CompiledSchema, name: &str) -> String {
    super::typescript::generate(schema)
        .unwrap()
        .remove(&PathBuf::from(name))
        .unwrap()
}

#[test]
fn typescript_emits_a_fact_tables_aggregate_row_as_an_interface() {
    let types = ts_file(&fact_table_schema(), "types.ts");

    assert!(types.contains("export interface SalesAggregateRow {"), "{types}");
    // A `numeric` measure's sum is an exact `Decimal`, a string on the wire.
    assert!(types.contains("  revenue_sum: string | null;"), "{types}");
}

#[test]
fn typescript_emits_a_typed_operation_for_each_fact_table_root() {
    let queries = ts_file(&fact_table_schema(), "queries.ts");

    assert!(queries.contains("export async function sales_aggregate("), "{queries}");
    assert!(
        queries.contains("aggregates?: SalesAggregateSelection[] | null"),
        "the arguments are typed by the published inputs:\n{queries}"
    );
    assert!(queries.contains("): Promise<SalesAggregateRow[]> {"), "{queries}");
    assert!(queries.contains("export async function sales_window("), "{queries}");
    assert!(queries.contains("windows?: WindowSpec[] | null"), "{queries}");
    assert!(queries.contains("): Promise<unknown[]> {"), "a window row is JSON:\n{queries}");
    assert!(
        queries.contains("import type { SalesAggregateRow, Store } from \"./types\";"),
        "{queries}"
    );
}

/// Validate `document`, the one a generator emits for fact-table root `root`, the way the
/// server validates a request, through core's public validators, then by the nullability
/// rule below. Returns the first refusal.
fn validate_fact_root_document(
    schema: &CompiledSchema,
    root: &fraiseql_core::schema::fact_table_surface::FactRoot,
    document: &str,
) -> Result<(), String> {
    use fraiseql_core::{graphql, runtime};

    let parsed = graphql::parse_query(document).map_err(|e| format!("parse: {e}"))?;
    let op = parsed.operation_name.as_deref();
    let field = parsed.selections.first().ok_or("no root field")?;
    if field.name != root.name {
        return Err(format!("the document calls `{}`, not `{}`", field.name, root.name));
    }
    runtime::validate_variable_types(schema, schema, op, &parsed.variables, false)
        .map_err(|e| e.to_string())?;
    let defined: Vec<String> = parsed.variables.iter().map(|v| v.name.clone()).collect();
    let referenced = runtime::collect_variable_references(&parsed).map_err(|e| e.to_string())?;
    runtime::validate_variable_uses(op, &defined, &referenced, false).map_err(|e| e.to_string())?;
    runtime::validate_variables_used(op, &defined, &referenced).map_err(|e| e.to_string())?;
    runtime::validate_argument_values(
        &format!("Query.{}", root.name),
        &root.arguments,
        &field.arguments,
        &parsed.variables,
    )
    .map_err(|e| e.to_string())?;
    if let Some(row) = &root.row_type {
        graphql::validate_selection_set(schema, row, &field.nested_fields)
            .map_err(|e| e.to_string())?;
        // The selection is the row's own fields: each selected field is checked against the
        // published row.
        let row = schema.derived.fact_rows.iter().find(|t| &t.name == row).unwrap();
        if let Some(unknown) = field
            .nested_fields
            .iter()
            .find(|f| f.name != "__typename" && !row.fields.iter().any(|r| r.name == f.name))
        {
            return Err(format!("`{}` is no field of `{}`", unknown.name, row.name));
        }
    }
    for (name, declared) in declared_variable_types(document) {
        let argument = root
            .arguments
            .iter()
            .find(|a| a.name == name)
            .ok_or_else(|| format!("`${name}` names no argument of `{}`", root.name))?;
        if !variable_allowed_at(&declared, &argument.arg_type, argument.nullable) {
            return Err(format!(
                "`${name}` is declared `{declared}`, which `{}` at a `{}` position does not \
                 allow",
                argument.name,
                argument.arg_type.to_graphql_string()
            ));
        }
    }
    Ok(())
}

/// Each variable an operation declares, with its type as the document writes it.
///
/// Read from the text, not from core's parse: core's `GraphQLType` keeps no item
/// nullability (`[X]` and `[X!]` parse alike), which is the half of a list declaration
/// the rule below needs.
fn declared_variable_types(document: &str) -> Vec<(String, String)> {
    let header = document.lines().next().unwrap_or_default();
    let Some(open) = header.find('(') else {
        return Vec::new();
    };
    let close = header.rfind(')').unwrap();
    header[open + 1..close]
        .split(", ")
        .map(|decl| {
            let (name, declared) = decl.split_once(": ").unwrap();
            (name.trim_start_matches('$').to_string(), declared.to_string())
        })
        .collect()
}

/// GraphQL § 5.8.5's nullability half: a variable may be used at a position only when its
/// declared type is at least as strict, at the position and at each list level. Core does
/// not check this yet (#1617); this rule stands in for that check here, and is replaced by
/// core's when #1617 is fixed.
fn variable_allowed_at(declared: &str, position: &FieldType, position_nullable: bool) -> bool {
    let (inner, non_null) =
        declared.strip_suffix('!').map_or((declared, false), |inner| (inner, true));
    if !non_null && !position_nullable {
        return false;
    }
    match position {
        FieldType::List {
            item,
            nullable_items,
        } => inner
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
            .is_some_and(|item_declared| variable_allowed_at(item_declared, item, *nullable_items)),
        other => !inner.starts_with('[') && inner == innermost_name(other),
    }
}

/// The named type inside a field type's list and non-null wrappers.
fn innermost_name(field_type: &FieldType) -> String {
    field_type
        .to_graphql_string()
        .trim_matches(|c| matches!(c, '[' | ']' | '!'))
        .to_string()
}

/// The documents a generator emitted for each fact-table root, extracted from its source by
/// `extract`, which is handed the root's name.
fn assert_fact_root_documents_validate(
    schema: &CompiledSchema,
    language: &str,
    extract: impl Fn(&str) -> String,
) {
    let roots = super::common::fact_roots(schema);
    assert!(!roots.is_empty(), "the fixture publishes no fact-table root");
    for root in roots {
        let document = extract(&root.name);
        if let Err(refusal) = validate_fact_root_document(schema, root, &document) {
            panic!("{language} `{}`: {refusal}\n{document}", root.name);
        }
    }
}

/// The text between `open` and the next `close` after it, in `source`.
fn between(source: &str, open: &str, close: &str) -> String {
    let start =
        source.find(open).unwrap_or_else(|| panic!("no `{open}` in:\n{source}")) + open.len();
    let end = source[start..].find(close).unwrap() + start;
    source[start..end].to_string()
}

/// The validator itself: each refusal it is relied on for, against a document no generator
/// wrote. A stricter variable than the position is allowed (§ 5.8.5), a looser one is not,
/// at the list's items too.
#[test]
fn the_fact_root_document_validator_refuses_what_the_server_would_and_what_core_cannot_yet() {
    let schema = fact_table_schema();
    let root = schema.fact_root("sales_aggregate").unwrap();
    let check = |doc: &str| validate_fact_root_document(&schema, root, doc);
    let ok = "query sales_aggregate($aggregates: [SalesAggregateSelection!], $limit: Int) {\n  \
              sales_aggregate(aggregates: $aggregates, limit: $limit) {\n    count\n  }\n}";
    check(ok).unwrap();
    check(&ok.replace("$limit: Int)", "$limit: Int!)")).expect("a stricter variable is allowed");
    let loose = check(&ok.replace("[SalesAggregateSelection!]", "[SalesAggregateSelection]"));
    assert!(
        loose.unwrap_err().contains("does not allow"),
        "a nullable item at `[X!]` is refused"
    );
    assert!(check(&ok.replace("Int)", "Nope)")).is_err(), "an unknown variable type");
    assert!(check(&ok.replace(", limit: $limit", "")).is_err(), "a variable never used");
    assert!(check(&ok.replace("count", "nope")).is_err(), "a field the row does not have");
}

#[test]
fn typescript_fact_root_documents_validate_against_the_schema() {
    let schema = fact_table_schema();
    let queries = ts_file(&schema, "queries.ts");
    assert_fact_root_documents_validate(&schema, "TypeScript", |root| {
        let constant = super::common::const_name(root);
        between(&queries, &format!("const {constant} = `"), "`")
    });
}

/// The Python file `name` generated from `schema`.
fn py_file(schema: &CompiledSchema, name: &str) -> String {
    super::python::generate(schema).unwrap().remove(&PathBuf::from(name)).unwrap()
}

#[test]
fn python_emits_a_fact_tables_aggregate_row_as_a_typed_dict() {
    let types = py_file(&fact_table_schema(), "types.py");

    assert!(types.contains("SalesAggregateRow = TypedDict("), "{types}");
    assert!(types.contains("\"revenue_sum\": str | None,"), "{types}");
}

#[test]
fn python_emits_a_typed_operation_for_each_fact_table_root() {
    let queries = py_file(&fact_table_schema(), "queries.py");

    assert!(queries.contains("def sales_aggregate("), "{queries}");
    assert!(
        queries.contains("    aggregates: list[SalesAggregateSelection] | None = None,"),
        "the arguments are typed by the published inputs:\n{queries}"
    );
    assert!(queries.contains(") -> list[SalesAggregateRow]:"), "{queries}");
    assert!(queries.contains("def sales_window("), "{queries}");
    assert!(queries.contains(") -> list[Any]:"), "a window row is JSON:\n{queries}");
}

#[test]
fn python_fact_root_documents_validate_against_the_schema() {
    let schema = fact_table_schema();
    let queries = py_file(&schema, "queries.py");
    assert_fact_root_documents_validate(&schema, "Python", |root| {
        let constant = super::common::const_name(root);
        between(&queries, &format!("_{constant} = \"\"\""), "\"\"\"")
    });
}

/// The Go file `name` generated from `schema`.
fn go_file(schema: &CompiledSchema, name: &str) -> String {
    super::go::generate(schema).unwrap().remove(&PathBuf::from(name)).unwrap()
}

#[test]
fn go_emits_a_fact_tables_aggregate_row_as_a_struct() {
    let types = go_file(&fact_table_schema(), "types.go");

    assert!(types.contains("type SalesAggregateRow struct {"), "{types}");
    assert!(types.contains("`json:\"revenue_sum\"`"), "{types}");
}

#[test]
fn go_emits_a_typed_operation_for_each_fact_table_root() {
    let queries = go_file(&fact_table_schema(), "queries.go");

    // Named as `go_export` names every operation: the root's name, its first letter upper.
    assert!(queries.contains("func (c *Client) Sales_aggregate("), "{queries}");
    assert!(
        queries.contains("aggregates []SalesAggregateSelection"),
        "the arguments are typed by the published inputs:\n{queries}"
    );
    assert!(queries.contains(") ([]SalesAggregateRow, error) {"), "{queries}");
    assert!(queries.contains("func (c *Client) Sales_window("), "{queries}");
    assert!(queries.contains(") ([]any, error) {"), "a window row is JSON:\n{queries}");
}

#[test]
fn go_fact_root_documents_validate_against_the_schema() {
    let schema = fact_table_schema();
    let queries = go_file(&schema, "queries.go");
    // Each document is a raw string constant opening on its operation, whose name is the
    // root's: `const …Document = \`query sales_aggregate(…`.
    assert_fact_root_documents_validate(&schema, "Go", |root| {
        let body = between(&queries, &format!("Document = `query {root}"), "`");
        format!("query {root}{body}")
    });
}

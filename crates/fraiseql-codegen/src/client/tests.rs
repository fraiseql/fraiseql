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

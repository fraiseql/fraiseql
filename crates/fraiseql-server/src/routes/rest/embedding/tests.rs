//! Tests for the `embedding` module.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use fraiseql_core::schema::{
    Cardinality, CompiledSchema, FieldDefinition, FieldType, Relationship, TypeDefinition,
};

use super::{
    executor::{
        declared_key, extract_join_key, extract_query_data, find_list_query_for_type,
        parent_join_column, set_empty_embedding, target_join_column,
    },
    project_missing_join_keys, required_join_keys, strip_projected_keys,
};
use crate::routes::rest::params::EmbeddedSpec;

fn schema_declaring(type_name: &str, fields: &[&str]) -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut td = TypeDefinition::new(type_name, "v_post");
    td.fields = fields
        .iter()
        .map(|f| FieldDefinition::new(*f, FieldType::parse("ID")))
        .collect();
    schema.types.push(td);
    schema
}

fn rel(name: &str, target: &str, cardinality: Cardinality) -> Relationship {
    Relationship {
        name: name.to_string(),
        target_type: target.to_string(),
        cardinality,
        foreign_key: "fk_user".to_string(),
        referenced_key: "id".to_string(),
    }
}

fn embed(relationship: &str) -> EmbeddedSpec {
    EmbeddedSpec {
        relationship: relationship.to_string(),
        rename:       None,
        fields:       Vec::new(),
    }
}

mod declared_spelling {
    use super::{Cardinality, declared_key, extract_join_key, rel, schema_declaring};

    /// **The collision this exists to prevent.** `[[relationships]]` names SQL
    /// columns (`fk_user`), but since 2.15.0 `where` accepts only the declared
    /// spelling. Handing the raw column to the parser would make the server
    /// refuse its own parent-scoping predicate, and every embedded list would
    /// come back empty — or worse, unscoped.
    #[test]
    fn a_storage_column_is_translated_to_the_declared_spelling() {
        let schema = schema_declaring("Post", &["id", "fkUser"]);
        assert_eq!(declared_key(&schema, "Post", "fk_user"), "fkUser");
    }

    /// A schema that declares the column as-written keeps it — the rule is
    /// "use what is declared", not "`camelCase` everything".
    #[test]
    fn a_column_the_schema_declares_verbatim_is_unchanged() {
        let schema = schema_declaring("Post", &["id", "fk_user"]);
        assert_eq!(declared_key(&schema, "Post", "fk_user"), "fk_user");
    }

    /// Fail-open: an unknown type, or one declaring nothing that matches, is
    /// passed through rather than guessed at (#939).
    #[test]
    fn an_unnameable_target_passes_the_column_through() {
        let schema = schema_declaring("Post", &["id"]);
        assert_eq!(declared_key(&schema, "Post", "fk_user"), "fk_user");
        assert_eq!(declared_key(&schema, "Nonexistent", "fk_user"), "fk_user");
    }

    /// The **parent** side obeys the same rule, and used not to: the projected row
    /// is keyed by the declared name, so a schema under `camelCase` handed
    /// `extract_join_key` a `fk_user` that no row ever carries. The embed then
    /// resolved to null even for a client that *had* selected the key.
    #[test]
    fn a_parent_row_is_read_under_the_declared_spelling() {
        let schema = schema_declaring("Post", &["id", "fkUser"]);
        let row = serde_json::json!({"id": 3, "fkUser": 7});

        assert_eq!(
            extract_join_key(&schema, "Post", &row, &rel("author", "User", Cardinality::ManyToOne)),
            Some(serde_json::json!(7)),
            "the relationship names the column `fk_user`; the row carries `fkUser`"
        );
    }
}

#[test]
fn extract_join_key_one_to_many() {
    let schema = schema_declaring("User", &["pk_user", "name"]);
    let mut rel = rel("posts", "Post", Cardinality::OneToMany);
    rel.referenced_key = "pk_user".to_string();
    let row = serde_json::json!({"pk_user": 42, "name": "Alice"});
    assert_eq!(extract_join_key(&schema, "User", &row, &rel), Some(serde_json::json!(42)));
}

#[test]
fn extract_join_key_many_to_one() {
    let schema = schema_declaring("Post", &["fk_user", "title"]);
    let row = serde_json::json!({"fk_user": 7, "title": "Hello"});
    assert_eq!(
        extract_join_key(&schema, "Post", &row, &rel("author", "User", Cardinality::ManyToOne)),
        Some(serde_json::json!(7))
    );
}

#[test]
fn extract_join_key_null_returns_none() {
    let schema = schema_declaring("Post", &["fk_user", "title"]);
    let row = serde_json::json!({"fk_user": null, "title": "Hello"});
    assert!(
        extract_join_key(&schema, "Post", &row, &rel("author", "User", Cardinality::ManyToOne))
            .is_none()
    );
}

#[test]
fn extract_join_key_missing_field_returns_none() {
    let schema = schema_declaring("User", &["pk_user", "name"]);
    let mut rel = rel("posts", "Post", Cardinality::OneToMany);
    rel.referenced_key = "pk_user".to_string();
    let row = serde_json::json!({"name": "Alice"});
    assert!(extract_join_key(&schema, "User", &row, &rel).is_none());
}

// ---------------------------------------------------------------------------
// #1230 — the projection carries the join key, and the response does not
// ---------------------------------------------------------------------------

/// The two sides of a relationship, stated once each. `ManyToOne` reads the parent's
/// **foreign key** — the column a client asking for `author` has no reason to select,
/// which is what made #1230 invisible on the `OneToMany` direction.
#[test]
fn the_parent_and_target_columns_are_opposite_ends_of_the_same_relationship() {
    let many_to_one = rel("author", "User", Cardinality::ManyToOne);
    assert_eq!(parent_join_column(&many_to_one), "fk_user");
    assert_eq!(target_join_column(&many_to_one), "id");

    let one_to_many = rel("posts", "Post", Cardinality::OneToMany);
    assert_eq!(parent_join_column(&one_to_many), "id");
    assert_eq!(target_join_column(&one_to_many), "fk_user");

    let one_to_one = rel("profile", "Profile", Cardinality::OneToOne);
    assert_eq!(parent_join_column(&one_to_one), "fk_user");
    assert_eq!(target_join_column(&one_to_one), "id");
}

/// #1230: `?select=id,author(name)` must project `fk_user`, because the embed is
/// resolved by reading that key off the parent row that comes back.
#[test]
fn a_many_to_one_embed_requires_the_foreign_key_in_the_parent_projection() {
    let mut schema = schema_declaring("Post", &["id", "fk_user", "title"]);
    schema.types[0].relationships = vec![rel("author", "User", Cardinality::ManyToOne)];

    assert_eq!(required_join_keys(&schema, "Post", &[embed("author")], &[]), vec!["fk_user"]);
}

/// A count reads the identical key, so `?select=name,posts.count` needs it too —
/// it counted zero for every parent otherwise.
#[test]
fn a_count_requires_the_same_key_an_embed_would() {
    let mut schema = schema_declaring("User", &["id", "name"]);
    schema.types[0].relationships = vec![rel("posts", "Post", Cardinality::OneToMany)];

    assert_eq!(
        required_join_keys(&schema, "User", &[], &["posts".to_string()]),
        vec!["id"],
        "the OneToMany parent side is `referenced_key`"
    );
}

/// Two relationships over one column, and an embed plus a count over the same
/// relationship, ask for the key once.
#[test]
fn a_key_two_selections_share_is_required_once() {
    let mut schema = schema_declaring("User", &["id", "name"]);
    schema.types[0].relationships = vec![
        rel("posts", "Post", Cardinality::OneToMany),
        rel("comments", "Comment", Cardinality::OneToMany),
    ];

    assert_eq!(
        required_join_keys(
            &schema,
            "User",
            &[embed("posts"), embed("comments")],
            &["posts".to_string()],
        ),
        vec!["id"]
    );
}

/// The required key is the **declared** spelling, since that is what the projection
/// and the returned row both speak.
#[test]
fn the_required_key_is_the_declared_spelling() {
    let mut schema = schema_declaring("Post", &["id", "fkUser"]);
    schema.types[0].relationships = vec![rel("author", "User", Cardinality::ManyToOne)];

    assert_eq!(required_join_keys(&schema, "Post", &[embed("author")], &[]), vec!["fkUser"]);
}

/// An unknown relationship or type yields nothing: refusing the request is the
/// parameter extractor's job, and inventing a projection key here would turn a clean
/// 400 into a confusing one.
#[test]
fn an_unknown_relationship_or_type_requires_nothing() {
    let mut schema = schema_declaring("Post", &["id", "fk_user"]);
    schema.types[0].relationships = vec![rel("author", "User", Cardinality::ManyToOne)];

    assert!(required_join_keys(&schema, "Post", &[embed("nope")], &[]).is_empty());
    assert!(required_join_keys(&schema, "Nonexistent", &[embed("author")], &[]).is_empty());
}

/// Only what was missing is added, and only what was added is reported — the report
/// is what gets stripped, so a key the client selected must not appear in it.
#[test]
fn only_the_keys_the_client_did_not_select_are_added_and_reported() {
    let mut projection = vec!["id".to_string(), "fk_user".to_string()];
    let added =
        project_missing_join_keys(&mut projection, &["fk_user".to_string(), "fk_team".to_string()]);

    assert_eq!(added, vec!["fk_team"], "`fk_user` was already selected by the client");
    assert_eq!(projection, vec!["id", "fk_user", "fk_team"]);
}

#[test]
fn a_projection_already_carrying_every_key_is_untouched() {
    let mut projection = vec!["id".to_string()];
    assert!(project_missing_join_keys(&mut projection, &["id".to_string()]).is_empty());
    assert_eq!(projection, vec!["id"]);
}

/// The strip runs over both response shapes — a list read and a single-resource GET.
#[test]
fn stripping_removes_the_named_keys_from_rows_and_from_a_single_object() {
    let keys = vec!["fk_user".to_string()];

    let mut rows = serde_json::json!([
        {"id": 1, "fk_user": 7, "author": {"name": "alice"}},
        {"id": 2, "fk_user": null, "author": null},
    ]);
    strip_projected_keys(&mut rows, &keys);
    assert_eq!(
        rows,
        serde_json::json!([
            {"id": 1, "author": {"name": "alice"}},
            {"id": 2, "author": null},
        ]),
        "including the row whose key was NULL — the client asked for neither"
    );

    let mut single = serde_json::json!({"id": 1, "fk_user": 7});
    strip_projected_keys(&mut single, &keys);
    assert_eq!(single, serde_json::json!({"id": 1}));
}

/// Nothing to strip, nothing touched — the path every request without an embed
/// takes.
#[test]
fn stripping_nothing_leaves_the_document_alone() {
    let mut rows = serde_json::json!([{"id": 1, "fk_user": 7}]);
    strip_projected_keys(&mut rows, &[]);
    assert_eq!(rows, serde_json::json!([{"id": 1, "fk_user": 7}]));

    let mut scalar = serde_json::json!(3);
    strip_projected_keys(&mut scalar, &["fk_user".to_string()]);
    assert_eq!(scalar, serde_json::json!(3));
}

#[test]
fn set_empty_embedding_one_to_many() {
    let mut row = serde_json::json!({"id": 1});
    set_empty_embedding(&mut row, "posts", Cardinality::OneToMany);
    assert_eq!(row["posts"], serde_json::json!([]));
}

#[test]
fn set_empty_embedding_many_to_one() {
    let mut row = serde_json::json!({"id": 1});
    set_empty_embedding(&mut row, "author", Cardinality::ManyToOne);
    assert!(row["author"].is_null());
}

#[test]
fn set_empty_embedding_one_to_one() {
    let mut row = serde_json::json!({"id": 1});
    set_empty_embedding(&mut row, "profile", Cardinality::OneToOne);
    assert!(row["profile"].is_null());
}

#[test]
fn extract_query_data_standard_envelope() {
    let parsed = serde_json::json!({
        "data": {
            "posts": [
                {"id": 1, "title": "Hello"},
                {"id": 2, "title": "World"},
            ]
        }
    });
    let data = extract_query_data(&parsed, "posts").unwrap();
    assert!(data.is_array());
    assert_eq!(data.as_array().unwrap().len(), 2);
}

#[test]
fn extract_query_data_missing_query_returns_none() {
    let parsed = serde_json::json!({"data": {}});
    assert!(extract_query_data(&parsed, "posts").is_none());
}

#[test]
fn find_list_query_for_type_returns_list_query() {
    use fraiseql_core::schema::{CompiledSchema, QueryDefinition};

    let mut schema = CompiledSchema::default();
    schema.queries.push(QueryDefinition {
        name: "post".to_string(),
        return_type: "Post".to_string(),
        returns_list: false,
        ..QueryDefinition::new("post", "Post")
    });
    schema.queries.push(QueryDefinition {
        name: "posts".to_string(),
        return_type: "Post".to_string(),
        returns_list: true,
        ..QueryDefinition::new("posts", "Post")
    });

    let found = find_list_query_for_type(&schema, "Post");
    assert!(found.is_some());
    assert_eq!(found.unwrap().name, "posts");
}

#[test]
fn find_list_query_for_type_no_match() {
    let schema = fraiseql_core::schema::CompiledSchema::default();
    assert!(find_list_query_for_type(&schema, "Post").is_none());
}

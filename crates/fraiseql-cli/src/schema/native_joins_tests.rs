#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use fraiseql_core::schema::{Cardinality, Relationship, TypeDefinition};

use super::*;

fn reference(child: &str, parent: &str, foreign_key: &str, referenced_key: &str) -> TviewReference {
    TviewReference {
        child:          child.to_string(),
        parent:         parent.to_string(),
        foreign_key:    foreign_key.to_string(),
        referenced_key: referenced_key.to_string(),
    }
}

fn relationship(
    name: &str,
    target: &str,
    cardinality: Cardinality,
    foreign_key: &str,
) -> Relationship {
    Relationship {
        name: name.to_string(),
        target_type: target.to_string(),
        cardinality,
        foreign_key: foreign_key.to_string(),
        referenced_key: "id".to_string(),
        native_join: None,
    }
}

/// `Post` (tv_post) has `comments` (tv_comment); `Comment` has `author` (tv_user) and
/// `reviewer` (tv_user); `Tag` reads a plain view.
fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::default();
    let mut post = TypeDefinition::new("Post", "app.tv_post");
    post.relationships
        .push(relationship("comments", "Comment", Cardinality::OneToMany, "post_id"));
    post.relationships
        .push(relationship("tags", "Tag", Cardinality::OneToMany, "post_id"));
    let mut comment = TypeDefinition::new("Comment", "app.tv_comment");
    comment
        .relationships
        .push(relationship("author", "User", Cardinality::ManyToOne, "fk_author"));
    comment.relationships.push(relationship(
        "reviewer",
        "User",
        Cardinality::ManyToOne,
        "author_id",
    ));
    schema.types.push(post);
    schema.types.push(comment);
    schema.types.push(TypeDefinition::new("User", "app.tv_user"));
    schema.types.push(TypeDefinition::new("Tag", "app.v_tag"));
    schema
}

fn references() -> Vec<TviewReference> {
    vec![
        reference("app.tv_comment", "app.tv_post", "fk_post", "pk_post"),
        reference("app.tv_comment", "app.tv_user", "fk_author", "pk_user"),
        reference("app.tv_comment", "app.tv_user", "reviewed_by", "pk_user"),
    ]
}

fn join_of(schema: &CompiledSchema, type_name: &str, rel: &str) -> Option<(String, String)> {
    let r = schema
        .find_type(type_name)
        .unwrap()
        .relationships
        .iter()
        .find(|r| r.name == rel)?;
    r.native_join
        .as_ref()
        .map(|j| (j.foreign_key.clone(), j.referenced_key.clone()))
}

#[test]
fn a_list_of_children_joins_on_the_childs_only_reference_to_the_parent() {
    let mut schema = schema();
    apply_native_joins(&mut schema, &references());
    assert_eq!(join_of(&schema, "Post", "comments"), Some(("fk_post".into(), "pk_post".into())));
}

#[test]
fn of_two_references_the_one_the_relationship_names_is_taken() {
    let mut schema = schema();
    apply_native_joins(&mut schema, &references());
    assert_eq!(
        join_of(&schema, "Comment", "author"),
        Some(("fk_author".into(), "pk_user".into()))
    );
}

#[test]
fn of_two_references_none_named_is_no_proof() {
    let mut schema = schema();
    apply_native_joins(&mut schema, &references());
    assert_eq!(join_of(&schema, "Comment", "reviewer"), None);
}

#[test]
fn a_type_reading_a_plain_view_keeps_its_stored_keys() {
    let mut schema = schema();
    let given = apply_native_joins(&mut schema, &references());
    assert_eq!(join_of(&schema, "Post", "tags"), None);
    assert_eq!(given, 2, "comments and author");
}

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
    referenced_key: &str,
) -> Relationship {
    Relationship {
        name: name.to_string(),
        target_type: target.to_string(),
        cardinality,
        foreign_key: foreign_key.to_string(),
        referenced_key: referenced_key.to_string(),
        native_join: None,
    }
}

/// `Post` (tv_post) has `comments` and `editedComments` (tv_comment); `Comment` has `author`
/// and `reviewer` (tv_user); `Tag` reads a plain view. The registry proves `fk_post`,
/// `fk_author` and `reviewed_by`, not `fk_editor`.
fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::default();
    let mut post = TypeDefinition::new("Post", "app.tv_post");
    post.relationships.push(relationship(
        "comments",
        "Comment",
        Cardinality::OneToMany,
        "fk_post",
        "pk_post",
    ));
    post.relationships.push(relationship(
        "editedComments",
        "Comment",
        Cardinality::OneToMany,
        "fk_editor",
        "pk_post",
    ));
    post.relationships.push(relationship(
        "tags",
        "Tag",
        Cardinality::OneToMany,
        "fk_post",
        "pk_post",
    ));
    let mut comment = TypeDefinition::new("Comment", "app.tv_comment");
    comment.relationships.push(relationship(
        "author",
        "User",
        Cardinality::ManyToOne,
        "fk_author",
        "pk_user",
    ));
    comment.relationships.push(relationship(
        "reviewer",
        "User",
        Cardinality::ManyToOne,
        "reviewed_by",
        "id",
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

fn join_of(schema: &CompiledSchema, type_name: &str, rel: &str) -> Option<NativeJoin> {
    let r = schema
        .find_type(type_name)
        .unwrap()
        .relationships
        .iter()
        .find(|r| r.name == rel)?;
    r.native_join.clone()
}

#[test]
fn a_relationship_whose_columns_the_registry_proves_joins_on_them() {
    let mut schema = schema();
    apply_native_joins(&mut schema, &references());
    assert_eq!(
        join_of(&schema, "Post", "comments"),
        Some(NativeJoin {
            foreign_key:    "fk_post".into(),
            referenced_key: "pk_post".into(),
            child_view:     "app.tv_comment".into(),
            parent_view:    "app.tv_post".into(),
        })
    );
    assert_eq!(join_of(&schema, "Comment", "author").unwrap().foreign_key, "fk_author");
}

#[test]
fn a_relationship_on_a_column_no_reference_proves_gets_none_even_if_another_is_the_only_one() {
    // `fk_editor` holds no FOREIGN KEY: the one reference, `fk_post`, is another relationship.
    let mut schema = schema();
    apply_native_joins(&mut schema, &references());
    assert_eq!(join_of(&schema, "Post", "editedComments"), None);
}

#[test]
fn a_relationship_referencing_another_parent_column_gets_none() {
    // `reviewed_by` references `pk_user`; the relationship says `id`.
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

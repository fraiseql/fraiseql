//! A list query's `orderBy` is a list of `{field, direction}` keys, each direction one of the
//! six published `SortDirection` values, spelled as published. The field-to-direction object
//! form and any other spelling are refused, on the typed argument and on the `JSON` one a
//! schema falls back to when it cannot adjudicate the return type.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::sync::Arc;

use crate::{
    runtime::{
        Executor,
        executor::test_support::{MockAdapter, mock_user_results, test_schema},
    },
    schema::{CompiledSchema, FieldDefinition, FieldType, TypeDefinition},
};

const LISTED: &str = "`ASC`, `DESC`, `ASC_NULLS_FIRST`, `ASC_NULLS_LAST`, `DESC_NULLS_FIRST`, \
                      `DESC_NULLS_LAST`";

/// `users` with a `User` type, so `orderBy` is typed `[UserOrderByInput!]`.
fn typed() -> CompiledSchema {
    let mut schema = test_schema();
    schema.types.push(
        TypeDefinition::new("User", "v_user")
            .with_field(FieldDefinition::new("id", FieldType::Id))
            .with_field(FieldDefinition::new("name", FieldType::String)),
    );
    schema.build_indexes();
    schema
}

async fn refusal(schema: CompiledSchema, document: &str) -> String {
    Executor::new(schema, Arc::new(MockAdapter::new(mock_user_results())))
        .execute(document, None)
        .await
        .expect_err(document)
        .to_string()
}

#[tokio::test]
async fn the_object_form_is_refused_naming_its_key() {
    for schema in [typed(), test_schema()] {
        let message = refusal(schema, r#"{ users(orderBy: {name: "DESC"}) { id name } }"#).await;
        assert!(
            message.contains("orderBy item takes `field` and `direction`, got `name`"),
            "{message}"
        );
    }
}

#[tokio::test]
async fn a_direction_in_another_case_is_refused_where_no_enum_adjudicates_it() {
    let message = refusal(
        test_schema(),
        r#"{ users(orderBy: [{field: "name", direction: "desc"}]) { id name } }"#,
    )
    .await;
    assert!(
        message.contains(&format!("orderBy direction 'desc' must be one of {LISTED}")),
        "{message}"
    );
}

#[tokio::test]
async fn a_direction_that_is_not_a_string_is_refused() {
    let message =
        refusal(test_schema(), r#"{ users(orderBy: [{field: "name", direction: 1}]) { id } }"#)
            .await;
    assert!(
        message.contains(&format!("orderBy direction 1 must be one of {LISTED}")),
        "{message}"
    );
}

#[tokio::test]
async fn one_key_written_as_an_object_is_a_one_item_list() {
    for schema in [typed(), test_schema()] {
        let served = Executor::new(schema, Arc::new(MockAdapter::new(mock_user_results())))
            .execute(r#"{ users(orderBy: {field: "name", direction: DESC}) { id } }"#, None)
            .await
            .unwrap();
        assert!(served.get("data").is_some(), "{served}");
    }
}

//! A mutation document's variable declared nullable at a non-null argument (§ 5.8.5) is
//! refused for the document, through every entry, before any root writes. The control that
//! declares the published type is `variables_of_their_declared_types_are_written`.

use serde_json::json;

use super::assert_refused;

#[tokio::test]
async fn a_nullable_variable_at_a_required_mutation_argument_is_refused() {
    assert_refused(
        "mutation_variable_types_nullable",
        "mutation ($l: String) { recordReceipt(orderLabel: $l) { id } }",
        json!({ "l": "A-1" }),
        "Variable `$l` is declared as `String` but is used for argument `orderLabel` on \
         Mutation.recordReceipt, which has type `String!`",
    )
    .await;
}

/// The first root's declaration is right and the second's is not: nothing is written.
#[tokio::test]
async fn a_nullable_variable_at_a_later_root_refuses_the_whole_operation() {
    assert_refused(
        "mutation_variable_types_nullable_second",
        "mutation ($l: String!, $p: Int) { a: recordReceipt(orderLabel: $l) { id } \
         b: recordPage(page: $p) { id } }",
        json!({ "l": "A-1", "p": 2 }),
        "Variable `$p` is declared as `Int` but is used for argument `page` on \
         Mutation.recordPage, which has type `Int!`",
    )
    .await;
}

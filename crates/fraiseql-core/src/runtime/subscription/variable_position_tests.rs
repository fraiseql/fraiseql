#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

//! A subscription's variable declared nullable at a non-null argument (§ 5.8.5) is refused at
//! subscribe, for the document.

use super::manager_for;
use crate::schema::{ArgumentDefinition, FieldType, SubscriptionDefinition};

fn subscribe(
    document: &str,
    variables: &serde_json::Value,
) -> crate::error::Result<serde_json::Value> {
    let manager = manager_for(
        SubscriptionDefinition::new("orderShipped", "Order")
            .with_topic("order_shipped")
            .with_argument(ArgumentDefinition::new("region", FieldType::String)),
    );
    manager.subscription_arguments(&crate::graphql::parse_query(document).unwrap(), Some(variables))
}

#[test]
fn a_nullable_variable_at_a_required_subscription_argument_is_refused() {
    let variables = serde_json::json!({ "r": "EU" });

    let refused =
        subscribe("subscription ($r: String) { orderShipped(region: $r) { id } }", &variables);
    let accepted =
        subscribe("subscription ($r: String!) { orderShipped(region: $r) { id } }", &variables);

    assert!(
        matches!(&refused, Err(crate::error::FraiseQLError::Validation { message, .. }) if message
            == "Variable `$r` is declared as `String` but is used for argument `region` on \
                Subscription.orderShipped, which has type `String!`"),
        "{refused:?}"
    );
    assert_eq!(accepted.unwrap(), serde_json::json!({ "region": "EU" }));
}

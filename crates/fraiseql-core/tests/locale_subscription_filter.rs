#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

//! #1525: a subscription filter on a localized field compares the event's **label** in the
//! subscriber's locale (the locale its plan was made in), not the stored locale map, which no
//! filter value equals.
//!
//! Two subscribers filter `productChanged(name: "Pomme")`, one planned in `fr-FR`, one in
//! `en-US`. An event whose `name` is `{"fr-FR": "Pomme", "en-US": "Apple"}` matches the first
//! and not the second, through `filter_fields`, `argument_paths` and a static filter alike.
//! Filters are evaluated in Rust at delivery, so no database is involved.

use std::{collections::BTreeMap, sync::Arc};

use fraiseql_core::{
    runtime::{
        Executor,
        subscription::{SubscriptionEvent, SubscriptionManager, SubscriptionOperation},
        with_request_locale_sync,
    },
    schema::{
        ArgumentDefinition, CompiledSchema, FieldDefinition, FieldType, FilterOperator,
        LocaleConfig, StaticFilterCondition, SubscriptionDefinition, SubscriptionFilter,
    },
};
use fraiseql_test_utils::{failing_adapter::FailingAdapter, schema_builder::TestTypeBuilder};
use serde_json::json;

/// `Product { id, name (localized) }`, and `productChanged` filtered by `filter`.
fn schema(subscription: SubscriptionDefinition) -> CompiledSchema {
    let mut name = FieldDefinition::nullable("name", FieldType::String);
    name.localized = true;
    let mut schema = CompiledSchema::new();
    schema.types.push(
        TestTypeBuilder::new("Product", "v_product")
            .with_simple_field("id", FieldType::Id)
            .with_field(name)
            .build(),
    );
    schema.subscriptions.push(subscription);
    schema.locale = Some(
        LocaleConfig::new("en-US", vec!["en-US".into(), "fr-FR".into()], BTreeMap::new(), vec![])
            .unwrap(),
    );
    schema.build_indexes();
    schema
}

fn subscription() -> SubscriptionDefinition {
    let mut definition = SubscriptionDefinition::new("productChanged", "Product");
    definition.arguments = vec![ArgumentDefinition::optional("name", FieldType::String)];
    definition
}

/// How many of a French and an English subscriber an event named `Pomme` in French and
/// `Apple` in English reaches, as `(french, english)`.
fn reached(definition: SubscriptionDefinition, variables: &serde_json::Value) -> (bool, bool) {
    let schema = schema(definition);
    let executor = Executor::new(schema.clone(), Arc::new(FailingAdapter::new()));
    let manager = SubscriptionManager::new(Arc::new(schema));
    let document = fraiseql_core::graphql::parse_query(
        "subscription($name: String) { productChanged(name: $name) { id } }",
    )
    .unwrap();
    let subscribe = |locale: &str| {
        let plan = with_request_locale_sync(Some(locale.to_string()), || {
            executor.plan_subscription(&document, Some(variables), None)
        })
        .unwrap();
        manager
            .subscribe_planned(Arc::new(plan), json!({}), variables.clone(), locale, vec![])
            .unwrap()
    };
    let french = subscribe("fr-FR");
    let english = subscribe("en-US");
    let mut receiver = manager.receiver();
    manager.publish_event(SubscriptionEvent::new(
        "Product",
        "p1",
        SubscriptionOperation::Update,
        json!({"id": "p1", "name": {"fr-FR": "Pomme", "en-US": "Apple"}}),
    ));
    let mut delivered = Vec::new();
    while let Ok(payload) = receiver.try_recv() {
        delivered.push(payload.subscription_id);
    }
    (delivered.contains(&french), delivered.contains(&english))
}

#[test]
fn a_filter_field_compares_the_label_in_the_subscribers_locale() {
    let mut definition = subscription();
    definition.filter_fields = vec!["name".to_string()];
    assert_eq!(reached(definition, &json!({"name": "Pomme"})), (true, false));
}

#[test]
fn an_argument_path_compares_the_label_in_the_subscribers_locale() {
    let definition = subscription().with_filter(SubscriptionFilter {
        argument_paths: std::iter::once(("name".to_string(), "/name".to_string())).collect(),
        static_filters: Vec::new(),
    });
    assert_eq!(reached(definition, &json!({"name": "Apple"})), (false, true));
}

#[test]
fn a_static_filter_compares_the_label_in_the_subscribers_locale() {
    let definition = subscription().with_filter(SubscriptionFilter {
        argument_paths: std::collections::HashMap::new(),
        static_filters: vec![StaticFilterCondition {
            path:     "/name".to_string(),
            operator: FilterOperator::Eq,
            value:    json!("Pomme"),
        }],
    });
    assert_eq!(reached(definition, &json!({})), (true, false));
}

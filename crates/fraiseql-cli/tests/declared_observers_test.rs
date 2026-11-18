//! An observer declared in the authoring schema is compiled into the schema the server
//! boots from, in the shape the observer runtime reads, or the compile refuses it by name.
//!
//! The runtime reads an action through `fraiseql_observers::ActionConfig`, which ignores a
//! key it does not hold. A declaration is therefore checked against that type here: a
//! misspelt key would otherwise compile, be stored at boot, and deliver nothing.
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use fraiseql_cli::schema::{SchemaConverter, intermediate::IntermediateSchema};
use fraiseql_core::schema::CompiledSchema;
use serde_json::{Value, json};

/// An `Order` schema with `[observers]` enabled, declaring `observers`.
fn schema_declaring(observers: &Value) -> Value {
    json!({
        "types": [
            {"name": "Customer", "sql_source": "v_customer", "fields": [
                {"name": "id", "type": "ID", "nullable": false},
                {"name": "email", "type": "String", "nullable": false}
            ]},
            {"name": "Order", "sql_source": "v_order", "fields": [
                {"name": "id", "type": "ID", "nullable": false},
                {"name": "total", "type": "Int", "nullable": false},
                {"name": "reviewStatus", "type": "String", "nullable": true},
                {"name": "customer", "type": "Customer", "nullable": true}
            ]}
        ],
        "queries": [
            {"name": "orders", "return_type": "Order", "returns_list": true,
             "sql_source": "v_order"}
        ],
        "observers_config": {"enabled": true},
        "observers": observers
    })
}

fn compile(schema: Value) -> anyhow::Result<CompiledSchema> {
    let intermediate: IntermediateSchema = serde_json::from_value(schema)?;
    SchemaConverter::convert(intermediate)
}

/// One observer on `Order` inserts with the given action.
fn observing_with(action: &Value) -> Value {
    json!([{
        "name": "order_created",
        "entity": "Order",
        "event": "INSERT",
        "actions": [action],
        "retry": {"max_attempts": 3, "backoff_strategy": "exponential",
                  "initial_delay_ms": 100, "max_delay_ms": 5000}
    }])
}

/// The refusal for one action on the `order_created` observer.
fn refusal_for(action: &Value) -> String {
    let err = compile(schema_declaring(&observing_with(action)))
        .expect_err("the declaration must be refused");
    format!("{err:#}")
}

#[test]
fn a_declared_observer_reaches_the_compiled_schema() {
    let action = json!({"type": "webhook", "url": "https://example.test/hook",
                        "body_template": "{\"total\": {{ total }}}"});
    let mut observers = observing_with(&action);
    observers[0]["condition"] = json!("total > 100");

    let compiled = compile(schema_declaring(&observers)).unwrap();

    let observer = compiled.find_observer("order_created").expect("the observer is compiled");
    assert_eq!(
        (observer.entity.as_str(), observer.event.as_str(), observer.condition.as_deref()),
        ("Order", "INSERT", Some("total > 100"))
    );
    assert_eq!(observer.actions, vec![action]);
    assert_eq!(
        (
            observer.retry.max_attempts,
            observer.retry.initial_delay_ms,
            observer.retry.max_delay_ms
        ),
        (3, 100, 5000)
    );
}

#[test]
fn every_compiled_action_is_one_the_runtime_reads() {
    let actions = json!([
        {"type": "webhook", "url_env": "ORDER_HOOK_URL", "signing_secret_env": "ORDER_HOOK_KEY"},
        {"type": "slack", "webhook_url_env": "SLACK_WEBHOOK_URL", "channel": "#orders",
         "message_template": "Order {{ id }}"},
        {"type": "email", "to": "ops@example.test", "subject": "New order",
         "body_template": "Order {{ id }}"},
        {"type": "database", "function_name": "app.fn_notify"},
        {"type": "log", "message_template": "Order {{ id }}"}
    ]);
    let mut observers = observing_with(&json!({}));
    observers[0]["actions"] = actions;

    let compiled = compile(schema_declaring(&observers)).unwrap();

    for action in &compiled.find_observer("order_created").unwrap().actions {
        let read: Result<fraiseql_observers::ActionConfig, _> =
            serde_json::from_value(action.clone());
        assert!(read.is_ok(), "the runtime cannot read {action}: {read:?}");
        let valid = read.unwrap().validate();
        assert!(valid.is_ok(), "the runtime refuses {action}: {valid:?}");
    }
}

#[test]
fn an_action_key_the_runtime_does_not_read_is_refused_naming_it() {
    let refusal = refusal_for(&json!({"type": "slack", "webhook_url_env": "SLACK_WEBHOOK_URL",
                                       "channel": "#orders", "message": "Order {{ id }}"}));

    assert!(refusal.contains("`order_created`"), "names the observer: {refusal}");
    assert!(refusal.contains("`message`"), "names the key: {refusal}");
    assert!(
        refusal.contains("message_template"),
        "lists the keys the action holds: {refusal}"
    );
}

#[test]
fn an_email_key_the_dispatcher_does_not_use_is_refused_naming_it() {
    for key in ["to_template", "subject_template", "reply_to"] {
        let mut action = json!({"type": "email", "to": "ops@example.test",
                                "subject": "New order", "body_template": "Order {{ id }}"});
        action[key] = json!("{{ email }}");

        let refusal = refusal_for(&action);

        assert!(refusal.contains("`order_created`"), "names the observer: {refusal}");
        assert!(refusal.contains(&format!("`{key}`")), "names the key: {refusal}");
    }
}

#[test]
fn an_action_the_runtime_would_refuse_is_refused_naming_the_observer() {
    let refusal = refusal_for(&json!({"type": "webhook"}));

    assert!(refusal.contains("`order_created`"), "names the observer: {refusal}");
    assert!(refusal.contains("'url' or 'url_env'"), "says what is missing: {refusal}");
}

#[test]
fn an_action_type_the_runtime_does_not_dispatch_is_refused_naming_it() {
    let refusal = refusal_for(&json!({"type": "sms", "to": "+33100000000"}));

    assert!(refusal.contains("`order_created`"), "names the observer: {refusal}");
    assert!(refusal.contains("sms"), "names the type: {refusal}");
}

#[test]
fn a_signing_secret_written_into_the_schema_is_refused_naming_the_key() {
    let refusal = refusal_for(&json!({"type": "webhook", "url": "https://example.test/hook",
                                       "signing_secret": "hunter2"}));

    assert!(refusal.contains("`order_created`"), "names the observer: {refusal}");
    assert!(refusal.contains("`signing_secret`"), "names the key: {refusal}");
    assert!(refusal.contains("signing_secret_env"), "names the way to sign: {refusal}");
    assert!(!refusal.contains("hunter2"), "the secret is not repeated: {refusal}");
}

#[test]
fn a_condition_the_runtime_cannot_parse_is_refused_naming_the_observer() {
    let mut observers = observing_with(&json!({"type": "webhook", "url": "https://example.test"}));
    observers[0]["condition"] = json!("total > 100 )");

    let refusal = format!("{:#}", compile(schema_declaring(&observers)).unwrap_err());

    assert!(refusal.contains("`order_created`"), "names the observer: {refusal}");
    assert!(refusal.contains("condition"), "names the key: {refusal}");
}

#[test]
fn a_condition_missing_part_of_a_comparison_or_call_is_refused_with_the_reason() {
    for (condition, reason) in [
        ("total >", "`total >` has nothing after the operator"),
        ("status == paid", "`status == 'paid'`"),
        ("total > 1.2.3", "`1.2.3` is not a number"),
        ("total > 'ten'", "`>` orders numbers, and `'ten'` is not one"),
        ("status == 'paid", "is never closed: add the closing `'`"),
        ("has_field('total'", "`has_field(` is never closed"),
    ] {
        let mut observers =
            observing_with(&json!({"type": "webhook", "url": "https://example.test"}));
        observers[0]["condition"] = json!(condition);

        let refusal = format!("{:#}", compile(schema_declaring(&observers)).unwrap_err());

        assert!(
            refusal.contains("`order_created`"),
            "{condition}: names the observer: {refusal}"
        );
        assert!(refusal.contains("`condition`"), "{condition}: names the key: {refusal}");
        assert!(refusal.contains(reason), "{condition}: says why: {refusal}");
    }
}

#[test]
fn a_retry_key_the_runtime_does_not_read_is_refused_naming_it() {
    let mut observers = observing_with(&json!({"type": "webhook", "url": "https://example.test"}));
    observers[0]["retry"] = json!({"max_attempts": 3, "backoff_strategy": "quadratic",
                                    "initial_delay_ms": 100, "max_delay_ms": 5000});

    let refusal = format!("{:#}", compile(schema_declaring(&observers)).unwrap_err());

    assert!(refusal.contains("`order_created`"), "names the observer: {refusal}");
    assert!(refusal.contains("retry"), "names the key: {refusal}");
    assert!(refusal.contains("quadratic"), "names the value: {refusal}");
}

#[test]
fn observers_declared_while_the_runtime_is_not_enabled_are_refused() {
    let mut schema = schema_declaring(&observing_with(
        &json!({"type": "webhook", "url": "https://example.test"}),
    ));
    schema.as_object_mut().unwrap().remove("observers_config");

    let refusal = format!("{:#}", compile(schema).unwrap_err());

    assert!(refusal.contains("order_created"), "names the observer: {refusal}");
    assert!(refusal.contains("[observers]"), "names the section to enable: {refusal}");
}

#[test]
fn a_webhook_template_naming_a_field_the_entity_does_not_hold_is_refused() {
    let refusal = refusal_for(&json!({"type": "webhook", "url": "https://example.test/hook",
                                       "body_template": "{\"t\": {{ totl }}}"}));

    assert!(refusal.contains("`order_created`"), "names the observer: {refusal}");
    assert!(refusal.contains("{{ totl }}"), "names the placeholder: {refusal}");
    assert!(refusal.contains("Order"), "names the entity: {refusal}");
}

#[test]
fn a_webhook_template_naming_a_field_by_its_published_name_is_refused_with_the_stored_key() {
    let refusal = refusal_for(&json!({"type": "webhook", "url": "https://example.test/hook",
                                       "body_template": "{\"s\": {{ reviewStatus }}}"}));

    assert!(refusal.contains("{{ reviewStatus }}"), "names the placeholder: {refusal}");
    assert!(
        refusal.contains("`review_status`"),
        "names the key the document holds: {refusal}"
    );
}

#[test]
fn a_webhook_template_reading_stored_keys_and_nested_fields_compiles() {
    let action = json!({"type": "webhook", "url": "https://example.test/hook",
        "body_template": "{\"s\": {{ review_status }}, \"e\": {{ customer.email }}}"});

    compile(schema_declaring(&observing_with(&action))).unwrap();
}

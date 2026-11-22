//! Observer authoring: an observer per action kind, as the compiler reads it.

use fraiseql_rust::{
    BackoffStrategy, EmailAction, Observer, ObserverEvent, ObserverRetry, SchemaRegistry,
    SlackAction, SlackDestination, WebhookAction, WebhookDestination,
};
use serde_json::{json, Value};

fn exported(registry: &SchemaRegistry) -> Value {
    serde_json::from_str(&registry.export_to_json()).expect("the export is JSON")
}

fn action_of(action: impl Into<fraiseql_rust::ObserverAction>) -> Value {
    let mut registry = SchemaRegistry::new();
    registry
        .register_observer(
            Observer::new("probe", "Order", ObserverEvent::Insert).with_action(action),
        )
        .expect("one observer registers");
    exported(&registry)["observers"][0]["actions"][0].clone()
}

#[test]
fn a_webhook_observer_carries_every_key_it_is_given() {
    let mut registry = SchemaRegistry::new();
    registry
        .register_observer(
            Observer::new("orderPlacedWebhook", "Order", ObserverEvent::Insert)
                .with_action(
                    WebhookAction::new(WebhookDestination::UrlEnv("HOOK_URL".to_string()))
                        .with_method("PUT")
                        .with_body_template(r#"{"total": {{ total }}}"#)
                        .with_signing_secret_env("HOOK_KEY")
                        .with_header("X-Source", "tests")
                        .with_header_env("Authorization", "HOOK_TOKEN"),
                )
                .with_retry(ObserverRetry::new(5, BackoffStrategy::Linear, 200, 5000)),
        )
        .expect("one observer registers");

    assert_eq!(
        exported(&registry)["observers"],
        json!([{
            "name": "orderPlacedWebhook", "entity": "Order", "event": "INSERT",
            "actions": [{
                "type": "webhook", "url_env": "HOOK_URL", "method": "PUT",
                "body_template": "{\"total\": {{ total }}}", "signing_secret_env": "HOOK_KEY",
                "headers": {"X-Source": "tests"}, "headers_env": {"Authorization": "HOOK_TOKEN"}
            }],
            "retry": {"max_attempts": 5, "backoff_strategy": "linear",
                      "initial_delay_ms": 200, "max_delay_ms": 5000}
        }])
    );
}

#[test]
fn a_webhook_given_only_its_url_authors_only_its_url() {
    assert_eq!(
        action_of(WebhookAction::new(WebhookDestination::Url(
            "https://hooks.example/x".to_string()
        ))),
        json!({"type": "webhook", "url": "https://hooks.example/x"})
    );
}

#[test]
fn a_slack_action_reads_the_default_env_unless_told_otherwise() {
    assert_eq!(
        action_of(SlackAction::new("#orders", "Order {{ id }} shipped")),
        json!({"type": "slack", "channel": "#orders", "message_template": "Order {{ id }} shipped",
               "webhook_url_env": "SLACK_WEBHOOK_URL"})
    );
    assert_eq!(
        action_of(
            SlackAction::new("#o", "m")
                .with_destination(SlackDestination::WebhookUrlEnv("OPS_SLACK".to_string()))
        ),
        json!({"type": "slack", "channel": "#o", "message_template": "m",
               "webhook_url_env": "OPS_SLACK"})
    );
    // A written URL is the one destination: the default env is not emitted beside it.
    assert_eq!(
        action_of(SlackAction::new("#o", "m").with_destination(SlackDestination::WebhookUrl(
            "https://hooks.slack.example/x".to_string()
        ))),
        json!({"type": "slack", "channel": "#o", "message_template": "m",
               "webhook_url": "https://hooks.slack.example/x"})
    );
}

#[test]
fn an_email_action_names_its_body_template() {
    assert_eq!(
        action_of(EmailAction::new("ops@example.com", "Order deleted", "Order {{ id }}")),
        json!({"type": "email", "to": "ops@example.com", "subject": "Order deleted",
               "body_template": "Order {{ id }}"})
    );
}

#[test]
fn a_condition_is_emitted_and_an_unset_retry_is_left_out() {
    let mut registry = SchemaRegistry::new();
    registry
        .register_observer(
            Observer::new("orderShipped", "Order", ObserverEvent::Update)
                .with_condition("status == 'shipped'")
                .with_action(EmailAction::new("ops@example.com", "s", "b")),
        )
        .expect("one observer registers");

    let definition = &exported(&registry)["observers"][0];
    assert_eq!(definition["event"], "UPDATE");
    assert_eq!(definition["condition"], "status == 'shipped'");
    // No retry: the server's own policy applies, so no block is written.
    assert!(definition.get("retry").is_none());
}

#[test]
fn every_event_and_backoff_is_written_as_the_compiler_spells_it() {
    let mut registry = SchemaRegistry::new();
    for (name, event, backoff) in [
        ("a", ObserverEvent::Insert, BackoffStrategy::Fixed),
        ("b", ObserverEvent::Update, BackoffStrategy::Linear),
        ("c", ObserverEvent::Delete, BackoffStrategy::Exponential),
    ] {
        registry
            .register_observer(
                Observer::new(name, "Order", event)
                    .with_action(EmailAction::new("a@example.com", "s", "b"))
                    .with_retry(ObserverRetry::new(1, backoff, 1, 1)),
            )
            .expect("distinct names register");
    }
    let spelled: Vec<(Value, Value)> = exported(&registry)["observers"]
        .as_array()
        .expect("an observers array")
        .iter()
        .map(|o| (o["event"].clone(), o["retry"]["backoff_strategy"].clone()))
        .collect();
    assert_eq!(
        spelled,
        [
            (json!("INSERT"), json!("fixed")),
            (json!("UPDATE"), json!("linear")),
            (json!("DELETE"), json!("exponential")),
        ]
    );
}

#[test]
fn no_observer_leaves_the_section_out() {
    assert!(exported(&SchemaRegistry::new()).get("observers").is_none());
}

#[test]
fn an_observer_name_is_registered_once() {
    let declare =
        || {
            Observer::new("orderCreated", "Order", ObserverEvent::Insert)
                .with_action(EmailAction::new("a@example.com", "s", "b"))
        };
    let mut registry = SchemaRegistry::new();
    registry.register_observer(declare()).expect("the first registers");
    let refused = registry.register_observer(declare()).expect_err("the second is refused");
    assert!(
        refused.to_string().contains("observer 'orderCreated' is already registered"),
        "{refused}"
    );
    assert_eq!(exported(&registry)["observers"].as_array().map(Vec::len), Some(1));
}

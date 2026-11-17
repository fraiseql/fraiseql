#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use serde_json::json;

use crate::config::ActionConfig;

fn refusal(action: &serde_json::Value) -> String {
    ActionConfig::from_authored(action).unwrap_err().to_string()
}

#[test]
fn an_action_of_every_dispatched_type_is_read() {
    for action in [
        json!({"type": "webhook", "url": "https://example.test/hook", "method": "PUT"}),
        json!({"type": "slack", "webhook_url": "https://example.test/slack",
               "message_template": "Order {{ id }}"}),
        json!({"type": "email", "to": "ops@example.test", "subject": "New order",
               "body_template": "Order {{ id }}"}),
        json!({"type": "database", "function_name": "app.fn_notify", "params": {"a": 1}}),
        json!({"type": "log", "message_template": "Order {{ id }}"}),
    ] {
        let read = ActionConfig::from_authored(&action);
        assert!(read.is_ok(), "{action} is refused: {read:?}");
    }
}

#[test]
fn a_key_the_type_does_not_hold_is_refused_naming_it_and_the_keys_it_holds() {
    let refusal =
        refusal(&json!({"type": "email", "to": "ops@example.test", "cc": "b@example.test",
                                   "subject": "New order", "body_template": "Order"}));

    assert!(refusal.contains("`cc`"), "{refusal}");
    assert!(refusal.contains("reply_to"), "lists the keys the action holds: {refusal}");
}

#[test]
fn a_key_the_type_does_not_hold_is_refused_even_when_it_is_null() {
    let refusal = refusal(&json!({"type": "slack", "webhook_url": "https://example.test/slack",
                                   "message": null}));

    assert!(refusal.contains("`message`"), "{refusal}");
}

#[test]
fn a_key_the_email_dispatcher_does_not_use_is_refused_naming_it() {
    for key in ["to_template", "subject_template", "reply_to"] {
        let mut action = json!({"type": "email", "to": "ops@example.test", "subject": "New order",
                                "body_template": "Order"});
        action[key] = json!("{{ email }}");

        let refusal = refusal(&action);

        assert!(refusal.contains(&format!("`{key}`")), "{refusal}");
    }
}

#[test]
fn a_held_key_left_null_is_not_a_request() {
    let action = json!({"type": "email", "to": "ops@example.test", "subject": "New order",
                        "body_template": "Order", "reply_to": null});

    assert!(ActionConfig::from_authored(&action).is_ok());
}

#[test]
fn an_action_validate_refuses_is_refused_with_its_reason() {
    assert!(refusal(&json!({"type": "webhook"})).contains("'url' or 'url_env'"));
}

#[test]
fn a_type_the_runtime_does_not_dispatch_is_refused_naming_it() {
    assert!(refusal(&json!({"type": "sms", "to": "+33100000000"})).contains("sms"));
}

#[test]
fn a_value_that_is_not_an_object_is_refused() {
    assert!(refusal(&json!("webhook")).contains("an object"));
}

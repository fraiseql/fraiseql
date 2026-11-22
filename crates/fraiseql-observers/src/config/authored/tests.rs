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
fn a_webhook_naming_both_destinations_is_refused_naming_the_fix() {
    let refusal = refusal(&json!({"type": "webhook", "url": "https://example.test/hook",
                                   "url_env": "HOOK_URL"}));

    assert!(refusal.contains("set one of `url` / `url_env`, not both"), "{refusal}");
}

#[test]
fn a_slack_action_naming_both_destinations_is_refused_naming_the_fix() {
    let refusal = refusal(&json!({"type": "slack", "webhook_url": "https://example.test/slack",
                                   "webhook_url_env": "SLACK_WEBHOOK_URL",
                                   "message_template": "Order {{ id }}"}));

    assert!(
        refusal.contains("set one of `webhook_url` / `webhook_url_env`, not both"),
        "{refusal}"
    );
}

#[test]
fn a_destination_left_null_beside_the_other_is_one_destination() {
    for action in [
        json!({"type": "webhook", "url": null, "url_env": "HOOK_URL"}),
        json!({"type": "webhook", "url": "https://example.test/hook", "url_env": null}),
        json!({"type": "slack", "webhook_url": null, "webhook_url_env": "SLACK_WEBHOOK_URL",
               "message_template": "m"}),
        json!({"type": "slack", "webhook_url": "https://example.test/slack",
               "webhook_url_env": null, "message_template": "m"}),
    ] {
        let read = ActionConfig::from_authored(&action);
        assert!(read.is_ok(), "{action} is refused: {read:?}");
    }
}

#[test]
fn a_slack_action_naming_neither_destination_is_refused() {
    assert!(
        refusal(&json!({"type": "slack", "message_template": "m"}))
            .contains("'webhook_url' or 'webhook_url_env'")
    );
}

#[test]
fn a_type_the_runtime_does_not_dispatch_is_refused_naming_it() {
    assert!(refusal(&json!({"type": "sms", "to": "+33100000000"})).contains("sms"));
}

#[test]
fn a_value_that_is_not_an_object_is_refused() {
    assert!(refusal(&json!("webhook")).contains("an object"));
}

#[test]
fn a_webhook_header_value_from_the_environment_is_read() {
    for headers_env in [
        json!({"Authorization": "HOOK_TOKEN"}),
        json!({}),
        json!(null),
    ] {
        let action = json!({"type": "webhook", "url": "https://example.test/hook",
                            "headers_env": headers_env});
        let read = ActionConfig::from_authored(&action);
        assert!(read.is_ok(), "{action} is refused: {read:?}");
    }
}

#[test]
fn a_webhook_headers_env_the_dispatcher_could_not_send_is_refused_naming_the_header() {
    for (headers, headers_env, named) in [
        (json!({}), json!({"Authorization": " "}), "Authorization"),
        (json!({}), json!({"Bad Header:": "HOOK_TOKEN"}), "Bad Header:"),
        (json!({"x-trace": "a"}), json!({"X-Trace": "TRACE"}), "X-Trace"),
    ] {
        let refusal = refusal(&json!({"type": "webhook", "url": "https://example.test/hook",
                                      "headers": headers, "headers_env": headers_env}));
        assert!(refusal.contains(named), "names `{named}`: {refusal}");
        assert!(refusal.contains("headers_env"), "names the key: {refusal}");
    }
}

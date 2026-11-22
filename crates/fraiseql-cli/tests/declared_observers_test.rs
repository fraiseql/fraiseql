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
fn an_action_naming_two_destinations_is_refused_naming_the_observer_action_and_fix() {
    for (action, fix) in [
        (
            json!({"type": "webhook", "url": "https://example.test/hook", "url_env": "HOOK_URL"}),
            "set one of `url` / `url_env`, not both",
        ),
        (
            json!({"type": "slack", "message_template": "Order {{ id }}",
                   "webhook_url": "https://example.test/slack",
                   "webhook_url_env": "SLACK_WEBHOOK_URL"}),
            "set one of `webhook_url` / `webhook_url_env`, not both",
        ),
    ] {
        // The second action is at fault, so the index is not the default 0.
        let observers = json!([{
            "name": "order_created", "entity": "Order", "event": "INSERT",
            "actions": [{"type": "webhook", "url_env": "OK_HOOK_URL"}, action]
        }]);
        let refusal = format!(
            "{:#}",
            compile(schema_declaring(&observers)).expect_err("the declaration must be refused")
        );

        assert!(refusal.contains("observer `order_created`, action 1"), "{refusal}");
        assert!(refusal.contains(fix), "{refusal}");
    }
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

/// The compiled retry of `order_created` declared with `retry` (`None`: no `retry` key).
fn compiled_retry(retry: Option<Value>) -> (u32, String, u32, u32) {
    let mut observers = observing_with(&json!({"type": "webhook", "url": "https://example.test"}));
    match retry {
        Some(retry) => observers[0]["retry"] = retry,
        None => {
            observers[0].as_object_mut().unwrap().remove("retry");
        },
    }
    let compiled = compile(schema_declaring(&observers)).unwrap();
    let retry = &compiled.find_observer("order_created").unwrap().retry;
    (
        retry.max_attempts,
        retry.backoff_strategy.clone(),
        retry.initial_delay_ms,
        retry.max_delay_ms,
    )
}

/// The policy the observer runtime runs when it is given none, read from its own crate.
fn runtime_policy() -> (u32, String, u32, u32) {
    (
        u32::from(fraiseql_observers::config::DEFAULT_RETRY_MAX_ATTEMPTS),
        fraiseql_observers::BackoffStrategy::default().as_str().to_string(),
        fraiseql_observers::config::DEFAULT_RETRY_INITIAL_DELAY_MS,
        fraiseql_observers::config::DEFAULT_RETRY_MAX_DELAY_MS,
    )
}

/// An observer authored without `retry` compiles to the policy the runtime runs when it
/// is given none, the one an observer created through the admin API without one gets.
#[test]
fn an_observer_without_retry_compiles_to_the_runtime_policy() {
    assert_eq!(compiled_retry(None), runtime_policy());
}

/// The same policy as literals, so a change to the runtime's defaults shows in review
/// here, not only as a moved constant.
#[test]
fn the_runtime_policy_is_three_exponential_attempts_from_one_second_to_one_minute() {
    assert_eq!(compiled_retry(None), (3, "exponential".to_string(), 1000, 60000));
}

/// A field left out of `retry` takes the runtime's value; the fields written are kept.
#[test]
fn a_retry_that_leaves_fields_out_takes_them_from_the_runtime_policy() {
    let (_, backoff, initial, max) = runtime_policy();
    assert_eq!(
        compiled_retry(Some(json!({"max_attempts": 7, "backoff_strategy": "linear"}))),
        (7, "linear".to_string(), initial, max)
    );
    assert_eq!(compiled_retry(Some(json!({"initial_delay_ms": 250}))), (3, backoff, 250, max));
}

/// The compiled schema always carries the whole policy: the runtime never reads a block
/// with a field missing, whatever the author left out.
#[test]
fn the_compiled_retry_carries_every_field() {
    let mut observers = observing_with(&json!({"type": "webhook", "url": "https://example.test"}));
    observers[0].as_object_mut().unwrap().remove("retry");
    let compiled = compile(schema_declaring(&observers)).unwrap();

    let document = serde_json::to_value(&compiled).unwrap();
    let observer = document["observers"]
        .as_array()
        .and_then(|observers| observers.iter().find(|o| o["name"] == "order_created"))
        .expect("the observer is in the compiled document");
    let mut keys: Vec<&str> =
        observer["retry"].as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "backoff_strategy",
            "initial_delay_ms",
            "max_attempts",
            "max_delay_ms"
        ]
    );
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

#[test]
fn a_header_value_taken_from_the_environment_compiles_as_the_variables_name() {
    let action = json!({"type": "webhook", "url": "https://example.test/hook",
                        "headers": {"X-Tenant-Id": "acme"},
                        "headers_env": {"Authorization": "ORDER_HOOK_TOKEN"}});

    let compiled = compile(schema_declaring(&observing_with(&action))).unwrap();

    let observer = compiled.find_observer("order_created").expect("the observer is compiled");
    assert_eq!(observer.actions, vec![action]);
}

/// A header whose name says it carries a credential is refused when its value is written
/// into the schema, whatever the case of its name, and the value is not repeated.
#[test]
fn a_credential_header_written_into_the_schema_is_refused_naming_headers_env() {
    for name in [
        "Authorization",
        "proxy-authorization",
        "Cookie",
        "X-API-Key",
        "X-CSRF-Token",
        "X-Webhook-Secret",
        "X-Password",
    ] {
        let refusal = refusal_for(&json!({"type": "webhook", "url": "https://example.test/hook",
                                           "headers": {name: "hunter2-value"}}));

        assert!(refusal.contains("`order_created`"), "{name}: names the observer: {refusal}");
        assert!(refusal.contains(name), "{name}: names the header: {refusal}");
        assert!(refusal.contains("headers_env"), "{name}: names the way out: {refusal}");
        assert!(!refusal.contains("hunter2-value"), "{name}: value not repeated: {refusal}");
    }
}

#[test]
fn a_header_named_in_both_headers_and_headers_env_is_refused_whatever_its_case() {
    let refusal = refusal_for(&json!({"type": "webhook", "url": "https://example.test/hook",
                                       "headers": {"x-trace": "a"},
                                       "headers_env": {"X-Trace": "TRACE_VALUE"}}));

    assert!(refusal.contains("`order_created`"), "names the observer: {refusal}");
    assert!(refusal.to_ascii_lowercase().contains("x-trace"), "names the header: {refusal}");
    assert!(refusal.contains("headers_env"), "names both keys: {refusal}");
}

#[test]
fn a_headers_env_entry_naming_no_variable_is_refused() {
    let refusal = refusal_for(&json!({"type": "webhook", "url": "https://example.test/hook",
                                       "headers_env": {"Authorization": ""}}));

    assert!(refusal.contains("Authorization"), "names the header: {refusal}");
    assert!(refusal.contains("headers_env"), "names the key: {refusal}");
}

#[test]
fn a_headers_env_entry_that_is_not_a_header_name_is_refused() {
    let refusal = refusal_for(&json!({"type": "webhook", "url": "https://example.test/hook",
                                       "headers_env": {"Bad Header:": "HOOK_TOKEN"}}));

    assert!(refusal.contains("Bad Header:"), "names the header: {refusal}");
    assert!(refusal.contains("headers_env"), "names the key: {refusal}");
}

/// What `fraiseql compile` printed for `schema` beside `toml`, and whether it compiled.
fn compile_output(schema: &Value, toml: &str) -> (bool, String) {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(dir.path().join("schema.json"), schema.to_string()).unwrap();
    std::fs::write(
        dir.path().join("fraiseql.toml"),
        format!("{toml}\n[project]\nname = \"app\"\n"),
    )
    .unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args(["compile", "schema.json", "--output", "schema.compiled.json"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let printed = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), printed)
}

/// `Order` captured from `tb_order`, observed by a webhook whose body reads the embedded
/// customer.
fn captured_order_reading_the_customer() -> Value {
    let action = json!({"type": "webhook", "url": "https://example.test/hook",
        "body_template": "{\"s\": {{ review_status }}, \"e\": {{ customer.email }}}"});
    let mut schema = schema_declaring(&observing_with(&action));
    schema["types"][1]["subscribable_tables"] = json!(["tb_order"]);
    schema
}

/// A table of plain columns is captured as its row, which holds no embedded field. The
/// template compiles, since a mutation's event fills it, and the compile says what a
/// captured event will not.
#[test]
fn a_template_a_captured_row_cannot_fill_compiles_with_a_warning() {
    let (compiled, printed) = compile_output(&captured_order_reading_the_customer(), "");

    assert!(compiled, "{printed}");
    for part in [
        "order_created",
        "{{ customer.email }}",
        "`Order`",
        "`tb_order`",
    ] {
        assert!(printed.contains(part), "the warning names {part}: {printed}");
    }
    assert!(!printed.contains("{{ review_status }}"), "a column carries a scalar: {printed}");
}

/// Declared to keep its document, the table is captured as a mutation stores it.
#[test]
fn a_table_declared_to_keep_its_document_compiles_without_the_warning() {
    let (compiled, printed) = compile_output(
        &captured_order_reading_the_customer(),
        "[subscriptions.capture.tb_order]\ndocument_column = \"data\"\n",
    );

    assert!(compiled, "{printed}");
    assert!(!printed.contains("{{ customer.email }}"), "{printed}");
}

/// A column the row holds and the document does not is still refused.
#[test]
fn a_column_only_the_captured_row_holds_is_still_refused_at_compile() {
    let mut schema = captured_order_reading_the_customer();
    schema["observers"][0]["actions"][0]["body_template"] = json!("{\"c\": {{ fk_customer }}}");

    let (compiled, printed) = compile_output(&schema, "");

    assert!(!compiled, "{printed}");
    assert!(printed.contains("{{ fk_customer }}"), "{printed}");
}

/// `Order` observed under `condition` by a webhook with no template; `row_captured` puts
/// `tb_order`, a table of plain columns, among its captured tables.
fn order_observed_when(condition: &str, row_captured: bool) -> Value {
    let action = json!({"type": "webhook", "url": "https://example.test/hook"});
    let mut schema = schema_declaring(&observing_with(&action));
    schema["observers"][0]["condition"] = json!(condition);
    if row_captured {
        schema["types"][1]["subscribable_tables"] = json!(["tb_order"]);
    }
    schema
}

/// Every condition message starts the same way, so a test can say none was printed.
const CONDITION_SAYS: &str = "`condition` reads `";

/// Only the document reaches an observer of a type with no table captured as its row, so
/// a key no field holds fails every event.
#[test]
fn a_condition_naming_no_field_of_a_document_only_type_is_refused() {
    let (compiled, printed) = compile_output(&order_observed_when("totl > 100", false), "");

    assert!(!compiled, "{printed}");
    for part in ["order_created", "`totl`", "`Order`"] {
        assert!(printed.contains(part), "the refusal names {part}: {printed}");
    }
}

#[test]
fn a_condition_naming_a_field_by_its_published_name_is_refused_with_the_stored_key() {
    let (compiled, printed) =
        compile_output(&order_observed_when("reviewStatus == 'approved'", false), "");

    assert!(!compiled, "{printed}");
    assert!(printed.contains("`review_status`"), "{printed}");
}

/// A condition reads one top-level key of the event, and none has a dot in it, so a call
/// naming a path never matches: refused whatever the type's tables.
#[test]
fn a_dotted_key_in_a_condition_call_is_refused() {
    for row_captured in [false, true] {
        let (compiled, printed) =
            compile_output(&order_observed_when("has_field('customer.email')", row_captured), "");

        assert!(!compiled, "row captured: {row_captured}: {printed}");
        assert!(printed.contains("`customer.email`"), "{printed}");
    }
}

/// The schema holds no table's columns: on a type whose table is captured as its row, a
/// key no field holds may be a column, which only that row supplies. Refused where the
/// document is the only image, warned of where the row is the other.
#[test]
fn the_same_unknown_key_is_refused_on_a_document_only_type_and_warned_of_on_a_row_captured_one() {
    let (compiled, printed) = compile_output(&order_observed_when("fk_customer == 3", false), "");
    assert!(!compiled, "document only: {printed}");
    assert!(printed.contains("`fk_customer`"), "{printed}");

    let (compiled, printed) = compile_output(&order_observed_when("fk_customer == 3", true), "");
    assert!(compiled, "row captured: {printed}");
    for part in ["order_created", "`fk_customer`", "`Order`", "`tb_order`"] {
        assert!(printed.contains(part), "the warning names {part}: {printed}");
    }
}

/// An embedded field is in the document and not in a captured row: the condition
/// compiles, and the compile says what a captured event will not hold. Declared to keep
/// its document, the table is captured as a mutation stores it, and nothing is said.
#[test]
fn a_condition_on_an_embedded_field_of_a_row_captured_type_compiles_with_a_warning() {
    let schema = order_observed_when("has_field('customer')", true);

    let (compiled, printed) = compile_output(&schema, "");
    assert!(compiled, "{printed}");
    for part in ["order_created", "`customer`", "`Order`", "`tb_order`"] {
        assert!(printed.contains(part), "the warning names {part}: {printed}");
    }

    let (compiled, printed) =
        compile_output(&schema, "[subscriptions.capture.tb_order]\ndocument_column = \"data\"\n");
    assert!(compiled, "{printed}");
    assert!(!printed.contains(CONDITION_SAYS), "{printed}");
}

/// Stored keys of scalar fields, in every form a condition reads, compile in silence.
#[test]
fn a_condition_on_stored_scalar_keys_compiles_without_a_word() {
    let condition = "total > 100 && !has_field('review_status') \
                     || field_changed_to('review_status', 'approved')";
    for row_captured in [false, true] {
        let (compiled, printed) = compile_output(&order_observed_when(condition, row_captured), "");

        assert!(compiled, "row captured: {row_captured}: {printed}");
        assert!(!printed.contains(CONDITION_SAYS), "{printed}");
    }
}

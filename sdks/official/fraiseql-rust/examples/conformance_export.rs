//! Authors the cross-SDK conformance fixture with the Rust SDK's public API.
//!
//! Driven by `sdks/official/conformance/run.py`; see
//! `sdks/official/conformance/README.md`.
//!
//! The one rule for every SDK's copy of this file: author through the SDK, never
//! hand-assemble the JSON.
//!
//! This SDK is field-level-RBAC focused: it registers types and their fields and has no
//! query, mutation, enum or input-object builders. Those gaps are declared in
//! `conformance/manifest.json` with their reasons, and the harness holds it to exactly
//! what it claims to support.

use std::{env, fs};

use fraiseql_rust::{
    BackoffStrategy, EmailAction, Field, Observer, ObserverEvent, ObserverRetry, SchemaRegistry,
    SlackAction, SlackDestination, VectorConfig, VectorIndex, VectorMetric, WebhookAction,
    WebhookDestination,
};

fn minimal() -> SchemaRegistry {
    let mut registry = SchemaRegistry::new();
    registry.register_type_with_source(
        "User",
        vec![
            Field::new("id", "ID").with_nullable(false),
            Field::new("email", "String").with_nullable(false),
        ],
        "v_user",
    );
    registry
}

fn full() -> SchemaRegistry {
    let mut registry = SchemaRegistry::new();
    registry.register_type_with_source(
        "User",
        vec![
            Field::new("id", "ID").with_nullable(false),
            Field::new("email", "String").with_nullable(false),
            // The quote characters are deliberate: this description is what used to make
            // the export unparseable JSON (#855).
            Field::new("name", "String")
                .with_nullable(true)
                .with_description(Some("The user's \"display\" name".to_string()))
                .with_deprecated(Some("use displayName".to_string()))
                .with_localized(),
            Field::new("salary", "Float")
                .with_nullable(true)
                .with_requires_scope(Some("read:User.salary".to_string())),
            // Two words and a digit segment (#1249). The Rust SDK's author writes the wire
            // name, so these are already camelCase; the translation is exercised by the
            // SDKs whose identifiers are idiomatic instead (Python, Ruby, Elixir, C#, F#).
            Field::new("lastLoginAt", "String").with_nullable(true),
            Field::new("phone1", "String").with_nullable(true),
        ],
        "v_user",
    );
    registry.register_type_with_source(
        "Order",
        vec![
            Field::new("id", "ID").with_nullable(false),
            Field::new("total", "Float").with_nullable(false),
            Field::new("status", "String").with_nullable(false),
            // The column `User.orders` joins on in the other SDKs' fixtures (#1266).
            // This SDK declares no relationship — see the `type_relationships` gap in
            // `conformance/manifest.json` — but the field is part of the `types`
            // construct and is asserted for every SDK.
            Field::new("fkUser", "ID").with_nullable(false),
        ],
        "v_order",
    );
    registry.register_type_with_source(
        "UserNotFound",
        vec![
            Field::new("message", "String").with_nullable(false),
            Field::new("code", "String").with_nullable(false),
        ],
        "v_user_not_found",
    );
    registry.register_type_with_source(
        "Document",
        vec![
            Field::new("id", "ID").with_nullable(false),
            Field::new("embedding", "Vector").with_nullable(false).with_vector_config(Some(
                VectorConfig::new(1536)
                    .with_index(VectorIndex::IvfFlat)
                    .with_metric(VectorMetric::L2),
            )),
            Field::new("fingerprint", "BitVector")
                .with_nullable(false)
                .with_vector_config(Some(
                    VectorConfig::new(768).with_metric(VectorMetric::Hamming),
                )),
            Field::new("compact", "HalfVector").with_nullable(true).with_vector_config(Some(
                VectorConfig::new(1536).with_metric(VectorMetric::InnerProduct),
            )),
            Field::new("terms", "SparseVector")
                .with_nullable(true)
                .with_vector_config(Some(VectorConfig::new(30000).with_index(VectorIndex::None))),
            Field::new("similarity", "Float")
                .with_nullable(false)
                .with_vector_distance(Some("embedding".to_string())),
        ],
        "v_document",
    );
    // Lists (#1586, #1608): the GraphQL type string, every level as written.
    registry.register_type_with_source(
        "ReadingSeries",
        vec![
            Field::new("id", "ID").with_nullable(false),
            Field::new("labels", "[String!]").with_nullable(false),
            Field::new("readings", "[Float]").with_nullable(true),
            Field::new("grid", "[[Float]!]").with_nullable(false),
        ],
        "v_reading_series",
    );
    // A 64-bit integer is the type string `BigInt`: the SDK's types are strings.
    registry.register_type_with_source(
        "LedgerEntry",
        vec![
            Field::new("id", "ID").with_nullable(false),
            Field::new("amount", "BigInt").with_nullable(false),
            Field::new("balance", "BigInt").with_nullable(true),
            Field::new("transferIds", "[BigInt!]").with_nullable(false),
        ],
        "v_ledger_entry",
    );
    // A date and time with no offset is the type string `LocalDateTime`.
    registry.register_type_with_source(
        "ShiftSlot",
        vec![
            Field::new("id", "ID").with_nullable(false),
            Field::new("startsAt", "LocalDateTime").with_nullable(false),
            Field::new("endsAt", "LocalDateTime").with_nullable(true),
            Field::new("breaksAt", "[LocalDateTime!]").with_nullable(false),
        ],
        "v_shift_slot",
    );
    // The exact scalars are the type strings `Decimal` and `UUID`.
    registry.register_type_with_source(
        "PriceQuote",
        vec![
            Field::new("id", "ID").with_nullable(false),
            Field::new("amount", "Decimal").with_nullable(false),
            Field::new("fxRate", "Decimal").with_nullable(true),
            Field::new("deltas", "[Decimal!]").with_nullable(false),
            Field::new("requestId", "UUID").with_nullable(false),
            Field::new("issuerId", "UUID").with_nullable(true),
        ],
        "v_price_quote",
    );
    // One observer per action kind. Every observer but the last sets its retry; the last
    // runs on the server's own policy, so it writes no retry block.
    for observer in [
        Observer::new("orderPlacedWebhook", "Order", ObserverEvent::Insert)
            .with_action(
                WebhookAction::new(WebhookDestination::UrlEnv("CONFORMANCE_HOOK_URL".to_string()))
                    .with_method("PUT")
                    .with_body_template(r#"{"total": {{ total }}}"#)
                    .with_signing_secret_env("CONFORMANCE_HOOK_KEY")
                    .with_header("X-Source", "conformance")
                    .with_header_env("Authorization", "CONFORMANCE_HOOK_TOKEN"),
            )
            .with_retry(ObserverRetry::new(5, BackoffStrategy::Linear, 200, 5000)),
        Observer::new("orderShippedSlack", "Order", ObserverEvent::Update)
            .with_condition("status == 'shipped'")
            .with_action(SlackAction::new("#orders", "Order {{ id }} shipped").with_destination(
                SlackDestination::WebhookUrlEnv("CONFORMANCE_SLACK_URL".to_string()),
            ))
            .with_retry(ObserverRetry::new(4, BackoffStrategy::Exponential, 250, 8000)),
        Observer::new("orderDeletedEmail", "Order", ObserverEvent::Delete)
            .with_action(EmailAction::new(
                "ops@example.com",
                "Order deleted",
                "Order {{ id }} was deleted",
            ))
            .with_retry(ObserverRetry::new(2, BackoffStrategy::Fixed, 500, 500)),
        Observer::new("orderCreatedDefaultRetry", "Order", ObserverEvent::Insert).with_action(
            EmailAction::new("ops@example.com", "Order created", "Order {{ id }} was created"),
        ),
        // One destination per action, written as a URL: no default environment variable beside it (#2279).
        Observer::new("orderRefundedDirect", "Order", ObserverEvent::Update)
            .with_condition("status == 'refunded'")
            .with_action(WebhookAction::new(WebhookDestination::Url(
                "https://hooks.example.com/conformance/refunds".to_string(),
            )))
            .with_action(SlackAction::new("#refunds", "Order {{ id }} refunded").with_destination(
                SlackDestination::WebhookUrl(
                    "https://hooks.slack.example.com/services/conformance".to_string(),
                ),
            )),
    ] {
        registry.register_observer(observer).expect("observer names are distinct");
    }
    registry
}

fn main() {
    let fixture =
        env::var("FRAISEQL_CONFORMANCE_FIXTURE").expect("FRAISEQL_CONFORMANCE_FIXTURE must be set");
    let out = env::var("FRAISEQL_CONFORMANCE_OUT").expect("FRAISEQL_CONFORMANCE_OUT must be set");

    let registry = match fixture.as_str() {
        "minimal" => minimal(),
        "full" => full(),
        other => panic!("unknown fixture {other}"),
    };

    fs::write(out, registry.export_to_json()).expect("failed to write schema.json");
}

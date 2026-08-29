//! End-to-end for the `[subscription_kafka]` mount, against a real broker (#1102).
//!
//! The whole of #1102 is that `KafkaAdapter` was surface no configuration could reach:
//! no config section, no route, no mount — and therefore no test, which is why it sat
//! four releases setting no `security.protocol` at all. A unit test of the adapter would
//! not have caught that, and does not close it now. What closes it is this: a section an
//! operator can write, mounted the way the server mounts it, producing a message a real
//! consumer reads back.
//!
//! Requires `KAFKA_BOOTSTRAP` (`host:port`, no scheme — the sink's own `kafka://` prefix
//! is added here). Skips loudly when unset rather than passing quietly: a test that says
//! nothing when its rig is missing is the shape this repository keeps deleting.

#![cfg(feature = "subscription-kafka")]
#![allow(clippy::unwrap_used, clippy::expect_used)]
// Reason: a silent skip, and a silently-swallowed transient consume error, are
// exactly the failure modes this suite exists to prevent — both must be visible.
#![allow(clippy::print_stderr)]

use std::{sync::Arc, time::Duration};

use fraiseql_core::{
    runtime::{
        SubscriptionManager,
        subscription::{SubscriptionEvent, SubscriptionOperation},
    },
    schema::{CompiledSchema, SubscriptionDefinition},
};
use fraiseql_kafka::rdkafka::{
    ClientConfig, Message,
    admin::{AdminClient, AdminOptions, NewTopic, TopicReplication},
    client::DefaultClientContext,
    consumer::{Consumer, StreamConsumer},
};
use fraiseql_server::{server_config::SubscriptionKafkaConfig, subscription_kafka};
use serde_json::Value;
use uuid::Uuid;

fn bootstrap() -> Option<String> {
    match std::env::var("KAFKA_BOOTSTRAP") {
        Ok(v) if !v.trim().is_empty() => Some(v),
        _ => {
            eprintln!(
                "SKIP: KAFKA_BOOTSTRAP is unset. This suite needs a broker; it runs on the \
                 Dagger leg that binds one."
            );
            None
        },
    }
}

fn consumer(bootstrap: &str, group: &str) -> StreamConsumer {
    ClientConfig::new()
        .set("bootstrap.servers", bootstrap)
        .set("group.id", group)
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()
        .expect("consumer")
}

/// Create the topic explicitly rather than leaning on broker auto-creation.
///
/// Auto-creation happens on first *produce*, so a consumer that subscribes before it
/// reports a transport failure against a topic that does not exist yet — which reads as
/// "the mirror published nothing" when the mirror is fine.
async fn create_topic(bootstrap: &str, topic: &str) {
    let admin: AdminClient<DefaultClientContext> = ClientConfig::new()
        .set("bootstrap.servers", bootstrap)
        .create()
        .expect("admin client");
    admin
        .create_topics(&[NewTopic::new(topic, 1, TopicReplication::Fixed(1))], &AdminOptions::new())
        .await
        .expect("create topic");
}

/// The mount, end to end: a `[subscription_kafka]` section builds a mirror, the mirror
/// runs on a task set, and a payload the subscription manager broadcasts arrives on the
/// topic — with the subscription's name on it, because a Kafka message here corresponds
/// to a delivery some subscriber received.
#[tokio::test]
async fn a_configured_mirror_publishes_what_subscribers_receive() {
    let Some(bootstrap_servers) = bootstrap() else {
        return;
    };
    let topic = format!("fraiseql-subs-{}", Uuid::new_v4());

    let section = SubscriptionKafkaConfig {
        endpoint: format!("kafka://{bootstrap_servers}"),
        default_topic: topic.clone(),
        ..SubscriptionKafkaConfig::default()
    };

    // Development plaintext, opted in: the guard refuses `kafka://` otherwise, which is
    // the point of the guard and is what the sibling test asserts. This is the escape
    // hatch it leaves for a dev broker, exercised the way an operator would.
    //
    // Scoped to the `build_mirror` call rather than set for the process: that call is
    // synchronous and is the only thing that reads the environment, so nothing here
    // races a sibling test's view of it.
    let mirror = temp_env::with_vars(
        [
            ("FRAISEQL_KAFKA_ALLOW_PLAINTEXT", Some("true")),
            ("FRAISEQL_ENV", Some("development")),
            ("FRAISEQL_PROFILE", None),
            ("KUBERNETES_SERVICE_HOST", None),
        ],
        || subscription_kafka::build_mirror(Some(&section)),
    )
    .expect("a guarded dev endpoint must build")
    .expect("a present section must produce a mirror");

    let mut schema = CompiledSchema::new();
    schema.subscriptions.push(SubscriptionDefinition::new("orderCreated", "Order"));
    let manager = Arc::new(SubscriptionManager::new(Arc::new(schema)));

    // A live subscription, because the manager broadcasts one payload per *matching*
    // one. Without this the mirror has nothing to mirror — which is itself the
    // difference between this transport and `[cdc_outbound]`.
    manager
        .subscribe("orderCreated", Value::Null, Value::Null, "conn-1")
        .expect("subscribe");

    let mut tasks = tokio::task::JoinSet::new();
    subscription_kafka::spawn(mirror, &manager, &mut tasks);

    create_topic(&bootstrap_servers, &topic).await;
    let consumer = consumer(&bootstrap_servers, &format!("g-{}", Uuid::new_v4()));
    consumer.subscribe(&[topic.as_str()]).expect("subscribe to topic");

    let matched = manager.publish_event(SubscriptionEvent::new(
        "Order",
        "ord_e2e_1",
        SubscriptionOperation::Create,
        serde_json::json!({"id": "ord_e2e_1", "total": 42}),
    ));
    assert_eq!(matched, 1, "the event must match the live subscription");

    // Poll rather than take a single `recv`: a consumer that joined before the group
    // stabilised reports a transient error once, and a single-shot read would score that
    // as "the mirror published nothing".
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let message = loop {
        assert!(tokio::time::Instant::now() < deadline, "no mirrored message arrived within 30s");
        match tokio::time::timeout(Duration::from_secs(5), consumer.recv()).await {
            Ok(Ok(message)) => break message,
            Ok(Err(error)) => eprintln!("transient consume error, retrying: {error}"),
            Err(_) => eprintln!("no message yet, retrying"),
        }
    };

    let payload: Value = serde_json::from_slice(message.payload().expect("payload")).unwrap();
    assert_eq!(payload["entity_id"], "ord_e2e_1");
    assert_eq!(payload["entity_type"], "Order");
    assert_eq!(
        payload["subscription_name"], "orderCreated",
        "a mirrored message names the subscription whose delivery it mirrors"
    );
    assert_eq!(payload["data"]["total"], 42);
    assert_eq!(
        message.key().map(|k| String::from_utf8_lossy(k).into_owned()),
        Some("ord_e2e_1".to_owned()),
        "the entity id is the partition key, which is what makes per-entity order hold"
    );

    tasks.abort_all();
}

/// The refusal, against the same real broker: without the development opt-in a
/// `kafka://` endpoint does not produce a client at all. Both directions in one rig, so
/// a guard that has quietly stopped refusing cannot hide behind a passing happy path.
#[tokio::test]
async fn the_same_endpoint_is_refused_without_the_development_opt_in() {
    let Some(bootstrap_servers) = bootstrap() else {
        return;
    };

    let section = SubscriptionKafkaConfig {
        endpoint: format!("kafka://{bootstrap_servers}"),
        default_topic: "fraiseql-subs-refused".to_owned(),
        ..SubscriptionKafkaConfig::default()
    };

    temp_env::with_vars(
        [
            ("FRAISEQL_KAFKA_ALLOW_PLAINTEXT", None::<&str>),
            ("FRAISEQL_ENV", Some("production")),
        ],
        || {
            let err = subscription_kafka::build_mirror(Some(&section))
                .expect_err("plaintext in production must refuse the boot");
            assert!(
                err.contains("FRAISEQL_KAFKA_ALLOW_PLAINTEXT"),
                "the refusal must name the opt-in it is refusing: {err}"
            );
        },
    );
}

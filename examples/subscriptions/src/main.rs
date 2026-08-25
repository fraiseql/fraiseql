//! Subscriptions, from the engine's side.
//!
//! A GraphQL subscription in FraiseQL is not a long-running query. The
//! [`SubscriptionManager`] holds the open subscriptions, a change source publishes
//! [`SubscriptionEvent`]s into it, and each event is matched against every open
//! subscription and projected to that subscriber's selection. Whatever moves the
//! bytes to the client — a WebSocket, an SSE stream, a webhook — sits on top of the
//! broadcast channel this hands out.
//!
//! This example plays the whole cycle in one process against the `examples/streaming`
//! schema: subscribe two clients, publish three events, show which client got what,
//! then unsubscribe. It needs no database and no server.
//!
//! Run it:
//!
//! ```text
//! ./run.sh
//! ```

use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use fraiseql_core::{
    runtime::{SubscriptionEvent, SubscriptionManager, SubscriptionOperation},
    schema::CompiledSchema,
};
use serde_json::json;

/// The streaming schema from `examples/streaming`, compiled.
const SCHEMA: &str = "../streaming/schema.compiled.json";

#[tokio::main]
async fn main() -> Result<()> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(SCHEMA);
    let json_text = std::fs::read_to_string(&path).with_context(|| missing_schema(&path))?;
    let schema = CompiledSchema::from_json(&json_text, false)
        .with_context(|| format!("{} is not a compiled schema", path.display()))?;
    println!("Loaded {}\n", path.display());

    let manager = SubscriptionManager::new(Arc::new(schema));

    // Take the receiver BEFORE subscribing. It is a broadcast channel: an event
    // published while nobody holds a receiver is dropped, not queued.
    let mut events = manager.receiver();

    // Two clients, two different subscriptions. `onMessage` and `onEvent` are
    // declared in examples/streaming/schema.py; `user_context` and `variables` are
    // what the transport learned about this client.
    let chat = manager
        .subscribe("onMessage", json!({"sub": "u-1"}), json!({}), "conn-chat")
        .context("subscribing to onMessage")?;
    let audit = manager
        .subscribe("onEvent", json!({"sub": "u-2"}), json!({}), "conn-audit")
        .context("subscribing to onEvent")?;
    println!("chat  subscription {chat} on connection conn-chat");
    println!("audit subscription {audit} on connection conn-audit");
    println!(
        "{} open subscriptions across {} connections\n",
        manager.subscription_count(),
        manager.connection_count()
    );

    // Publish. `entity_type` is what routes an event: it must equal the subscription's
    // GraphQL return type — "Message" for onMessage, "Event" for onEvent. An event for
    // an entity nobody is watching matches zero subscriptions and costs nothing.
    for event in [
        SubscriptionEvent::new(
            "Message",
            "m-1",
            SubscriptionOperation::Create,
            json!({"id": "m-1", "username": "ada", "content": "the engine is up"}),
        ),
        SubscriptionEvent::new(
            "Event",
            "e-1",
            SubscriptionOperation::Create,
            json!({"id": "e-1", "type": "deploy", "data": {"version": "2.15.0"}}),
        ),
        SubscriptionEvent::new(
            "LiveMetrics",
            "s-1",
            SubscriptionOperation::Create,
            json!({"id": "s-1", "metric": "rps", "value": 1420}),
        ),
    ] {
        let entity = event.entity_type.clone();
        let matched = manager.publish_event(event);
        println!("published an event for {entity} → matched {matched} subscription(s)");
    }

    // Drain what the transport would have written to each client.
    println!();
    while let Ok(payload) = events.try_recv() {
        println!(
            "  → {} (subscription {}): {}",
            payload.subscription_name,
            payload.subscription_id,
            serde_json::to_string(&payload.data)?
        );
    }

    // Closing a connection unsubscribes everything on it — the transport calls this
    // when the socket goes away, so a dropped client cannot leak subscriptions.
    manager.unsubscribe_connection("conn-chat");
    println!("\nconn-chat closed; {} subscription(s) remain", manager.subscription_count());

    manager.unsubscribe(audit).context("unsubscribing audit")?;
    println!("audit unsubscribed; {} subscription(s) remain", manager.subscription_count());

    Ok(())
}

fn missing_schema(path: &std::path::Path) -> String {
    format!(
        "cannot read {}.\n\nThe compiled schema is a build artifact (it is gitignored). Make it:\n\
         \n    cargo run -p fraiseql-cli -- compile examples/streaming/schema.json \\\n\
         \x20        -o examples/streaming/schema.compiled.json\n\n\
         or run ./run.sh from this directory, which does that first.",
        path.display()
    )
}

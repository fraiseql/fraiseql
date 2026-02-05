//! Elasticsearch sink example demonstrating event indexing
//!
//! This example shows how to:
//! 1. Create test events
//! 2. Configure the Elasticsearch sink
//! 3. Index events to Elasticsearch
//! 4. Verify data with sample queries
//!
//! # Prerequisites
//!
//! - Elasticsearch running on localhost:9200
//! - Index template applied (see migrations/elasticsearch/README.md)
//!
//! # Running
//!
//! ```bash
//! docker-compose -f docker-compose.elasticsearch.yml up -d
//! cargo run --example elasticsearch_sink
//! ```

use fraiseql_observers::{ElasticsearchSink, ElasticsearchSinkConfig, EntityEvent, EventKind};
use serde_json::json;
use tokio::sync::mpsc;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    println!("🚀 Starting Elasticsearch Sink Example");
    println!("====================================\n");

    // Step 1: Create configuration
    println!("Step 1: Creating Elasticsearch sink configuration...");
    let config = ElasticsearchSinkConfig::default().with_env_overrides();

    println!("  URL:           {}", config.url);
    println!("  Index Prefix:  {}", config.index_prefix);
    println!("  Bulk Size:     {}", config.bulk_size);
    println!("  Timeout:       {}s\n", config.flush_interval_secs);

    // Validate configuration
    config.validate()?;
    println!("✅ Configuration validated\n");

    // Step 2: Create sink
    println!("Step 2: Creating Elasticsearch sink...");
    let sink = ElasticsearchSink::new(config.clone())?;
    println!("✅ Sink created\n");

    // Step 3: Health check
    println!("Step 3: Checking Elasticsearch health...");
    match sink.health_check().await {
        Ok(()) => println!("✅ Elasticsearch is healthy\n"),
        Err(e) => {
            eprintln!("⚠️  Elasticsearch health check failed: {}", e);
            eprintln!("Make sure Elasticsearch is running:");
            eprintln!("  docker-compose -f docker-compose.elasticsearch.yml up -d\n");
            return Err(e.into());
        },
    }

    // Step 4: Create channel
    println!("Step 4: Setting up event channel...");
    let (tx, rx) = mpsc::channel(100);
    println!("✅ Channel created (capacity: 100)\n");

    // Step 5: Spawn sink task in background
    println!("Step 5: Spawning sink background task...");
    let sink_handle = tokio::spawn(async move {
        match sink.run(rx).await {
            Ok(()) => println!("✅ Sink completed successfully"),
            Err(e) => eprintln!("❌ Sink error: {}", e),
        }
    });

    // Step 6: Generate and send test events
    println!("Step 6: Generating and indexing test events...");
    let num_events = 50;

    for i in 0..num_events {
        let entity_types = ["User", "Order", "Product", "Invoice"];
        let event_types = [EventKind::Created, EventKind::Updated, EventKind::Deleted];

        #[allow(clippy::cast_precision_loss)] // i is bounded by num_events (50)
        let amount = 100.0 + i as f64;
        let event = EntityEvent::new(
            event_types[i % event_types.len()],
            entity_types[i % entity_types.len()].to_string(),
            Uuid::new_v4(),
            json!({
                "test_index": i,
                "name": format!("{}-{}", entity_types[i % entity_types.len()], i),
                "status": if i % 3 == 0 { "active" } else { "inactive" },
                "created_at": chrono::Utc::now().to_rfc3339(),
                "data": {
                    "amount": amount,
                    "currency": "USD"
                }
            }),
        )
        .with_user_id(format!("user-{}", i % 10));

        if let Err(e) = tx.send(event).await {
            eprintln!("⚠️  Failed to send event {}: {}", i, e);
        }

        if (i + 1) % 10 == 0 {
            println!("  Sent {} events...", i + 1);
        }
    }

    println!("✅ Sent {} test events\n", num_events);

    // Step 7: Close channel and wait for sink
    println!("Step 7: Closing channel and waiting for sink to finish...");
    drop(tx);

    // Give sink time to process remaining events
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Wait for sink task to complete
    match sink_handle.await {
        Ok(()) => println!("✅ Sink task completed\n"),
        Err(e) => eprintln!("❌ Sink task error: {}\n", e),
    }

    // Step 8: Provide verification instructions
    println!("Step 8: Verifying data in Elasticsearch...");
    println!("\nTo verify the data was indexed, run these commands:\n");

    println!("# Check total event count:");
    println!("  curl 'localhost:9200/fraiseql-events-*/_count?pretty'");
    println!();

    println!("# View sample events:");
    println!("  curl 'localhost:9200/fraiseql-events-*/_search?pretty&size=5'");
    println!();

    println!("# Count by event type:");
    println!("  curl -X POST 'localhost:9200/fraiseql-events-*/_search?pretty' \\");
    println!("    -H 'Content-Type: application/json' -d '{{");
    println!("      \"size\": 0,");
    println!("      \"aggs\": {{");
    println!("        \"by_type\": {{ \"terms\": {{ \"field\": \"event_type\" }} }}");
    println!("      }}");
    println!("    }}'");
    println!();

    println!("# Count by entity type:");
    println!("  curl -X POST 'localhost:9200/fraiseql-events-*/_search?pretty' \\");
    println!("    -H 'Content-Type: application/json' -d '{{");
    println!("      \"size\": 0,");
    println!("      \"aggs\": {{");
    println!("        \"by_entity\": {{ \"terms\": {{ \"field\": \"entity_type\" }} }}");
    println!("      }}");
    println!("    }}'");
    println!();

    println!("# Open Kibana for visualization:");
    println!("  open http://localhost:5601");
    println!();

    println!("See docs/elasticsearch_queries.md for more query examples.");
    println!();

    println!("✅ Example completed successfully!");
    println!("====================================");

    Ok(())
}

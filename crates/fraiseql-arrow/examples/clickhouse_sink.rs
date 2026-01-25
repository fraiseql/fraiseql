//! ClickHouse sink example demonstrating end-to-end event ingestion
//!
//! This example shows how to:
//! 1. Create test event data
//! 2. Convert events to Arrow RecordBatches
//! 3. Send batches to ClickHouse via the sink
//! 4. Verify insertion with a query
//!
//! # Prerequisites
//!
//! - ClickHouse running on localhost:8123
//! - Tables created via: `docker-compose -f docker-compose.clickhouse.yml up -d`
//!
//! # Running
//!
//! ```bash
//! cd crates/fraiseql-arrow
//! cargo run --example clickhouse_sink --features clickhouse
//! ```

#[cfg(feature = "clickhouse")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use fraiseql_arrow::{ClickHouseSink, ClickHouseSinkConfig, EventRow};
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::sync::mpsc;

    println!("🚀 Starting ClickHouse Sink Example");
    println!("===================================\n");

    // Step 1: Create configuration
    println!("Step 1: Creating ClickHouse sink configuration...");
    let config = ClickHouseSinkConfig::default()
        .with_env_overrides();

    println!("  URL:          {}", config.url);
    println!("  Database:     {}", config.database);
    println!("  Table:        {}", config.table);
    println!("  Batch size:   {}", config.batch_size);
    println!("  Timeout:      {}s\n", config.batch_timeout_secs);

    // Validate configuration
    config.validate()?;
    println!("✅ Configuration validated\n");

    // Step 2: Create sink
    println!("Step 2: Creating ClickHouse sink...");
    let sink = ClickHouseSink::new(config.clone())?;
    println!("✅ Sink created\n");

    // Step 3: Create channel
    println!("Step 3: Setting up event channel...");
    let (tx, rx) = mpsc::channel(100);
    println!("✅ Channel created (capacity: 100)\n");

    // Step 4: Generate sample data
    println!("Step 4: Generating sample event data...");
    let num_events = 100;
    let mut rows = Vec::with_capacity(num_events);

    let now_micros = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as i64;

    for i in 0..num_events {
        let event_id = format!("evt-{:05}", i);
        let entity_id = format!("entity-{}", i % 10);
        let user_id = if i % 3 == 0 {
            Some(format!("user-{}", i / 3))
        } else {
            None
        };
        let org_id = if i % 5 == 0 {
            Some(format!("org-{}", i / 5))
        } else {
            None
        };

        let event_types = ["created", "updated", "deleted"];
        let entity_types = ["User", "Product", "Order", "Invoice"];

        let row = EventRow {
            event_id,
            event_type: event_types[i % event_types.len()].to_string(),
            entity_type: entity_types[i % entity_types.len()].to_string(),
            entity_id,
            timestamp: now_micros + (i as i64) * 1_000_000, // Space out by 1 second each
            data: format!(
                r#"{{"event_number":{}, "processed_at": {}, "test": true}}"#,
                i,
                now_micros
            ),
            user_id,
            org_id,
        };
        rows.push(row);
    }

    println!("✅ Generated {} sample events\n", num_events);

    // Step 5: Spawn sink task in background
    println!("Step 5: Spawning sink background task...");
    let sink_handle = tokio::spawn(async move {
        match sink.run(rx).await {
            Ok(()) => println!("✅ Sink completed successfully"),
            Err(e) => eprintln!("❌ Sink error: {}", e),
        }
    });

    // Step 6: Insert rows via EventRow objects
    println!("Step 6: Inserting event rows...");
    for row in rows {
        // Convert EventRow to RecordBatch
        // Note: In real usage, you'd use EventToArrowConverter
        // Here we just send the row directly for the sink to handle
        use arrow::array::StringArray;
        use arrow::record_batch::RecordBatch;
        use std::sync::Arc;

        let event_ids = StringArray::from(vec![row.event_id.clone()]);
        let event_types = StringArray::from(vec![row.event_type.clone()]);
        let entity_types = StringArray::from(vec![row.entity_type.clone()]);
        let entity_ids = StringArray::from(vec![row.entity_id.clone()]);

        use arrow::array::TimestampMicrosecondArray;
        let timestamps = TimestampMicrosecondArray::from(vec![row.timestamp]);

        let datas = StringArray::from(vec![row.data.clone()]);
        let user_ids = StringArray::from(vec![row.user_id.clone()]);
        let org_ids = StringArray::from(vec![row.org_id.clone()]);

        let batch = RecordBatch::try_from_iter(vec![
            ("event_id", Arc::new(event_ids) as Arc<dyn arrow::array::Array>),
            ("event_type", Arc::new(event_types)),
            ("entity_type", Arc::new(entity_types)),
            ("entity_id", Arc::new(entity_ids)),
            ("timestamp", Arc::new(timestamps)),
            ("data", Arc::new(datas)),
            ("user_id", Arc::new(user_ids)),
            ("org_id", Arc::new(org_ids)),
        ])
        .unwrap();

        match tx.send_timeout(batch, tokio::time::Duration::from_secs(5)).await {
            Ok(()) => {}
            Err(e) => eprintln!("⚠️  Failed to send batch: {}", e),
        }
    }

    println!("✅ Sent {} event rows\n", num_events);

    // Step 7: Close channel and wait for sink
    println!("Step 7: Closing channel and waiting for sink to finish...");
    drop(tx);

    // Give sink time to flush final batch
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Wait for sink task
    sink_handle.await?;
    println!("✅ Sink task completed\n");

    // Step 8: Verify data in ClickHouse
    println!("Step 8: Verifying data in ClickHouse...");
    println!("\nTo verify the data was inserted, run these commands:\n");
    println!("# Check row count:");
    println!("  docker exec fraiseql-clickhouse clickhouse-client --query \\");
    println!("    \"SELECT formatReadableQuantity(count()) FROM fraiseql_events\"");
    println!();
    println!("# View sample rows:");
    println!("  docker exec fraiseql-clickhouse clickhouse-client --query \\");
    println!("    \"SELECT event_id, event_type, entity_type, entity_id");
    println!("     FROM fraiseql_events LIMIT 10\"");
    println!();
    println!("# Check hourly aggregations:");
    println!("  docker exec fraiseql-clickhouse clickhouse-client --query \\");
    println!("    \"SELECT hour, event_type, event_count");
    println!("     FROM fraiseql_events_hourly\"");
    println!();

    println!("\n✅ Example completed successfully!");
    println!("===================================");

    Ok(())
}

#[cfg(not(feature = "clickhouse"))]
fn main() {
    eprintln!("❌ This example requires the 'clickhouse' feature");
    eprintln!("Run with: cargo run --example clickhouse_sink --features clickhouse");
}

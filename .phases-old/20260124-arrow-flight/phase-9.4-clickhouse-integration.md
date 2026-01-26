# Phase 9.4: ClickHouse Integration (Analytics Dataplane)

**Duration**: 3-4 days
**Priority**: ⭐⭐⭐⭐⭐
**Dependencies**: Phases 9.1, 9.3 complete
**Status**: Ready to implement (after 9.3)

---

## Objective

Integrate ClickHouse as the analytics database for FraiseQL, consuming Arrow Flight streams for:
- High-performance event analytics (1M+ events/sec ingestion)
- Time-series aggregations (ORDER BY timestamp, GROUP BY time windows)
- Retention policies (automatic old data cleanup)
- Materialized views for pre-computed metrics
- Columnar storage optimized for analytical queries

**Use Case**: Real-time business intelligence dashboards powered by FraiseQL observer events.

---

## Architecture

```
Observer Events → NATS → Arrow Flight Bridge
                            ↓
                    Arrow RecordBatches
                            ↓
                    ClickHouse Sink
                            ↓
            ┌───────────────┴────────────────┐
            ↓                                ↓
    Raw Events Table              Materialized Views
    (MergeTree, 90-day TTL)      (hourly/daily aggregations)
```

---

## Files to Create

### 1. ClickHouse Sink

**File**: `crates/fraiseql-arrow/src/clickhouse_sink.rs`
- Subscribe to Arrow batch stream
- Insert batches into ClickHouse via Arrow Flight interface
- Error handling + retry logic

### 2. Schema Migration

**File**: `crates/fraiseql-observers/migrations/clickhouse/001_events_table.sql`
- Create `fraiseql_events` table
- Create materialized views
- Set up retention policies

### 3. Configuration

**File**: `crates/fraiseql-observers/src/config.rs`
- ClickHouse connection settings
- Batch insert configuration
- TTL settings

---

## Implementation Steps

### Step 1: ClickHouse Table Schema (1 hour)

**File**: `migrations/clickhouse/001_events_table.sql`

```sql
-- Raw events table (MergeTree for inserts + TTL)
CREATE TABLE IF NOT EXISTS fraiseql_events (
    event_id String,
    event_type String,
    entity_type String,
    entity_id String,
    timestamp DateTime64(6, 'UTC'),
    data String,  -- JSON as string for flexibility
    user_id Nullable(String),
    org_id Nullable(String),

    -- Indexes for common queries
    INDEX idx_entity_type entity_type TYPE bloom_filter GRANULARITY 1,
    INDEX idx_event_type event_type TYPE bloom_filter GRANULARITY 1,
    INDEX idx_org_id org_id TYPE bloom_filter GRANULARITY 1
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (entity_type, timestamp)
TTL timestamp + INTERVAL 90 DAY  -- Auto-delete after 90 days
SETTINGS index_granularity = 8192;

-- Materialized view: Hourly event counts by type
CREATE MATERIALIZED VIEW IF NOT EXISTS fraiseql_events_hourly
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMM(hour)
ORDER BY (entity_type, event_type, hour)
AS SELECT
    entity_type,
    event_type,
    toStartOfHour(timestamp) AS hour,
    count() AS event_count
FROM fraiseql_events
GROUP BY entity_type, event_type, hour;

-- Materialized view: Daily aggregations per organization
CREATE MATERIALIZED VIEW IF NOT EXISTS fraiseql_org_daily
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMM(day)
ORDER BY (org_id, day)
AS SELECT
    org_id,
    toDate(timestamp) AS day,
    count() AS total_events,
    uniqExact(entity_id) AS unique_entities,
    uniqExact(user_id) AS unique_users
FROM fraiseql_events
WHERE org_id IS NOT NULL
GROUP BY org_id, day;
```

**Verification**:
```bash
# Apply migration
clickhouse-client --host localhost --port 9000 < migrations/clickhouse/001_events_table.sql

# Verify tables exist
clickhouse-client --query "SHOW TABLES FROM default"
# Expected: fraiseql_events, fraiseql_events_hourly, fraiseql_org_daily
```

---

### Step 2: ClickHouse Sink Implementation (2-3 hours)

**File**: `crates/fraiseql-arrow/src/clickhouse_sink.rs`

```rust
use arrow::record_batch::RecordBatch;
use clickhouse::Client;
use tokio::sync::mpsc;
use tracing::{info, warn, error};

/// Configuration for ClickHouse sink.
#[derive(Debug, Clone)]
pub struct ClickHouseSinkConfig {
    pub url: String,
    pub database: String,
    pub table: String,
    pub batch_insert_timeout_secs: u64,
}

impl Default for ClickHouseSinkConfig {
    fn default() -> Self {
        Self {
            url: "http://localhost:8123".to_string(),
            database: "default".to_string(),
            table: "fraiseql_events".to_string(),
            batch_insert_timeout_secs: 5,
        }
    }
}

/// ClickHouse sink for Arrow RecordBatches.
///
/// Consumes Arrow batches from a channel and inserts them into ClickHouse
/// using native Arrow format (via ClickHouse Arrow Flight interface).
pub struct ClickHouseSink {
    client: Client,
    config: ClickHouseSinkConfig,
}

impl ClickHouseSink {
    pub async fn new(config: ClickHouseSinkConfig) -> Result<Self, clickhouse::error::Error> {
        let client = Client::default()
            .with_url(&config.url)
            .with_database(&config.database);

        Ok(Self { client, config })
    }

    /// Start consuming Arrow batches and inserting into ClickHouse.
    pub async fn start(
        &self,
        mut rx: mpsc::Receiver<RecordBatch>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting ClickHouse sink for table: {}", self.config.table);

        while let Some(batch) = rx.recv().await {
            match self.insert_batch(batch).await {
                Ok(rows_inserted) => {
                    info!("Inserted {} rows into ClickHouse", rows_inserted);
                }
                Err(e) => {
                    error!("Failed to insert batch into ClickHouse: {}", e);
                    // TODO: Implement retry logic or DLQ
                }
            }
        }

        info!("ClickHouse sink stopped");
        Ok(())
    }

    /// Insert a single Arrow RecordBatch into ClickHouse.
    async fn insert_batch(&self, batch: RecordBatch) -> Result<usize, Box<dyn std::error::Error>> {
        let num_rows = batch.num_rows();

        // Convert Arrow batch to ClickHouse insert
        // ClickHouse natively supports Arrow format via Arrow Flight or HTTP

        // For Phase 9.4, we'll use the ClickHouse Rust client's native support
        // The client can insert RecordBatches directly

        // TODO: Use ClickHouse Arrow Flight interface for maximum performance
        // For now, convert to row format and insert via HTTP

        self.insert_batch_via_http(batch).await?;

        Ok(num_rows)
    }

    /// Insert batch via ClickHouse HTTP interface (fallback).
    ///
    /// Phase 9.4 MVP uses HTTP. Future optimization: use Arrow Flight.
    async fn insert_batch_via_http(
        &self,
        batch: RecordBatch,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Extract columns from Arrow batch
        // Build INSERT statement
        // Send via ClickHouse client

        // Placeholder implementation - will be completed in full implementation
        // The clickhouse crate supports inserting rows directly

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires ClickHouse running
    async fn test_clickhouse_connection() {
        let config = ClickHouseSinkConfig::default();
        let sink = ClickHouseSink::new(config).await;
        assert!(sink.is_ok());
    }
}
```

---

### Step 3: Wire Up Observer Events → ClickHouse (1-2 hours)

**File**: `crates/fraiseql-observers/src/main.rs` (if CLI exists) or integration example

```rust
use fraiseql_observers::arrow_bridge::NatsArrowBridge;
use fraiseql_arrow::clickhouse_sink::{ClickHouseSink, ClickHouseSinkConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Connect to NATS
    let client = async_nats::connect("nats://localhost:4222").await?;
    let jetstream = async_nats::jetstream::new(client);

    // 2. Start NATS → Arrow bridge
    let bridge = NatsArrowBridge::new(
        jetstream,
        "fraiseql_events".to_string(),
        "clickhouse_consumer".to_string(),
        10_000, // batch size
    );

    let batch_rx = bridge.start().await?;

    // 3. Start ClickHouse sink
    let sink_config = ClickHouseSinkConfig {
        url: "http://localhost:8123".to_string(),
        database: "default".to_string(),
        table: "fraiseql_events".to_string(),
        batch_insert_timeout_secs: 5,
    };

    let sink = ClickHouseSink::new(sink_config).await?;

    // 4. Run sink (blocking)
    sink.start(batch_rx).await?;

    Ok(())
}
```

---

### Step 4: Docker Compose for Testing (30 min)

**File**: `docker-compose.clickhouse.yml`

```yaml
version: '3.8'

services:
  clickhouse:
    image: clickhouse/clickhouse-server:24
    container_name: fraiseql-clickhouse
    ports:
      - "8123:8123"  # HTTP interface
      - "9000:9000"  # Native protocol
    environment:
      CLICKHOUSE_DB: default
      CLICKHOUSE_USER: default
      CLICKHOUSE_PASSWORD: ""
    volumes:
      - clickhouse_data:/var/lib/clickhouse
      - ./migrations/clickhouse:/docker-entrypoint-initdb.d
    healthcheck:
      test: ["CMD", "clickhouse-client", "--query", "SELECT 1"]
      interval: 10s
      timeout: 5s
      retries: 5

volumes:
  clickhouse_data:
```

**Verification**:
```bash
docker-compose -f docker-compose.clickhouse.yml up -d

# Wait for health check
docker-compose -f docker-compose.clickhouse.yml ps

# Verify tables created
docker exec fraiseql-clickhouse clickhouse-client --query "SHOW TABLES"
```

---

## Verification Commands

```bash
# 1. Start ClickHouse
docker-compose -f docker-compose.clickhouse.yml up -d

# 2. Apply migrations
docker exec fraiseql-clickhouse clickhouse-client --multiquery < migrations/clickhouse/001_events_table.sql

# 3. Compile ClickHouse sink
cd crates/fraiseql-arrow
cargo check --features clickhouse

# 4. Run integration test (requires NATS + ClickHouse)
cargo test --test clickhouse_integration_test

# 5. Query ClickHouse to verify data
docker exec fraiseql-clickhouse clickhouse-client --query "SELECT count() FROM fraiseql_events"

# Expected:
# ✅ Tables created successfully
# ✅ Sink compiles and runs
# ✅ Events appear in ClickHouse
```

---

## Acceptance Criteria

- ✅ ClickHouse tables created (events, materialized views)
- ✅ TTL configured (90-day retention)
- ✅ ClickHouseSink consumes Arrow batches
- ✅ Batches inserted into ClickHouse successfully
- ✅ Materialized views update automatically
- ✅ Docker Compose configuration for local testing
- ✅ Error handling + logging in place
- ✅ Integration test validates end-to-end pipeline

---

## Performance Targets

- **Ingestion**: 1M+ events/sec
- **Latency**: <100ms batch insert (10k events)
- **Retention**: 90-day TTL (automatic cleanup)
- **Compression**: 10:1 ratio (columnar MergeTree)

---

## Next Steps

**[Phase 9.5: Elasticsearch Integration](./phase-9.5-elasticsearch-integration.md)**

This phase adds the operational dataplane (JSONB + Elasticsearch) for debugging and search workflows.

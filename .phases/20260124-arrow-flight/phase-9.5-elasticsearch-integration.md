# Phase 9.5: Elasticsearch Integration (Operational Dataplane)

**Duration**: 3-4 days
**Priority**: ⭐⭐⭐⭐
**Dependencies**: Phase 9.3 complete
**Status**: Ready to implement (parallel with 9.4)

---

## Objective

Integrate Elasticsearch as the operational database for FraiseQL, providing:
- Fast full-text search for debugging ("find events where error contains X")
- Flexible JSON querying for incident response
- Request/event inspection for support workflows
- GraphQL request logs and error tracking
- Complementary to ClickHouse (not redundant)

**Principle**: ClickHouse = Facts/Metrics, Elasticsearch = Searchable Documents

---

## Architecture

```
Observer Events → NATS → JSONB Indexer
                            ↓
                    Elasticsearch
                            ↓
        ┌───────────────────┴─────────────────┐
        ↓                                      ↓
  Event Index                         Request Log Index
  (debugging, search)                 (GraphQL queries, errors)
```

---

## Files to Create

**File**: `crates/fraiseql-observers/src/elasticsearch_sink.rs`
- Subscribe to NATS events
- Index events as JSONB documents
- Bulk indexing with retry logic

**File**: `migrations/elasticsearch/events_index.json`
- Index template for events
- Field mappings
- Retention policies (ILM)

**File**: `migrations/elasticsearch/requests_index.json`
- Index template for GraphQL requests
- Error tracking fields

---

## Implementation Steps

### Step 1: Elasticsearch Index Templates (1 hour)

**File**: `migrations/elasticsearch/events_index.json`

```json
{
  "index_patterns": ["fraiseql-events-*"],
  "template": {
    "settings": {
      "number_of_shards": 3,
      "number_of_replicas": 1,
      "index.lifecycle.name": "fraiseql-events-policy",
      "index.lifecycle.rollover_alias": "fraiseql-events"
    },
    "mappings": {
      "properties": {
        "event_id": { "type": "keyword" },
        "event_type": { "type": "keyword" },
        "entity_type": { "type": "keyword" },
        "entity_id": { "type": "keyword" },
        "timestamp": { "type": "date" },
        "data": {
          "type": "object",
          "enabled": true
        },
        "user_id": { "type": "keyword" },
        "org_id": { "type": "keyword" }
      }
    }
  }
}
```

**File**: `migrations/elasticsearch/ilm_policy.json` (Index Lifecycle Management)

```json
{
  "policy": {
    "phases": {
      "hot": {
        "min_age": "0ms",
        "actions": {
          "rollover": {
            "max_age": "7d",
            "max_size": "50gb"
          }
        }
      },
      "warm": {
        "min_age": "30d",
        "actions": {
          "shrink": {
            "number_of_shards": 1
          }
        }
      },
      "delete": {
        "min_age": "90d",
        "actions": {
          "delete": {}
        }
      }
    }
  }
}
```

**Verification**:
```bash
# Apply index template
curl -X PUT "localhost:9200/_index_template/fraiseql-events" \
  -H 'Content-Type: application/json' \
  -d @migrations/elasticsearch/events_index.json

# Apply ILM policy
curl -X PUT "localhost:9200/_ilm/policy/fraiseql-events-policy" \
  -H 'Content-Type: application/json' \
  -d @migrations/elasticsearch/ilm_policy.json

# Verify
curl "localhost:9200/_index_template/fraiseql-events"
```

---

### Step 2: Elasticsearch Sink (2-3 hours)

**File**: `crates/fraiseql-observers/src/elasticsearch_sink.rs`

```rust
use elasticsearch::{Elasticsearch, BulkParts, http::transport::Transport};
use serde_json::json;
use tokio::sync::mpsc;
use tracing::{info, warn, error};
use crate::EntityEvent;

#[derive(Debug, Clone)]
pub struct ElasticsearchSinkConfig {
    pub url: String,
    pub index_prefix: String,
    pub bulk_size: usize,
    pub flush_interval_secs: u64,
}

impl Default for ElasticsearchSinkConfig {
    fn default() -> Self {
        Self {
            url: "http://localhost:9200".to_string(),
            index_prefix: "fraiseql-events".to_string(),
            bulk_size: 1000,
            flush_interval_secs: 5,
        }
    }
}

pub struct ElasticsearchSink {
    client: Elasticsearch,
    config: ElasticsearchSinkConfig,
}

impl ElasticsearchSink {
    pub async fn new(config: ElasticsearchSinkConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let transport = Transport::single_node(&config.url)?;
        let client = Elasticsearch::new(transport);

        Ok(Self { client, config })
    }

    /// Start consuming events and indexing into Elasticsearch.
    pub async fn start(
        &self,
        mut rx: mpsc::Receiver<EntityEvent>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting Elasticsearch sink");

        let mut event_buffer = Vec::with_capacity(self.config.bulk_size);

        while let Some(event) = rx.recv().await {
            event_buffer.push(event);

            if event_buffer.len() >= self.config.bulk_size {
                self.flush_buffer(&mut event_buffer).await?;
            }
        }

        // Flush remaining events
        if !event_buffer.is_empty() {
            self.flush_buffer(&mut event_buffer).await?;
        }

        info!("Elasticsearch sink stopped");
        Ok(())
    }

    /// Flush event buffer to Elasticsearch using bulk API.
    async fn flush_buffer(
        &self,
        buffer: &mut Vec<EntityEvent>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if buffer.is_empty() {
            return Ok(());
        }

        let mut body: Vec<serde_json::Value> = Vec::new();

        for event in buffer.iter() {
            // Bulk API format: action metadata, then document
            let index_name = format!("{}-{}", self.config.index_prefix,
                                     event.timestamp.format("%Y.%m"));

            body.push(json!({
                "index": {
                    "_index": index_name,
                    "_id": event.id.to_string()
                }
            }));

            body.push(json!({
                "event_id": event.id.to_string(),
                "event_type": event.event_type,
                "entity_type": event.entity_type,
                "entity_id": event.entity_id,
                "timestamp": event.timestamp.to_rfc3339(),
                "data": event.data,
                "user_id": event.user_id,
                "org_id": event.org_id,
            }));
        }

        let response = self
            .client
            .bulk(BulkParts::None)
            .body(body)
            .send()
            .await?;

        let response_body = response.json::<serde_json::Value>().await?;

        if response_body["errors"].as_bool().unwrap_or(false) {
            warn!("Bulk indexing had errors: {:?}", response_body);
        } else {
            info!("Indexed {} events into Elasticsearch", buffer.len());
        }

        buffer.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires Elasticsearch running
    async fn test_elasticsearch_connection() {
        let config = ElasticsearchSinkConfig::default();
        let sink = ElasticsearchSink::new(config).await;
        assert!(sink.is_ok());
    }
}
```

---

### Step 3: Docker Compose (30 min)

**File**: `docker-compose.elasticsearch.yml`

```yaml
version: '3.8'

services:
  elasticsearch:
    image: elasticsearch:8.15.0
    container_name: fraiseql-elasticsearch
    environment:
      - discovery.type=single-node
      - xpack.security.enabled=false
      - "ES_JAVA_OPTS=-Xms512m -Xmx512m"
    ports:
      - "9200:9200"
      - "9300:9300"
    volumes:
      - es_data:/usr/share/elasticsearch/data
    healthcheck:
      test: ["CMD-SHELL", "curl -f http://localhost:9200/_cluster/health || exit 1"]
      interval: 30s
      timeout: 10s
      retries: 5

  kibana:
    image: kibana:8.15.0
    container_name: fraiseql-kibana
    ports:
      - "5601:5601"
    environment:
      ELASTICSEARCH_HOSTS: http://elasticsearch:9200
    depends_on:
      elasticsearch:
        condition: service_healthy

volumes:
  es_data:
```

---

### Step 4: Search Examples (1 hour)

**File**: `examples/elasticsearch_queries.sh`

```bash
#!/bin/bash

# Search for errors
curl -X POST "localhost:9200/fraiseql-events-*/_search?pretty" -H 'Content-Type: application/json' -d'
{
  "query": {
    "bool": {
      "must": [
        { "match": { "event_type": "Order.Failed" }}
      ],
      "filter": [
        { "range": { "timestamp": { "gte": "now-24h" }}}
      ]
    }
  },
  "sort": [{ "timestamp": "desc" }],
  "size": 100
}
'

# Aggregate events by type (last 7 days)
curl -X POST "localhost:9200/fraiseql-events-*/_search?pretty" -H 'Content-Type: application/json' -d'
{
  "size": 0,
  "query": {
    "range": { "timestamp": { "gte": "now-7d" }}
  },
  "aggs": {
    "by_type": {
      "terms": { "field": "event_type" }
    }
  }
}
'

# Find events for specific user
curl -X POST "localhost:9200/fraiseql-events-*/_search?pretty" -H 'Content-Type: application/json' -d'
{
  "query": {
    "term": { "user_id": "user-123" }
  },
  "sort": [{ "timestamp": "desc" }]
}
'
```

---

## Verification Commands

```bash
# 1. Start Elasticsearch + Kibana
docker-compose -f docker-compose.elasticsearch.yml up -d

# 2. Wait for health check
docker-compose -f docker-compose.elasticsearch.yml ps

# 3. Apply index templates
./scripts/setup_elasticsearch.sh

# 4. Run integration test
cargo test --test elasticsearch_integration_test

# 5. Query via Kibana
open http://localhost:5601

# Expected:
# ✅ Events indexed in Elasticsearch
# ✅ Full-text search works
# ✅ Kibana visualizations available
```

---

## Acceptance Criteria

- ✅ Elasticsearch index template configured
- ✅ ILM policy applied (90-day retention)
- ✅ ElasticsearchSink consumes events from NATS
- ✅ Bulk indexing works (1k events per batch)
- ✅ Search queries functional (full-text, range, aggregations)
- ✅ Kibana dashboard accessible
- ✅ Docker Compose for local development
- ✅ Integration tests passing

---

## Use Cases Enabled

1. **Incident Response**: "Find all errors for org-123 in the last hour"
2. **Debugging**: "Show me all Order.Failed events with error_code=PAYMENT_DECLINED"
3. **Audit Trail**: "Which user triggered these events?"
4. **Pattern Detection**: "Aggregate events by type for anomaly detection"

---

## Next Steps

**[Phase 9.6: Cross-Language Client Examples](./phase-9.6-client-examples.md)**

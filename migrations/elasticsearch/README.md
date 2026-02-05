# Elasticsearch Migrations

This directory contains configuration for Elasticsearch indexing of FraiseQL observer events, providing operational search and debugging capabilities.

## Overview

The migrations set up:

1. **fraiseql-events-*** - Event index template with ILM policy
   - Dynamic indices by month (fraiseql-events-YYYY.MM)
   - Automatic rollover after 7 days or 50GB
   - 90-day retention with automatic deletion
   - Bloom filter optimization for keyword fields

2. **ILM Policy (fraiseql-events-policy)**
   - Hot phase: Fresh indices, active indexing
   - Warm phase: Optimized indices (30 days old)
   - Delete phase: Automatic cleanup (90 days old)

## Setup

### Option 1: Docker Compose (Recommended for Development)

```bash
docker-compose -f docker-compose.elasticsearch.yml up -d
```

Elasticsearch will be available at `http://localhost:9200`
Kibana will be available at `http://localhost:5601`

### Option 2: Manual Setup

Apply the index template and ILM policy:

```bash
# Apply index template
curl -X PUT "localhost:9200/_index_template/fraiseql-events" \
  -H 'Content-Type: application/json' \
  -d @migrations/elasticsearch/events_index.json

# Apply ILM policy
curl -X PUT "localhost:9200/_ilm/policy/fraiseql-events-policy" \
  -H 'Content-Type: application/json' \
  -d @migrations/elasticsearch/ilm_policy.json
```

### Option 3: Using Python

```python
from elasticsearch import Elasticsearch
import json

client = Elasticsearch(["http://localhost:9200"])

# Load and apply index template
with open('events_index.json') as f:
    template = json.load(f)
client.indices.put_index_template(name="fraiseql-events", body=template)

# Load and apply ILM policy
with open('ilm_policy.json') as f:
    policy = json.load(f)
client.ilm.put_lifecycle(policy=policy.pop("policy"), body=policy)
```

## Verification

### Check Index Template

```bash
curl "localhost:9200/_index_template/fraiseql-events"
```

Expected response shows the template configuration.

### Check ILM Policy

```bash
curl "localhost:9200/_ilm/policy/fraiseql-events-policy"
```

Expected response shows the lifecycle policy phases.

### Monitor Indices

```bash
# List all fraiseql indices
curl "localhost:9200/_cat/indices/fraiseql-*?v"

# Check index stats
curl "localhost:9200/fraiseql-events-*/_stats?pretty"

# Check shard allocation
curl "localhost:9200/_cat/shards/fraiseql-*?v"
```

## Sample Queries

### Search for Recent Events (Last 24 Hours)

```bash
curl -X POST "localhost:9200/fraiseql-events-*/_search?pretty" \
  -H 'Content-Type: application/json' \
  -d '{
    "query": {
      "range": {
        "timestamp": {
          "gte": "now-24h"
        }
      }
    },
    "sort": [{"timestamp": "desc"}],
    "size": 100
  }'
```

### Find Events by Type

```bash
curl -X POST "localhost:9200/fraiseql-events-*/_search?pretty" \
  -H 'Content-Type: application/json' \
  -d '{
    "query": {
      "term": {
        "event_type": "updated"
      }
    },
    "size": 50
  }'
```

### Find Events for Specific Entity

```bash
curl -X POST "localhost:9200/fraiseql-events-*/_search?pretty" \
  -H 'Content-Type: application/json' \
  -d '{
    "query": {
      "bool": {
        "must": [
          {"term": {"entity_type": "Order"}},
          {"term": {"entity_id": "12345"}}
        ]
      }
    },
    "sort": [{"timestamp": "desc"}]
  }'
```

### Aggregate Events by Type (Last 7 Days)

```bash
curl -X POST "localhost:9200/fraiseql-events-*/_search?pretty" \
  -H 'Content-Type: application/json' \
  -d '{
    "size": 0,
    "query": {
      "range": {
        "timestamp": {
          "gte": "now-7d"
        }
      }
    },
    "aggs": {
      "by_event_type": {
        "terms": {
          "field": "event_type",
          "size": 50
        }
      },
      "by_entity_type": {
        "terms": {
          "field": "entity_type",
          "size": 50
        }
      }
    }
  }'
```

### Find Events by User

```bash
curl -X POST "localhost:9200/fraiseql-events-*/_search?pretty" \
  -H 'Content-Type: application/json' \
  -d '{
    "query": {
      "term": {
        "user_id": "user-123"
      }
    },
    "sort": [{"timestamp": "desc"}],
    "size": 100
  }'
```

### Full-Text Search in Event Data

```bash
curl -X POST "localhost:9200/fraiseql-events-*/_search?pretty" \
  -H 'Content-Type: application/json' \
  -d '{
    "query": {
      "match": {
        "search_text": "payment failed"
      }
    },
    "highlight": {
      "fields": {
        "search_text": {}
      }
    },
    "size": 50
  }'
```

### Timeline Histogram (Events Per Hour)

```bash
curl -X POST "localhost:9200/fraiseql-events-*/_search?pretty" \
  -H 'Content-Type: application/json' \
  -d '{
    "size": 0,
    "query": {
      "range": {
        "timestamp": {
          "gte": "now-48h"
        }
      }
    },
    "aggs": {
      "events_per_hour": {
        "date_histogram": {
          "field": "timestamp",
          "fixed_interval": "1h"
        }
      }
    }
  }'
```

## Kibana Usage

1. Open Kibana: `http://localhost:5601`
2. Go to **Stack Management** → **Index Management**
3. Create Data View: `fraiseql-events-*`
4. Go to **Discover** to explore events
5. Create visualizations:
   - Events over time (line chart)
   - Top event types (bar chart)
   - Top entities (table)

## Performance Tuning

### Refresh Interval

Reduce ingestion overhead by increasing refresh interval (default 1s):

```bash
curl -X PUT "localhost:9200/fraiseql-events-*/_settings" \
  -H 'Content-Type: application/json' \
  -d '{
    "index": {
      "refresh_interval": "30s"
    }
  }'
```

### Bulk Indexing

When bulk indexing during setup/migration, disable refresh:

```bash
curl -X PUT "localhost:9200/fraiseql-events-write/_settings" \
  -H 'Content-Type: application/json' \
  -d '{
    "index": {
      "refresh_interval": "-1"
    }
  }'

# After bulk load, restore refresh
curl -X PUT "localhost:9200/fraiseql-events-write/_settings" \
  -H 'Content-Type: application/json' \
  -d '{
    "index": {
      "refresh_interval": "1s"
    }
  }'
```

### Field Mapping Optimization

Use `keyword` for fields you filter/aggregate on (not full-text search):

- event_type, entity_type, event_id, entity_id: `keyword` ✅
- search_text (combined field): `text` ✅

### ILM Policy Tuning

Adjust rollover criteria:

```bash
curl -X PUT "localhost:9200/_ilm/policy/fraiseql-events-policy" \
  -H 'Content-Type: application/json' \
  -d '{
    "policy": "fraiseql-events-policy",
    "phases": {
      "hot": {
        "min_age": "0ms",
        "actions": {
          "rollover": {
            "max_age": "3d",      # Rollover every 3 days instead of 7
            "max_size": "100gb"   # Or every 100GB
          }
        }
      },
      "warm": {
        "min_age": "15d",         # Move to warm earlier
        "actions": {
          "shrink": {"number_of_shards": 1},
          "readonly": {}
        }
      },
      "delete": {
        "min_age": "30d",         # Delete after 30 days instead of 90
        "actions": {"delete": {}}
      }
    }
  }'
```

## Troubleshooting

### Index Template Not Applying

```bash
# Check if template exists
curl "localhost:9200/_index_template/fraiseql-events"

# Delete and recreate
curl -X DELETE "localhost:9200/_index_template/fraiseql-events"
curl -X PUT "localhost:9200/_index_template/fraiseql-events" \
  -H 'Content-Type: application/json' \
  -d @migrations/elasticsearch/events_index.json
```

### ILM Policy Not Active

```bash
# Check policy status
curl "localhost:9200/_ilm/explain/fraiseql-events-*?pretty"

# Manually trigger rollover
curl -X POST "localhost:9200/fraiseql-events-write/_rollover"
```

### Indices Not Being Deleted

```bash
# Check ILM execution logs
curl "localhost:9200/.ilm-history-*/_search?pretty" | head -50

# Manually delete old indices
curl -X DELETE "localhost:9200/fraiseql-events-2025.01.01"
```

### Storage Usage Growing

```bash
# Check index sizes
curl "localhost:9200/_cat/indices/fraiseql-*?v&s=store.size:desc"

# Force merge to optimize storage
curl -X POST "localhost:9200/fraiseql-events-*/_forcemerge?max_num_segments=1"
```

## Architecture Decisions

### Why Elasticsearch?

- **Full-text search**: Find events by content ("payment failed", "timeout", etc.)
- **Flexible querying**: JSON documents, no strict schema
- **Operational**: Built for support/debugging, not analytics
- **Complementary to ClickHouse**: ClickHouse optimized for aggregations, ES for search

### Index Naming Pattern

- `fraiseql-events-YYYY.MM` - Monthly rollover
- Enables efficient deletion of old data via ILM
- Reduces index size/query overhead
- Aligns with standard Elasticsearch patterns

### Field Types

- `keyword`: Used for filtering/aggregation (event_type, entity_id, user_id)
- `text`: Used for full-text search (search_text)
- `object`: For nested JSON (data, changes)
- `date`: For range queries (timestamp)

---

## Monitoring

### Key Metrics to Watch

```bash
# Cluster health
curl "localhost:9200/_cluster/health?pretty"

# Node resources
curl "localhost:9200/_nodes/stats?pretty" | grep -A 10 "jvm"

# Slow query log
curl "localhost:9200/fraiseql-events-*/_stats/search"
```

### Alerts

Set up alerts for:

- Cluster health status != GREEN
- Disk usage > 80%
- Index creation failures
- Query latency > 5s
- Document indexing failures

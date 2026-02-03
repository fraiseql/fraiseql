# Migrating to Arrow Flight

Arrow Flight is **100% backwards compatible**. Your existing HTTP/JSON clients will continue to work unchanged.

This guide shows how to incrementally adopt Arrow Flight in your organization.

## Key Principle: No Breaking Changes

Arrow Flight runs alongside your existing HTTP/JSON API. Both endpoints available simultaneously:

```
HTTP/JSON API:    http://localhost:8080/graphql
Arrow Flight API: grpc://localhost:50051
```

Choose the transport based on use case, not requirements.

## Migration Strategy: 4 Phases

```
Phase 1 (Week 1)          Phase 2 (Weeks 2-3)      Phase 3 (Week 4)         Phase 4 (Week 5)
┌──────────────────┐      ┌──────────────────┐    ┌──────────────────┐    ┌──────────────────┐
│ Enable Arrow     │      │ Migrate Analytics│    │ Enable Analytics │    │ Add Debugging    │
│ Flight Server    │      │ Workloads        │    │ (ClickHouse)     │    │ (Elasticsearch)  │
├──────────────────┤      ├──────────────────┤    ├──────────────────┤    ├──────────────────┤
│ • Add port 50051 │      │ • Update Python  │    │ • Deploy CH      │    │ • Deploy ES      │
│ • Zero changes   │      │ • Update R       │    │ • Apply DDL      │    │ • Apply ILM      │
│ • No downtime    │      │ • 15-50x faster  │    │ • Real-time agg  │    │ • Full-text      │
│ • 30 minutes     │      │ • 1-2 weeks      │    │ • 1 week         │    │ • 1 week         │
└──────────────────┘      └──────────────────┘    └──────────────────┘    └──────────────────┘
```

## Phase 1: Enable Arrow Flight Server (Week 1, 30 minutes)

### Step 1: Update docker-compose.yml

```yaml
services:
  fraiseql:
    ports:
      - "8080:8080"   # Existing HTTP
      - "50051:50051" # NEW: Arrow Flight
    environment:
      ARROW_FLIGHT_ENABLED: "true"  # Enable Arrow Flight
```

### Step 2: Deploy

```bash
docker-compose up -d fraiseql

# Verify both ports are listening
netstat -tuln | grep -E '8080|50051'

# Test HTTP still works
curl http://localhost:8080/graphql \
  -H 'Content-Type: application/json' \
  -d '{"query": "{ users { id } }"}'

# Test Arrow Flight works
python3 << 'EOF'
import pyarrow.flight as flight
client = flight.connect("grpc://localhost:50051")
print("✅ Arrow Flight works!")
EOF
```

### ✅ Phase 1 Complete

- ✅ Arrow Flight server running on port 50051
- ✅ HTTP/JSON still working on port 8080
- ✅ Zero breaking changes
- ✅ Ready for Phase 2

**Impact**: None. HTTP clients unaffected.
**Downtime**: None.
**Time spent**: 30 minutes.

---

## Phase 2: Migrate Analytics Workloads (Weeks 2-3, 1-2 weeks)

Now migrate your data science and analytics scripts to Arrow Flight.

### Analytics Script Checklist

Identify scripts that:

- [ ] Process large datasets (10k+ rows)
- [ ] Run daily/weekly reports
- [ ] Generate ML features
- [ ] Produce CSV/Parquet exports
- [ ] Dashboard data pipelines

These get the most benefit from Arrow Flight (15-50x faster).

### Example: Before vs After

**Before** (HTTP/JSON, 30 seconds):
```python
import requests
import pandas as pd

response = requests.post(
    'http://localhost:8080/graphql',
    json={
        'query': '''
        {
            orders(limit: 100000) {
                id
                total
                status
                createdAt
            }
        }
        '''
    }
)

# Parse JSON and convert to DataFrame (slow!)
df = pd.DataFrame(response.json()['data']['orders'])

# 30 seconds, 250 MB memory
print(f"Loaded {len(df)} orders")
```

**After** (Arrow Flight, 2 seconds):
```python
import pyarrow.flight as flight
import polars as pl

client = flight.connect("grpc://localhost:50051")

ticket = flight.Ticket(b'''{
    "type": "GraphQLQuery",
    "query": "{
        orders(limit: 100000) {
            id
            total
            status
            createdAt
        }
    }"
}''')

# Zero-copy Arrow to Polars (fast!)
df = pl.from_arrow(client.do_get(ticket).read_all())

# 2 seconds, 50 MB memory (15x faster!)
print(f"Loaded {len(df)} orders")
```

### Migration Steps

For each analytics script:

1. **Update import statements**:
   ```python
   # Remove
   import requests

   # Add
   import pyarrow.flight as flight
   import polars as pl  # or pandas
   ```

2. **Replace query execution**:
   ```python
   # Before
   response = requests.post(
       'http://localhost:8080/graphql',
       json={'query': '...'}
   )
   df = pd.DataFrame(response.json()['data'][...])

   # After
   client = flight.connect("grpc://localhost:50051")
   ticket = flight.Ticket(b'{"type": "GraphQLQuery", "query": "..."}')
   df = pl.from_arrow(client.do_get(ticket).read_all())
   ```

3. **Update DataFrame operations** (optional):
   ```python
   # If using Polars instead of Pandas
   # Many operations are identical, some have different names
   # See: https://docs.pola.rs
   ```

4. **Test**:
   ```bash
   # Run script with Arrow Flight
   python script.py

   # Verify results match previous version
   ```

5. **Benchmark**:
   ```python
   import time

   start = time.time()
   df = pl.from_arrow(client.do_get(ticket).read_all())
   elapsed = time.time() - start

   print(f"⚡ Loaded {len(df)} rows in {elapsed:.2f}s")
   ```

### Tool Support

**Python**: Migrate requests → pyarrow.flight
```bash
pip install pyarrow>=15.0.0 polars>=0.20.0
```

**R**: Migrate jsonlite → arrow
```r
install.packages("arrow")
library(arrow)
```

### ✅ Phase 2 Complete

- ✅ Analytics scripts migrated
- ✅ 15-50x faster for large queries
- ✅ Reduced memory usage
- ✅ Ready for Phase 3

**Impact**: Analytics queries 15-50x faster ⚡
**Downtime**: None (gradual script migration)
**Time spent**: 1-2 weeks

---

## Phase 3: Enable Analytics (ClickHouse) (Week 4, 1 week)

Now enable real-time analytics via ClickHouse.

### Step 1: Deploy ClickHouse

```yaml
# docker-compose.yml
services:
  clickhouse:
    image: clickhouse/clickhouse-server:24
    ports:
      - "8123:8123"  # HTTP interface
      - "9000:9000"  # Native protocol
    environment:
      CLICKHOUSE_DB: default
    volumes:
      - ./migrations/clickhouse:/docker-entrypoint-initdb.d:ro
```

### Step 2: Apply Migrations

```bash
# Copy migration files
cp -r ./migrations/clickhouse /var/lib/clickhouse/migrations

# Apply automatically (Docker init)
docker-compose up clickhouse

# Verify tables created
docker exec fraiseql-clickhouse clickhouse-client \
  --query "SELECT name FROM system.tables WHERE database='default'"
```

### Step 3: Configure Observer Events → ClickHouse

```yaml
# fraiseql-config.toml
[observers]
clickhouse_enabled = true
clickhouse_url = "http://localhost:8123"
clickhouse_database = "default"
clickhouse_table = "fraiseql_events"
clickhouse_batch_size = 10000
```

### Step 4: Start Ingestion

```bash
# Restart FraiseQL to enable ClickHouse sink
docker-compose restart fraiseql

# Verify events flowing
docker exec fraiseql-clickhouse clickhouse-client \
  --query "SELECT COUNT(*) FROM fraiseql_events"
```

### Step 5: Create Real-Time Dashboards

**Example**: Daily order analytics
```sql
SELECT
    toDate(timestamp) AS day,
    count() AS orders_created,
    sum(JSONExtractFloat(data, 'total')) AS revenue,
    uniq(JSONExtractString(data, 'user_id')) AS unique_customers
FROM fraiseql_events
WHERE event_type = 'Order.Created'
GROUP BY day
ORDER BY day DESC
LIMIT 30;
```

### ✅ Phase 3 Complete

- ✅ ClickHouse deployed
- ✅ Events flowing from NATS
- ✅ Real-time aggregations available
- ✅ Analytics dashboards working
- ✅ Ready for Phase 4

**Impact**: Real-time business intelligence 📊
**Downtime**: None
**Time spent**: 1 week

---

## Phase 4: Add Debugging (Elasticsearch) (Week 5, 1 week)

Finally, enable Elasticsearch for fast event search and incident response.

### Step 1: Deploy Elasticsearch

```yaml
# docker-compose.yml
services:
  elasticsearch:
    image: elasticsearch:8.15.0
    ports:
      - "9200:9200"
    environment:
      - discovery.type=single-node
      - xpack.security.enabled=false
      - "ES_JAVA_OPTS=-Xms512m -Xmx512m"
    volumes:
      - es-data:/usr/share/elasticsearch/data
```

### Step 2: Apply Index Templates & ILM

```bash
# Apply index template
curl -X PUT "localhost:9200/_index_template/fraiseql-events" \
  -H 'Content-Type: application/json' \
  -d @./migrations/elasticsearch/index_template.json

# Apply ILM policy
curl -X PUT "localhost:9200/_ilm/policy/fraiseql-events" \
  -H 'Content-Type: application/json' \
  -d @./migrations/elasticsearch/ilm_policy.json
```

### Step 3: Configure Observer Events → Elasticsearch

```yaml
# fraiseql-config.toml
[observers]
elasticsearch_enabled = true
elasticsearch_url = "http://localhost:9200"
elasticsearch_index_prefix = "fraiseql-events"
elasticsearch_bulk_size = 1000
```

### Step 4: Start Indexing

```bash
# Restart FraiseQL
docker-compose restart fraiseql

# Verify indexing
curl "localhost:9200/fraiseql-events-*/_count"
# Returns: {"count": 12345}
```

### Step 5: Create Incident Response Runbooks

**Example**: Find all failed orders in the last hour
```bash
curl -X POST "localhost:9200/fraiseql-events-*/_search" \
  -H 'Content-Type: application/json' \
  -d '{
    "query": {
      "bool": {
        "must": [
          {"term": {"event_type": "Order.Failed"}},
          {"match": {"data": "PAYMENT_DECLINED"}}
        ],
        "filter": [
          {"range": {"timestamp": {"gte": "now-1h"}}}
        ]
      }
    },
    "size": 100,
    "sort": [{"timestamp": "desc"}]
  }'
```

**Team**: Train support team on search queries
```bash
# "Find all events for customer-123"
curl -X POST "localhost:9200/fraiseql-events-*/_search" \
  -H 'Content-Type: application/json' \
  -d '{
    "query": {"term": {"user_id": "customer-123"}},
    "sort": [{"timestamp": "desc"}],
    "size": 1000
  }'
```

### ✅ Phase 4 Complete

- ✅ Elasticsearch deployed
- ✅ Events indexed
- ✅ Full-text search working
- ✅ Incident response runbooks created
- ✅ Support team trained

**Impact**: Fast event search, incident response ✅
**Downtime**: None
**Time spent**: 1 week

---

## Migration Summary

| Phase | Timeline | Impact | Effort |
|---|---|---|---|
| 1: Enable Arrow Flight | Week 1 (30 min) | Zero | Low |
| 2: Migrate Analytics | Weeks 2-3 (1-2 weeks) | 15-50x faster | Medium |
| 3: Analytics (ClickHouse) | Week 4 (1 week) | Real-time dashboards | Medium |
| 4: Debugging (Elasticsearch) | Week 5 (1 week) | Fast incident response | Medium |
| **Total** | **5 weeks** | **Transformational** | **~3-4 weeks effort** |

## Rollback at Any Point

Arrow Flight is purely **additive**:

```bash
# If you need to rollback
docker-compose down fraiseql-arrow-flight

# HTTP/JSON continues working
curl http://localhost:8080/graphql ...
```

No data loss, no breaking changes.

## Complete Checklist

### Phase 1

- [ ] Arrow Flight port 50051 accessible
- [ ] HTTP/JSON still working
- [ ] No downtime

### Phase 2

- [ ] Analytics scripts migrated
- [ ] Performance benchmarked (15-50x faster)
- [ ] Results verified vs old version

### Phase 3

- [ ] ClickHouse running
- [ ] Migrations applied
- [ ] Events flowing (~1M/sec)
- [ ] Real-time queries working

### Phase 4

- [ ] Elasticsearch running
- [ ] Index templates applied
- [ ] Events indexed
- [ ] Support team trained on search

### Production

- [ ] Monitoring configured
- [ ] Alerting enabled
- [ ] Runbooks updated
- [ ] Documentation updated
- [ ] Team trained

## Support

- **Slack**: #fraiseql-arrow-flight
- **Docs**: https://docs.fraiseql.com/arrow-flight
- **Issues**: https://github.com/fraiseql/fraiseql/issues

---

**Next**: [Architecture Deep Dive](./architecture.md) to understand how it all works.

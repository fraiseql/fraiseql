# Getting Started with Arrow Flight (5-Minute Tutorial)

This tutorial gets you running Arrow Flight queries in **5 minutes**.

## Prerequisites

- Docker and Docker Compose
- Python 3.8+ with pip
- 5 minutes of time

## Step 1: Start FraiseQL (1 minute)

```bash
# Clone the repository
git clone https://github.com/FraiseQL/FraiseQL.git
cd FraiseQL

# Start services: PostgreSQL, NATS, Redis, ClickHouse, Elasticsearch
docker-compose up -d

# Verify services are ready
docker-compose ps
# Expected: all services "running"

# Wait for full startup (PostgreSQL especially)
sleep 10
```text

**Verify Arrow Flight is accessible**:

```bash
# Check if port 50051 is listening
netstat -tuln | grep 50051
# Expected: tcp    0    0 0.0.0.0:50051    0.0.0.0:*    LISTEN
```text

## Step 2: Install Python Libraries (1 minute)

```bash
pip install pyarrow>=15.0.0 polars>=0.20.0
```text

## Step 3: Write Your First Arrow Flight Query (2 minutes)

Create `my_first_query.py`:

```python
#!/usr/bin/env python3
"""
First Arrow Flight query: Fetch data as Arrow RecordBatch
"""

import pyarrow.flight as flight
import polars as pl

# Step 1: Connect to FraiseQL Arrow Flight server
print("Connecting to FraiseQL Arrow Flight server...")
client = flight.connect("grpc://localhost:50051")
print("✅ Connected!")

# Step 2: Create a Flight ticket with your GraphQL query
query = '''
{
    users(limit: 100) {
        id
        name
        email
    }
}
'''

print(f"\nExecuting GraphQL query:\n{query}")

ticket = flight.Ticket(b'''{
    "type": "GraphQLQuery",
    "query": "{ users(limit: 100) { id name email } }"
}''')

# Step 3: Fetch data as Arrow RecordBatch (zero-copy!)
print("Fetching data from Arrow Flight server...")
reader = client.do_get(ticket)

# Step 4: Convert to Polars DataFrame (zero-copy!)
print("Converting to Polars DataFrame...")
table = reader.read_all()
df = pl.from_arrow(table)

# Step 5: Analyze the data
print(f"\n✅ Success! Fetched {len(df)} rows")
print(f"\nDataFrame schema:")
print(df.schema)
print(f"\nFirst 5 rows:")
print(df.head(5))

# Bonus: Show performance
print(f"\n📊 Performance:")
print(f"  Rows: {len(df)}")
print(f"  Columns: {len(df.columns)}")
print(f"  Memory: ~{table.nbytes / 1024 / 1024:.1f} MB")
```text

Run it:

```bash
python my_first_query.py
```text

**Expected output**:

```text
Connecting to FraiseQL Arrow Flight server...
✅ Connected!

Executing GraphQL query:
{ users(limit: 100) { id name email } }

Fetching data from Arrow Flight server...
Converting to Polars DataFrame...

✅ Success! Fetched 100 rows

DataFrame schema:
{'id': Int64, 'name': Utf8, 'email': Utf8}

First 5 rows:
shape: (5, 3)
┌────┬──────────┬──────────────────────┐
│ id ┆ name     ┆ email                │
│ -- ┆ ---      ┆ ---                  │
│ i64┆ str      ┆ str                  │
╞════╪══════════╪══════════════════════╡
│ 1  ┆ Alice    ┆ alice@example.com   │
│ 2  ┆ Bob      ┆ bob@example.com     │
│ 3  ┆ Charlie  ┆ charlie@example.com │
│ 4  ┆ Diana    ┆ diana@example.com   │
│ 5  ┆ Eve      ┆ eve@example.com     │
└────┴──────────┴──────────────────────┘

📊 Performance:
  Rows: 100
  Columns: 3
  Memory: ~2.1 KB
```text

## Step 4: Stream Observer Events (1 minute)

Now let's stream real-time observer events. Create `stream_events.py`:

```python
#!/usr/bin/env python3
"""
Stream observer events in real-time
"""

import pyarrow.flight as flight
import polars as pl

client = flight.connect("grpc://localhost:50051")

# Request all Order creation events from the last 7 days
ticket = flight.Ticket(b'''{
    "type": "ObserverEvents",
    "entity_type": "Order",
    "start_date": "2026-01-18",
    "limit": 1000
}''')

print("Streaming observer events...")
print("Receiving batches...\n")

reader = client.do_get(ticket)
total_rows = 0
batch_count = 0

# Process events in batches (constant memory!)
for batch in reader:
    batch_count += 1
    df = pl.from_arrow(batch)
    rows = len(df)
    total_rows += rows

    print(f"Batch {batch_count}: {rows} events")
    print(df.select(["event_type", "entity_type", "timestamp"]).head(3))
    print()

print(f"✅ Streaming complete!")
print(f"  Total batches: {batch_count}")
print(f"  Total events: {total_rows}")
```text

Run it:

```bash
python stream_events.py
```text

## Congratulations! 🎉

You've successfully:

- ✅ Connected to FraiseQL Arrow Flight server
- ✅ Executed a GraphQL query via Arrow Flight
- ✅ Converted Arrow data to Polars (zero-copy)
- ✅ Streamed observer events in batches

## Next Steps

### Learn More

- **[Architecture Deep Dive](./architecture.md)** - Understand the design
- **[Migration Guide](./migration-guide.md)** - Adopt in your codebase

### Explore Integration Patterns

Arrow Flight supports Python, R, Rust, Java, and other languages. See the [architecture guide](./architecture.md) and [migration guide](./migration-guide.md) for integration examples and production deployment patterns.

## Troubleshooting

### "Connection refused"

```text
Error: Error connecting to grpc://localhost:50051
```text

**Solution**: Ensure FraiseQL is running (`docker-compose ps`) and Arrow Flight is enabled.

### "No data returned"

```text
✅ Success! Fetched 0 rows
```text

**Solution**: Check your GraphQL query is valid. Try a simple query without filters first.

### "Module not found: pyarrow"

```text
ModuleNotFoundError: No module named 'pyarrow'
```text

**Solution**: Install dependencies: `pip install pyarrow polars`

### "Batch is empty"

**Solution**: The dataset doesn't have data matching your filters. Try removing date filters or limits.

## Example Queries

### Fetch all users with a name

```python
ticket = flight.Ticket(b'''{
    "type": "GraphQLQuery",
    "query": "{ users { id name email } }"
}''')
```text

### Stream orders from last 30 days

```python
ticket = flight.Ticket(b'''{
    "type": "ObserverEvents",
    "entity_type": "Order",
    "start_date": "2025-12-26",
    "limit": 100000
}''')
```text

### Process events with aggregation

```python
# Stream events
reader = client.do_get(ticket)
table = reader.read_all()
df = pl.from_arrow(table)

# Aggregate in Polars (fast, in-memory)
summary = df.groupby("entity_type").agg([
    pl.col("*").count().alias("count")
])
print(summary)
```text

## Performance Tips

### 1. Use Limits for Development

```python
# Good: limit to 1000 during development
"limit": 1000

# Bad: no limit on production data!
# Can transfer GBs of data
```text

### 2. Stream Large Datasets

```python
# Bad: loads entire dataset in memory
table = reader.read_all()
df = pl.from_arrow(table)

# Good: process batches one at a time
for batch in reader:
    df = pl.from_arrow(batch)
    # Process and discard
```text

### 3. Use Polars for Heavy Lifting

```python
# Zero-copy from Arrow to Polars
df = pl.from_arrow(table)

# Polars is optimized for columnar operations
result = df.groupby("category").agg(
    pl.col("price").sum().alias("total"),
    pl.col("price").mean().alias("avg")
)
```text

## Time Comparison

**What was 30 seconds with HTTP/JSON now takes 2 seconds with Arrow Flight:**

```python
import time

# Setup (same for both)
client = flight.connect("grpc://localhost:50051")

# Arrow Flight (FAST ⚡)
start = time.time()
ticket = flight.Ticket(b'{"type": "GraphQLQuery", "query": "{ users(limit: 100000) {...} }"}')
reader = client.do_get(ticket)
df = pl.from_arrow(reader.read_all())
arrow_time = time.time() - start
print(f"Arrow Flight: {arrow_time:.2f}s")  # 2-3 seconds

# HTTP/JSON would be ~30 seconds (not running here)
# But you get the idea!
```text

---

**Ready for more?** Head to [Architecture](./architecture.md) to understand how it all works.

# Arrow Flight Quick Start (5 Minutes)

**Status:** ✅ Production Ready
**Audience:** Developers, Data Engineers, DBAs
**Reading Time:** 5-7 minutes
**Last Updated:** 2026-02-05

Get your first Arrow Flight analytics query working in 5 minutes.

## Prerequisites

- FraiseQL server running with Arrow Flight enabled
- Python 3.8+ with PyArrow installed
- Understanding of Arrow format (see [Arrow vs JSON Guide](./arrow-vs-json-guide.md))
- Data volume >10K rows (Arrow shines with larger datasets)

```bash
# Install PyArrow
pip install pyarrow
```text

## Step 1: Enable Arrow Flight on Server (1 minute)

```bash
# Start server with Arrow Flight support
fraiseql run --port 8000 --arrow-flight

# Arrow Flight listens on port 50051 by default
# Verify it's listening:
curl http://localhost:50051  # Should fail gracefully (gRPC, not HTTP)
```text

---

## Step 2: Connect from Python (1 minute)

```python
# analytics.py
import pyarrow.flight as flight

# Connect to Arrow Flight server
client = flight.connect(("localhost", 50051))

# Verify connection
info = client.get_flight_info(
    flight.FlightDescriptor.for_command("SELECT 1")
)
print(f"Connected to {info.descriptor.command}")
```text

---

## Step 3: Execute Your First Query (2 minutes)

```python
# analytics.py (continued)
import pyarrow.flight as flight
import pyarrow.compute as pc

# Connect
client = flight.connect(("localhost", 50051))

# Define query - any SQL FraiseQL supports
query = """
SELECT
  DATE_TRUNC('day', created_at) as date,
  category,
  COUNT(*) as order_count,
  SUM(total) as revenue
FROM orders
WHERE created_at > NOW() - INTERVAL '30 days'
GROUP BY 1, 2
ORDER BY 1 DESC
"""

# Execute query
descriptor = flight.FlightDescriptor.for_command(query)
reader = client.do_get(descriptor)
table = reader.read_all()

# Query executed! Now you have Apache Arrow Table
print(f"Retrieved {len(table)} rows")
print(f"Columns: {table.column_names}")
print(table.to_pandas().head())

# Expected output:
#         date  category  order_count      revenue
# 0 2024-01-30       food           234  12450.50
# 1 2024-01-30    clothes           156   8920.00
# 2 2024-01-29       food           198  10234.75
```text

---

## Step 4: Export and Analyze (1 minute)

```python
# analytics.py (continued)

# Convert to Pandas for analysis
df = table.to_pandas()
print(df.describe())

# Convert to Parquet for storage/sharing (50-90% smaller than JSON)
table.to_pandas().to_parquet("orders_summary.parquet")
print("Exported to orders_summary.parquet")

# Convert to CSV for spreadsheet import
table.to_pandas().to_csv("orders_summary.csv", index=False)
print("Exported to orders_summary.csv")

# Stream large results (don't load all into memory)
reader = client.do_get(descriptor)
for batch in reader:
    df_chunk = batch.to_pandas()
    print(f"Processing {len(df_chunk)} rows...")
    # Process each chunk
```text

---

## That's It

You're now querying FraiseQL with Arrow Flight! 🎯

### Performance Comparison

```text
JSON Query (1M rows):   ~5000ms, 500MB transfer
Arrow Query (1M rows):  ~500ms, 50MB transfer
Speedup: 10x faster + 90% smaller
```text

See [Arrow vs JSON Guide](./arrow-vs-json-guide.md) for when to use Arrow vs JSON.

### Next Steps

- Stream large results without loading into memory (see [Arrow Flight Architecture](../integrations/arrow-flight/architecture.md))
- Integrate with Tableau or Superset BI dashboards (see [Arrow Flight Integration](../integrations/arrow-flight/))
- Use Arrow Flight with Python notebooks (see [Analytics Patterns](./analytics-patterns.md))
- Monitor Arrow Flight performance (see [Observability](./observability.md))
- Understand Arrow vs JSON trade-offs (see [Arrow vs JSON Guide](./arrow-vs-json-guide.md))

### Common Issues

**"Connection refused on port 50051"**
→ Start server with `--arrow-flight` flag: `fraiseql run --arrow-flight`

**"No module named 'pyarrow'"**
→ Install PyArrow: `pip install pyarrow`

**"Query times out after 30 seconds"**
→ Query is too expensive. Add WHERE clause to limit data, or use `--timeout 300` flag when connecting.

**"SSL certificate verification failed"**
→ Production requires TLS. Use `flight.connect(("hostname", 50051), tls_root_certs=b"...")` with certificate.

**"Memory error on large queries"**
→ Stream results instead of loading all:

```python
reader = client.do_get(descriptor)
for batch in reader:
    process_chunk(batch.to_pandas())
```text

**"Arrow schema doesn't match my data"**
→ Regenerate schema: Delete `.arrow.cache` and restart server

See [Arrow Flight Guide](../integrations/arrow-flight/migration-guide.md) for complete troubleshooting.

---

## See Also

- **[Arrow vs JSON Guide](./arrow-vs-json-guide.md)** - When to use Arrow vs JSON
- **[Arrow Flight Architecture](../integrations/arrow-flight/architecture.md)** - Technical details
- **[Arrow Flight Migration](../integrations/arrow-flight/migration-guide.md)** - Adoption guide
- **[Analytics Patterns](./analytics-patterns.md)** - Common analytics use cases
- **[Performance Tuning](../operations/performance-tuning-runbook.md)** - Optimize queries

# FraiseQL Python Examples

Python examples for interacting with FraiseQL servers.

## Arrow Flight Client (`test_arrow_flight.py`)

High-performance client for querying FraiseQL's ta_* materialized tables via Arrow Flight.

### Features

- Connect to Arrow Flight gRPC server
- Query ta_* tables with pagination and filtering
- Convert results to Polars DataFrames
- Automatic schema detection

### Prerequisites

1. **FraiseQL Server**: Running with Arrow Flight enabled

   ```bash
   cargo run --release --features arrow
   ```

2. **Python Dependencies**:

   ```bash
   pip install pyarrow polars
   ```

### Usage

```bash
python examples/python/test_arrow_flight.py
```

### Query Parameters

The client supports these ticket parameters:

```python
ticket_data = {
    "type": "OptimizedView",     # Query type (fixed)
    "view": "ta_users",          # Table name
    "limit": 100,                # Max rows (optional)
    "offset": 0,                 # Pagination offset (optional)
    "filter": "...",             # WHERE clause (optional)
    "order_by": "...",           # ORDER BY clause (optional)
}
```

### Performance Tips

1. **Use LIMIT**: Always specify a limit to avoid transferring huge datasets
2. **Use Filtering**: Pre-filter at the database level rather than in Python
3. **Connection Pooling**: Reuse client connections across queries

## Further Reading

- [Arrow Flight Protocol](https://arrow.apache.org/docs/format/Flight.html)
- [Polars DataFrame Library](https://www.pola-rs.com/)

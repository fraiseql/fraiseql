# FraiseQL DDL Generation - Quick Start Guide

## Installation

```bash
# Add tools directory to Python path
export PYTHONPATH=/home/lionel/code/fraiseql/tools:$PYTHONPATH

# Or import directly
python3 -c "
import sys
sys.path.insert(0, '/home/lionel/code/fraiseql/tools')
from fraiseql_tools import load_schema, generate_tv_ddl
"
```

## Quick Examples

### Example 1: Generate JSON View DDL

```python
from fraiseql_tools import load_schema, generate_tv_ddl

# Load your FraiseQL schema
schema = load_schema("schema.json")

# Generate DDL for table-backed JSON view
tv_ddl = generate_tv_ddl(
    schema,
    entity="User",
    view="user",
    refresh_strategy="trigger-based"
)

# Save to file
with open("ddl_user_view.sql", "w") as f:
    f.write(tv_ddl)

# Execute against PostgreSQL
# psql -d mydb -f ddl_user_view.sql
```

### Example 2: Generate Arrow Columnar View

```python
from fraiseql_tools import load_schema, generate_ta_ddl

schema = load_schema("schema.json")

# Generate DDL for Arrow columnar view
ta_ddl = generate_ta_ddl(
    schema,
    entity="Order",
    view="order",
    refresh_strategy="scheduled"
)

# Use with Arrow Flight for streaming
with open("ddl_order_arrow_view.sql", "w") as f:
    f.write(ta_ddl)
```

### Example 3: Auto-Select Refresh Strategy

```python
from fraiseql_tools import suggest_refresh_strategy, generate_tv_ddl

# Analyze your workload
write_volume = 100  # writes per minute
latency_requirement_ms = 500  # acceptable staleness
read_volume = 50000  # reads per minute

# Get recommendation
strategy = suggest_refresh_strategy(write_volume, latency_requirement_ms, read_volume)

# Generate with optimal strategy
ddl = generate_tv_ddl(schema, entity="User", view="user", refresh_strategy=strategy)
```

### Example 4: Generate Composition Views

```python
from fraiseql_tools import generate_composition_views

# Generate helper views for relationships
comp_ddl = generate_composition_views(
    schema,
    entity="User",
    relationships=["posts", "comments"]
)

# Combine with main view DDL
full_ddl = tv_ddl + "\n\n" + comp_ddl
```

### Example 5: Validate Generated DDL

```python
from fraiseql_tools import validate_generated_ddl

errors = validate_generated_ddl(tv_ddl)

if errors:
    print("Validation warnings:")
    for error in errors:
        print(f"  - {error}")
else:
    print("✓ DDL validation passed")
```

## Running Examples

```bash
cd /home/lionel/code/fraiseql

# Run basic example
python3 examples/ddl-generation/example_basic.py

# Run Arrow view example
python3 examples/ddl-generation/example_arrow.py

# Run refresh strategy example
python3 examples/ddl-generation/example_refresh_strategy.py
```

## Functions Reference

### load_schema(path: str) -> dict

Load a FraiseQL schema.json file.

```python
schema = load_schema("schema.json")
```

### generate_tv_ddl(schema, entity, view, refresh_strategy="trigger-based", ...) -> str

Generate DDL for table-backed JSON view.

Parameters:
- `schema`: Schema dict from load_schema()
- `entity`: Entity name (e.g., "User")
- `view`: View suffix without "tv_" (e.g., "user")
- `refresh_strategy`: "trigger-based" or "scheduled" (default: "trigger-based")
- `include_composition_views`: Include composition views (default: True)
- `include_monitoring_functions`: Include monitoring functions (default: True)

Returns: Complete PostgreSQL DDL as string

### generate_ta_ddl(schema, entity, view, refresh_strategy="scheduled", ...) -> str

Generate DDL for table-backed Arrow columnar view.

Parameters:
- `schema`: Schema dict from load_schema()
- `entity`: Entity name (e.g., "Order")
- `view`: View suffix without "ta_" (e.g., "order")
- `refresh_strategy`: "scheduled" or "manual" (default: "scheduled")
- `include_monitoring_functions`: Include monitoring functions (default: True)

Returns: Complete PostgreSQL DDL as string

### generate_composition_views(schema, entity, relationships) -> str

Generate composition views for relationships.

Parameters:
- `schema`: Schema dict from load_schema()
- `entity`: Entity name
- `relationships`: List of relationship field names

Returns: DDL for composition views

### suggest_refresh_strategy(write_volume, latency_requirement_ms, read_volume) -> str

Get recommended refresh strategy.

Parameters:
- `write_volume`: Expected writes per minute
- `latency_requirement_ms`: Maximum acceptable staleness in milliseconds
- `read_volume`: Expected reads per minute

Returns: "trigger-based" or "scheduled"

### validate_generated_ddl(sql: str) -> list[str]

Validate generated DDL.

Parameters:
- `sql`: Generated DDL string

Returns: List of validation warnings (empty if valid)

## Generated View Structure

### JSON Views (tv_*)

Stores entities as JSONB with:
- Complete entity representation
- Materialization tracking
- Staleness tracking for efficient refresh
- Multiple indexes for query optimization
- Monitoring and health check functions

Use for:
- Fast JSON queries
- Document-oriented access patterns
- GraphQL query resolution
- Efficient nested relationship loading

### Arrow Views (ta_*)

Stores entities as Arrow IPC RecordBatches with:
- Per-field columnar storage
- Batch-level metadata
- Arrow Flight protocol support
- Compression options
- Bulk analytics queries

Use for:
- Columnar analytics
- Arrow Flight streaming
- Bulk exports
- Read-only aggregations

## Refresh Strategies

### Trigger-based

**Pros:**
- Real-time data freshness
- Immediate consistency
- Low read latency

**Cons:**
- Higher overhead on writes
- Not ideal for bulk operations

**Best for:**
- Read-heavy workloads
- Strict latency requirements (< 100ms)
- Low write volume (< 10 writes/sec)

### Scheduled

**Pros:**
- Low write overhead
- Predictable refresh timing
- Efficient for bulk operations

**Cons:**
- Data lag (up to refresh interval)
- Lower freshness guarantees

**Best for:**
- Batch import systems
- Acceptable staleness (30+ minutes)
- High write volume (> 1000 writes/min)

## Monitoring Generated Views

All generated DDL includes monitoring functions:

```sql
-- Check view statistics
SELECT * FROM get_view_statistics_user();

-- Analyze staleness
SELECT * FROM analyze_staleness_user();

-- Check health status
SELECT * FROM health_check_user_summary();

-- Monitor performance
SELECT * FROM analyze_query_performance_user();

-- Track refresh metrics
SELECT * FROM track_refresh_performance_user();
```

## Output Location

Generated DDL files are automatically saved to the same directory as the example scripts:

```
examples/ddl-generation/
├── ddl_user_view.sql           # Generated JSON view
├── ddl_order_arrow_view.sql    # Generated Arrow view
└── README.md                   # Full documentation
```

## Integration with FraiseQL

The generated views integrate with FraiseQL's authoring layer:

```python
# In your FraiseQL schema definition
@fraiseql.query
def users(limit: int = 100) -> list[User]:
    return fraiseql.config(sql_source="tv_user")  # Use generated view

# Export schema
fraiseql.export_schema("schema.json")

# Generate DDL
python3 -c "
import sys
sys.path.insert(0, '/home/lionel/code/fraiseql/tools')
from fraiseql_tools import load_schema, generate_tv_ddl

schema = load_schema('schema.json')
ddl = generate_tv_ddl(schema, entity='User', view='user')

with open('ddl_user.sql', 'w') as f:
    f.write(ddl)
"
```

## Troubleshooting

### Import Error

```python
import sys
sys.path.insert(0, '/home/lionel/code/fraiseql/tools')
from fraiseql_tools import load_schema
```

### Schema Not Found

Ensure schema.json exists and use absolute path:
```python
from pathlib import Path
schema_path = Path(__file__).parent / "schema.json"
schema = load_schema(str(schema_path))
```

### Entity Not Found

Check entity name is correct (case-sensitive):
```python
# List available entities
print([t['name'] for t in schema['types']])
```

### Template Not Found

Ensure templates directory exists:
```bash
ls -la /home/lionel/code/fraiseql/tools/fraiseql_tools/templates/
```

## Performance Tips

1. **For very large schemas**: Generate views for one entity at a time
2. **For bulk refresh**: Use scheduled strategy with batch processing
3. **For real-time**: Use trigger-based strategy with low write volume
4. **For analytics**: Use Arrow views with batch materialization

## Next Steps

1. Copy your FraiseQL schema.json
2. Run: `python3 -c "from fraiseql_tools import load_schema, generate_tv_ddl; ..."`
3. Execute generated DDL against your PostgreSQL database
4. Use generated views in FraiseQL queries

For detailed documentation, see:
- `IMPLEMENTATION.md`: Technical architecture
- `examples/ddl-generation/README.md`: Complete usage guide
- Template files: `templates/*.sql`

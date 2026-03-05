# Fact Table Module

**Source file**: `crates/fraiseql-core/src/compiler/fact_table/` (split into `mod.rs`, `detector.rs`, `tests.rs`)

**Tests**: 30+ unit tests in `#[cfg(test)] mod tests` at the bottom of the file. Run with:
```bash
cargo nextest run -p fraiseql-core --lib compiler::fact_table
```

---

## Overview

The fact table pattern is FraiseQL's approach to analytics workloads. It is inspired by
dimensional modeling (Kimball-style data warehousing) but adapted for PostgreSQL JSONB.

The module provides:
1. **Auto-detection** — discovers `tf_*` tables by name convention and introspects their structure
2. **JSONB dimension extraction** — samples actual data to infer dimension key paths
3. **Query validation** — validates window function requests against discovered table metadata

---

## The Fact Table Pattern

### Naming: `tf_*` tables

Any table whose name starts with `tf_` (table fact) is treated as a fact table.
Detection: `name.starts_with("tf_") && name.len() > 3`.

Examples: `tf_sales`, `tf_events`, `tf_page_views_daily`

### Structure: Measures + JSONB Dimensions + Denormalized Filters

```sql
CREATE TABLE tf_sales (
    id           BIGSERIAL PRIMARY KEY,

    -- Measures: numeric SQL columns for fast GROUP BY aggregation
    revenue      DECIMAL(10,2) NOT NULL,
    quantity     INT NOT NULL,
    cost         DECIMAL(10,2) NOT NULL,

    -- Dimensions: JSONB for flexible, schemaless grouping
    dimensions   JSONB NOT NULL,
    -- e.g. {"category": "Electronics", "region": "APAC", "channel": "Online"}

    -- Denormalized filters: indexed SQL columns for fast WHERE
    customer_id  UUID NOT NULL,
    product_id   UUID NOT NULL,
    occurred_at  TIMESTAMPTZ NOT NULL
);

CREATE INDEX ON tf_sales(customer_id);
CREATE INDEX ON tf_sales(product_id);
CREATE INDEX ON tf_sales(occurred_at);
```

**Why this design?**

- **No joins at query time**: ETL denormalizes dimensions into JSONB at load time.
  Analytical queries aggregate over a single table, avoiding multi-way joins.
- **Measures in SQL columns**: `SUM(revenue)` uses native SQL aggregation — fast and
  index-friendly.
- **Dimensions in JSONB**: Flexible schema. Adding a new dimension (`"sub_region"`) requires
  no ALTER TABLE — just add the key in ETL.
- **Indexed filter columns**: `WHERE customer_id = $1 AND occurred_at >= $2` uses B-tree
  indexes, pushing filtering down before aggregation.

---

## Introspection Flow

```
DatabaseIntrospector::list_fact_tables()
      ↓ finds all tables starting with "tf_"
For each table:
  DatabaseIntrospector::get_columns(table_name)
      ↓ returns Vec<(name, data_type, is_nullable)>
  Classify each column:
      numeric AND NOT ends with "_id" AND NOT named "id"  → Measure
      JSONB or JSON                                        → DimensionColumn
      ends with "_id" AND indexed                         → Filter (UUID/INT FK)
      TIMESTAMPTZ/TIMESTAMP AND indexed                   → Filter (time)
  DatabaseIntrospector::get_sample_jsonb(table_name, "dimensions")
      ↓ SELECT dimensions FROM table LIMIT 100
  extract_dimension_paths(sample_jsonb, "dimensions", db_type)
      ↓ returns Vec<DimensionPath>
FactTableMetadata { measures, dimensions, denormalized_filters }
```

The resulting `FactTableMetadata` is used by `WindowPlanner` to validate that dimension
and measure names in window function requests actually exist on the table.

---

## JSONB Dimension Extraction

### Sampling Strategy

Instead of requiring explicit dimension declarations, FraiseQL samples the JSONB column
to discover dimension key paths:

```rust
pub fn extract_dimension_paths(
    sample: &serde_json::Value,
    column_name: &str,
    db_type: DatabaseType,
) -> Vec<DimensionPath>
```

The sampler walks the JSON structure recursively with a **max depth of 3** to avoid
infinite recursion on circular or deeply nested structures.

For each key found, it generates a database-specific extraction expression:

| Database | Generated expression |
|----------|---------------------|
| PostgreSQL | `dimensions->>'category'` |
| MySQL | `JSON_UNQUOTE(JSON_EXTRACT(dimensions, '$.category'))` |
| SQLite | `json_extract(dimensions, '$.category')` |
| SQL Server | `JSON_VALUE(dimensions, '$.category')` |

### Data type inference

The sampler infers types from the observed JSON value type:

| JSON value | Inferred type |
|-----------|---------------|
| `"Electronics"` | `string` |
| `42` (integer) | `integer` |
| `3.14` | `float` |
| `true`/`false` | `boolean` |
| `[...]` | `array` |
| `{...}` | `object` (nested) |

### Limitations

- **Single-sample extraction**: Path discovery uses one sample row. Heterogeneous JSONB
  (some rows have extra keys) means not all paths will be discovered from one sample.
  If a dimension is missing from the sample, it will not appear in the metadata.
- **Arrays are opaque**: Array-type dimensions are listed but not expanded. Array element
  paths must be declared explicitly if needed.
- **Max depth 3**: Deeply nested structures are truncated. For `{a: {b: {c: {d: ...}}}}`,
  paths up to `a.b.c` are discovered; `a.b.c.d` is not.

---

## Calendar Dimensions (Performance Optimization)

Fact tables can include pre-computed temporal bucket columns for fast time-series aggregation:

```sql
CREATE TABLE tf_sales (
    ...
    date_info     JSONB NOT NULL,   -- {"date":"2024-03-15","week":11,"month":3,"quarter":1,"year":2024}
    month_info    JSONB NOT NULL,   -- {"month":3,"quarter":1,"year":2024}
    quarter_info  JSONB NOT NULL,   -- {"quarter":1,"year":2024}
    year_info     JSONB NOT NULL,   -- {"year":2024}
);
```

The introspector detects `*_info` columns with JSONB type and maps them to temporal
bucket levels:

| Column | Inferred buckets |
|--------|-----------------|
| `date_info` | date, week, month, quarter, year |
| `week_info` | week, month, quarter, year |
| `month_info` | month, quarter, year |
| `quarter_info` | quarter, year |
| `year_info` | year |

**Why pre-compute?** `DATE_TRUNC('month', occurred_at)` at query time applies a function
to every row. Indexes on `date_info->>'month'` are text comparisons — cheap and indexable.
This can yield 10–20× faster temporal aggregations on large tables.

---

## Cross-Database Support

Fact tables require JSONB (PostgreSQL-native binary JSON). MySQL, SQL Server, and SQLite
use `JSON` or text columns without JSONB operators.

| Database | Status | Notes |
|----------|--------|-------|
| PostgreSQL | ✅ Full | Native JSONB operators, fastest path |
| MySQL | ❌ Not supported | JSON_EXTRACT works but JSONB operators absent; planned for v2.2 |
| SQL Server | ❌ Not supported | JSON_VALUE available but no JSONB aggregation |
| SQLite | ❌ Not supported | json_extract works but limited aggregation support |

A MySQL-compatible path would use `JSON_EXTRACT` with explicit dimension path declarations
(no sampling). This is in the planned roadmap. See `docs/adr/0009-database-feature-parity.md`.

---

## Explicit Table Declaration (Alternative to Auto-Detection)

Developers can declare fact tables explicitly instead of relying on auto-detection:

```python
@fraiseql.fact_table(
    name="tf_sales",
    measures=["revenue", "quantity", "cost"],
    dimensions=["category", "region", "channel"],
    primary_key="id",
)
class Sales:
    ...
```

Explicit declarations override introspected metadata and are preferred in production
deployments where sample-based discovery may be unreliable.

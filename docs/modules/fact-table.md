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

For each key found, it generates a PostgreSQL extraction expression, e.g.
`dimensions->>'category'`.

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

## Database Requirements

Fact tables require JSONB (PostgreSQL-native binary JSON): dimension extraction uses
the `->>` operator and aggregation relies on native JSONB operator support. PostgreSQL
is the only supported backend (see [database-compatibility.md](../database-compatibility.md)).

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

---

## Read gates: the type a fact table is read as

An aggregate or a window over a fact table reads its values: a group key *is* the value, a
sum or a max is computed from them, and a filter or an order answers a question about them.
So a fact table whose columns are gated names the type it is read as, and every read gate
of that type applies:

```json
"fact_tables": {
  "tf_sales": {
    "table_name": "tf_sales",
    "type_name": "Sale",
    "measures": [{"name": "revenue", "sql_type": "Decimal", "nullable": false},
                 {"name": "margin",  "sql_type": "Decimal", "nullable": true}],
    "dimensions": {"name": "data", "paths": [{"name": "segment", "json_path": "data->>'segment'", "data_type": "text"}]},
    "denormalized_filters": [{"name": "tenant_id", "sql_type": "Text", "indexed": true}]
  }
}
```

(`type` is accepted for `type_name` in the intermediate schema.)

The two SDKs that author fact tables emit it:

```go
fraiseql.NewFactTable("data").TableName("tf_sales").TypeName("Sale"). /* measures … */ Register()
```

```typescript
SchemaRegistry.registerFactTable("tf_sales", measures, dimensions, filters, { typeName: "Sale" });
```

- Every measure, denormalized filter, dimension path and native-mapping key of the fact
  table must be a field of the type (matched by snake_case name). A link to a type that does
  not exist, or that lacks one of them, is refused when the schema loads.
- Every name a request references — `where`, `groupBy`, the aggregates and `having`,
  `orderBy`, a window's `select`, `partitionBy`, `orderBy` and function field — must be a
  field of the type (an undeclared JSONB key is refused), and one the caller may read: a
  `requires_scope` field needs the scope whether it masks or rejects, and an `authorize`
  field is never referenceable. Otherwise the request is refused (403) before any SQL runs.
- The type's `requires_role` must be held.
- A fact table with no `type_name` declares no field gate, and none applies.

Independently of the link, when a row-level security policy is configured an aggregate or a
window without a principal is refused, as a regular query is: the policy cannot be
evaluated without one.

---

## Aggregation Result Caching

Aggregation queries on fact tables are cached by `CachedDatabaseAdapter` using a
version-aware key strategy. Source: `cache/fact_table_cache.rs` and
`cache/fact_table_version.rs`.

### Cache Key

```
agg:<SHA256(sql + schema_version + version_component)>
```

The `version_component` varies by strategy (see below).

### Version Strategies

Choose a strategy based on how the fact table is updated:

| Strategy | Best For | Cache Invalidation |
|----------|----------|--------------------|
| `Disabled` | Real-time accuracy required | No caching |
| `VersionTable` | ETL / batch loads with explicit version bumps | Read from `tf_versions` table; re-fetched at most every 1s |
| `TimeBased { ttl_seconds }` | Dashboards tolerant of short lag | Time-bucketed key expires after `ttl_seconds` |
| `SchemaVersion` | Immutable historical data | Cache cleared on schema deployment |

### Version Table Setup (VersionTable strategy)

```sql
CREATE TABLE tf_versions (
    table_name TEXT PRIMARY KEY,
    version    BIGINT NOT NULL DEFAULT 1,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE OR REPLACE FUNCTION bump_tf_version(p_table_name TEXT) RETURNS BIGINT AS $$
DECLARE new_version BIGINT;
BEGIN
    INSERT INTO tf_versions (table_name, version, updated_at)
    VALUES (p_table_name, 1, NOW())
    ON CONFLICT (table_name) DO UPDATE
    SET version = tf_versions.version + 1, updated_at = NOW()
    RETURNING tf_versions.version INTO new_version;
    RETURN new_version;
END;
$$ LANGUAGE plpgsql;
```

After each ETL load:

```sql
SELECT bump_tf_version('tf_sales');
```

### Configuration

```rust
use fraiseql_core::cache::fact_table_version::{
    FactTableCacheConfig, FactTableVersionStrategy,
};

let mut config = FactTableCacheConfig::default();

// ETL-loaded: explicit version bump required
config.set_strategy("tf_sales", FactTableVersionStrategy::VersionTable);

// Dashboards: 5-minute staleness acceptable
config.set_strategy(
    "tf_page_views",
    FactTableVersionStrategy::TimeBased { ttl_seconds: 300 },
);

// Historical rates: immutable until next deployment
config.set_strategy(
    "tf_historical_rates",
    FactTableVersionStrategy::SchemaVersion,
);
```

### Execution Flow

```
execute_aggregation_query(sql)
  ↓ Extract table name (regex: "FROM tf_xxx")
  ↓ If not a fact table → execute without cache
  ↓ Look up strategy for table (falls back to default)
  ↓ If Disabled → execute without cache
  ↓ Compute version component from strategy
  ↓ Generate SHA256 cache key
  ↓ Cache hit  → return cached result
    Cache miss → execute query, store result
```

The `FactTableVersionProvider` caches version lookups with a 1-second TTL to avoid
hammering the `tf_versions` table on every query.

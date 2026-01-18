# Phase 4b: Advanced SQL Features - Aggregation & ORDER BY Enhancement

**Status**: Planning
**Dependencies**: Phase 4 (Compiler), Phase 2 (Database)
**Estimated Duration**: 4-5 days
**Complexity**: Medium-High

---

## Overview

Add support for two critical SQL features that greatly expand FraiseQL's capabilities:

1. **GROUP BY / HAVING / Aggregates** (SUM, AVG, COUNT, MIN, MAX, etc.)
2. **ORDER BY with COLLATION** (locale-aware sorting)

These features are battle-tested in v1 and need to be integrated into v2's compiled query architecture.

---

## Architecture Integration

### Where These Features Fit

```
Phase 1-3: Foundation (schema, db, cache, security) ✅
Phase 4:   Compiler (parse, validate, lower, codegen) ✅
  │
  ├─ Phase 4a: Basic SQL generation ✅
  │
  └─ Phase 4b: Advanced SQL generation ⬅️ THIS PHASE
       ├─ GROUP BY / HAVING / Aggregates
       └─ ORDER BY with COLLATION
Phase 5:   Runtime (executor, matcher, planner) ✅
Phase 6:   HTTP Server ✅
Phase 7:   Utilities ✅
Phase 8:   Python Authoring ✅
```

**Integration Points**:

- **Phase 4 (Compiler)**: Schema compilation needs to understand aggregate fields
- **Phase 2 (Database)**: SQL generation needs GROUP BY/HAVING/COLLATE clauses
- **Phase 8 (Python)**: Decorators need aggregate configuration options

---

## Feature 1: GROUP BY / HAVING / Aggregates

### v1 Architecture (Reference)

**Fact Table Pattern** (from v1):

```sql
CREATE TABLE sales_facts (
    id UUID PRIMARY KEY,
    -- Measures (SQL columns) - aggregatable
    revenue NUMERIC NOT NULL,
    quantity INTEGER NOT NULL,
    cost NUMERIC NOT NULL,
    -- Dimensions (JSONB) - groupable
    data JSONB NOT NULL,  -- {category, region, product_name}
    -- Denormalized filters
    user_id UUID,
    created_at TIMESTAMP
);
```

**Design Rules**:

- ✅ Aggregate functions (SUM, AVG) operate on SQL columns
- ✅ GROUP BY operates on JSONB paths
- ✅ HAVING uses WhereClause pattern for type safety
- ❌ No aggregating JSONB fields (require explicit casting)
- ❌ No grouping by SQL columns (except temporal bucketing)

**v1 Implementation** (from `fraiseql_v1/fraiseql-python/src/fraiseql/sql/group_by_generator.py`):

```python
@dataclass(frozen=True)
class GroupByField:
    """Single GROUP BY dimension from JSONB data column."""
    field: str
    bucket: TemporalBucket | None = None  # day, week, month, quarter, year
    alias: str | None = None

@dataclass(frozen=True)
class AggregateField:
    """Single aggregate on SQL column."""
    function: str  # SUM, AVG, COUNT, MIN, MAX, STDDEV, etc.
    field: str | None = None  # None for COUNT(*)
    alias: str
    distinct: bool = False

@dataclass(frozen=True)
class GroupBySet:
    """Complete GROUP BY specification."""
    group_by: list[GroupByField]
    aggregates: list[AggregateField]
    having: WhereClause | None = None
```

**Generated SQL** (example):

```sql
SELECT
    data->'category' AS category,
    DATE_TRUNC('month', created_at) AS month,
    SUM(revenue) AS total_revenue,
    AVG(quantity) AS avg_quantity,
    COUNT(*) AS total_sales
FROM sales_facts
WHERE created_at >= '2024-01-01'
GROUP BY
    data->'category',
    DATE_TRUNC('month', created_at)
HAVING SUM(revenue) > 10000
ORDER BY total_revenue DESC
```

### v2 Integration Plan

#### 1. Extend CompiledSchema

**File**: `crates/fraiseql-core/src/schema/compiled.rs`

Add aggregate configuration to `QueryDefinition`:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueryDefinition {
    // ... existing fields ...

    /// Aggregation configuration (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aggregation: Option<AggregationConfig>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AggregationConfig {
    /// Fields to group by (JSONB paths)
    pub group_by: Vec<GroupByField>,

    /// Aggregate expressions (SQL columns)
    pub aggregates: Vec<AggregateField>,

    /// HAVING clause (filter on aggregates)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub having: Option<WhereClause>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupByField {
    /// JSONB field path (e.g., "category", "product.name")
    pub field: String,

    /// Temporal bucket (for timestamp grouping)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bucket: Option<TemporalBucket>,

    /// Result alias
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TemporalBucket {
    Day,
    Week,
    Month,
    Quarter,
    Year,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AggregateField {
    /// Aggregate function (SUM, AVG, COUNT, etc.)
    pub function: String,

    /// SQL column to aggregate (None for COUNT(*))
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,

    /// Result alias
    pub alias: String,

    /// Use DISTINCT (for COUNT DISTINCT, etc.)
    #[serde(default)]
    pub distinct: bool,
}
```

#### 2. Update SQL Generation

**File**: `crates/fraiseql-core/src/db/postgres/adapter.rs`

Add new method for aggregate queries:

```rust
async fn execute_aggregate_query(
    &self,
    view: &str,
    aggregation: &AggregationConfig,
    where_clause: Option<&WhereClause>,
    order_by: Option<&OrderBy>,
    limit: Option<u32>,
) -> Result<Vec<JsonbValue>> {
    // Generate SELECT with aggregates
    // Generate GROUP BY clause
    // Generate HAVING clause
    // Execute and return results
}
```

**SQL Generation Logic**:

```rust
// SELECT clause
let select_parts: Vec<String> = vec![
    // Group by fields
    aggregation.group_by.iter().map(|g| {
        if let Some(bucket) = &g.bucket {
            format!("DATE_TRUNC('{}', {}) AS {}",
                bucket, g.field, g.alias.unwrap_or(&g.field))
        } else {
            format!("data->>'{}' AS {}",
                g.field, g.alias.unwrap_or(&g.field))
        }
    }),
    // Aggregate fields
    aggregation.aggregates.iter().map(|a| {
        let distinct = if a.distinct { "DISTINCT " } else { "" };
        if let Some(field) = &a.field {
            format!("{}({}{})", a.function, distinct, field)
        } else {
            format!("{}(*)", a.function)
        }
    })
].concat().join(", ");

// GROUP BY clause
let group_by_parts: Vec<String> = aggregation.group_by.iter()
    .enumerate()
    .map(|(i, _)| (i + 1).to_string())  // Use column positions
    .collect();

// HAVING clause
let having_sql = if let Some(having) = &aggregation.having {
    format!("HAVING {}", generate_where_sql(having, "data"))
} else {
    String::new()
};
```

#### 3. Update Python Decorators

**File**: `fraiseql-python/src/fraiseql/decorators.py`

Add aggregation parameter to `@query` decorator:

```python
@fraiseql.query(
    sql_source="v_sales",
    aggregation={
        "group_by": [
            {"field": "category"},
            {"field": "created_at", "bucket": "month", "alias": "month"}
        ],
        "aggregates": [
            {"function": "SUM", "field": "revenue", "alias": "total_revenue"},
            {"function": "AVG", "field": "quantity", "alias": "avg_quantity"},
            {"function": "COUNT", "alias": "total_sales"}
        ],
        "having": {
            "total_revenue__gt": 10000
        }
    }
)
def sales_by_category_month() -> list[SalesSummary]:
    """Get sales aggregated by category and month."""
    pass
```

---

## Feature 2: ORDER BY with COLLATION

### Background

**Problem**: Default PostgreSQL sorting doesn't handle locale-aware sorting correctly.

```sql
-- Default sorting (wrong for most languages)
ORDER BY name
-- Result: Apple, Banana, Émile, Zebra (Émile out of order)

-- Locale-aware sorting (correct)
ORDER BY name COLLATE "en-US-x-icu"
-- Result: Apple, Banana, Émile, Zebra (correct alphabetical)
```

**Collation Types**:

- `en-US-x-icu`: English (United States) - ICU collation
- `fr-FR-x-icu`: French (France)
- `de-DE-x-icu`: German (Germany)
- `C`: Fast byte-order sorting (for non-text or known-ASCII)
- Database default: Usually `en_US.UTF-8`

### v2 Integration Plan

#### 1. Extend OrderBy Type

**File**: `crates/fraiseql-core/src/db/types.rs`

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderByField {
    /// Field name (JSONB path)
    pub field: String,

    /// Sort direction
    #[serde(default)]
    pub direction: SortDirection,

    /// Collation (for locale-aware text sorting)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collation: Option<String>,

    /// Nulls ordering
    #[serde(default)]
    pub nulls: NullsOrdering,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum SortDirection {
    Asc,
    Desc,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum NullsOrdering {
    First,
    Last,
    Default,  // Database default behavior
}
```

#### 2. Update SQL Generation

**File**: `crates/fraiseql-core/src/db/postgres/adapter.rs`

```rust
fn generate_order_by_sql(order_by: &[OrderByField], jsonb_column: &str) -> String {
    order_by.iter().map(|field| {
        let direction = match field.direction {
            SortDirection::Asc => "ASC",
            SortDirection::Desc => "DESC",
        };

        let nulls = match field.nulls {
            NullsOrdering::First => " NULLS FIRST",
            NullsOrdering::Last => " NULLS LAST",
            NullsOrdering::Default => "",
        };

        let collation = if let Some(collate) = &field.collation {
            format!(" COLLATE \"{}\"", collate)
        } else {
            String::new()
        };

        format!("{}->>'{}'{}{}{}",
            jsonb_column,
            field.field,
            collation,
            direction,
            nulls
        )
    }).collect::<Vec<_>>().join(", ")
}
```

**Generated SQL**:

```sql
-- Without collation
ORDER BY data->>'name' ASC

-- With collation
ORDER BY data->>'name' COLLATE "en-US-x-icu" ASC NULLS LAST

-- Multiple fields with mixed collation
ORDER BY
    data->>'country' COLLATE "en-US-x-icu" ASC,
    data->>'city' COLLATE "en-US-x-icu" ASC,
    data->>'created_at' DESC NULLS LAST
```

#### 3. Update Python Decorators

**File**: `fraiseql-python/src/fraiseql/decorators.py`

Add order_by configuration to auto_params:

```python
@fraiseql.query(
    sql_source="v_user",
    auto_params={
        "limit": True,
        "offset": True,
        "where": True,
        "order_by": {
            "enabled": True,
            "default": [
                {"field": "name", "direction": "ASC", "collation": "en-US-x-icu"},
                {"field": "created_at", "direction": "DESC", "nulls": "LAST"}
            ]
        }
    }
)
def users() -> list[User]:
    """Get users with locale-aware sorting."""
    pass
```

---

## Implementation Plan

### Step 1: Schema Extensions (Day 1)

**Goal**: Add aggregation and collation types to schema

**Tasks**:

1. Update `crates/fraiseql-core/src/schema/compiled.rs`:
   - Add `AggregationConfig`, `GroupByField`, `AggregateField`, `TemporalBucket`
   - Add `collation` field to existing `OrderBy` type (if exists)

2. Update `crates/fraiseql-core/src/db/types.rs`:
   - Add/update `OrderByField` with `collation` and `nulls` fields
   - Add `SortDirection` and `NullsOrdering` enums

3. Write unit tests:
   - Schema serialization/deserialization
   - Type validation

**Verification**:

```bash
cargo test --lib schema::compiled
cargo test --lib db::types
```

### Step 2: SQL Generation (Day 2-3)

**Goal**: Generate SQL for GROUP BY/HAVING/COLLATE

**Tasks**:

1. Create `crates/fraiseql-core/src/db/postgres/aggregate.rs`:
   - `generate_aggregate_select()` - SELECT with aggregates
   - `generate_group_by()` - GROUP BY clause
   - `generate_having()` - HAVING clause with WhereClause

2. Update `crates/fraiseql-core/src/db/postgres/adapter.rs`:
   - Add `execute_aggregate_query()` method
   - Update `generate_order_by_sql()` with collation support

3. Write integration tests:
   - Test aggregate queries against test database
   - Test collation sorting with unicode data
   - Test HAVING clause filtering

**Verification**:

```bash
# Start test database
make db-up

# Run integration tests
cargo test --lib db::postgres::aggregate -- --ignored
cargo test --lib db::postgres::order_by -- --ignored
```

### Step 3: Python Decorator Support (Day 4)

**Goal**: Add aggregation and collation to Python API

**Tasks**:

1. Update `fraiseql-python/src/fraiseql/decorators.py`:
   - Add `aggregation` parameter to `@query` decorator
   - Add `order_by` configuration to `auto_params`

2. Update `fraiseql-python/src/fraiseql/types.py`:
   - Add validation for aggregation specs
   - Add validation for collation strings

3. Write Python tests:
   - Test aggregation decorator parameter
   - Test order_by with collation
   - Test schema JSON output

**Verification**:

```bash
cd fraiseql-python
PYTHONPATH=src python -m pytest tests/ -v
```

### Step 4: Documentation & Examples (Day 5)

**Goal**: Document new features with examples

**Tasks**:

1. Update `fraiseql-python/README.md`:
   - Add aggregation example
   - Add collation example

2. Create `fraiseql-python/examples/aggregation_query.py`:
   - Sales by category example
   - Temporal bucketing example
   - HAVING clause example

3. Create `fraiseql-python/examples/collation_query.py`:
   - Locale-aware sorting example
   - Multi-field ordering example

4. Update `.claude/IMPLEMENTATION_ROADMAP.md`:
   - Mark Phase 4b as complete
   - Update feature matrix

**Verification**:

```bash
cd fraiseql-python
python examples/aggregation_query.py
python examples/collation_query.py
```

---

## Testing Strategy

### Unit Tests

**Schema** (Rust):

- Aggregation config serialization
- Collation field validation
- Temporal bucket enum

**SQL Generation** (Rust):

- GROUP BY clause generation
- HAVING clause generation
- ORDER BY with COLLATE
- Aggregate SELECT generation

**Decorators** (Python):

- Aggregation parameter validation
- Collation string validation
- Schema JSON output

### Integration Tests

**Database** (Rust with test DB):

- Aggregate query execution
- HAVING clause filtering
- Collation sorting with unicode
- Temporal bucketing

**End-to-End** (Python + Rust):

- Python decorator → JSON → Rust compilation
- Full query execution with aggregates
- Locale-aware sorting verification

### Test Data

**For Aggregation**:

```sql
CREATE TABLE test_sales (
    id UUID PRIMARY KEY,
    revenue NUMERIC NOT NULL,
    quantity INTEGER NOT NULL,
    data JSONB NOT NULL,  -- {category, region, product}
    created_at TIMESTAMP NOT NULL
);

INSERT INTO test_sales VALUES
    ('uuid1', 1000, 10, '{"category":"A","region":"US"}', '2024-01-15'),
    ('uuid2', 2000, 20, '{"category":"A","region":"EU"}', '2024-01-20'),
    ('uuid3', 1500, 15, '{"category":"B","region":"US"}', '2024-02-10');
```

**For Collation**:

```sql
CREATE TABLE test_users (
    id UUID PRIMARY KEY,
    data JSONB NOT NULL  -- {name: "Émile", "Zoë", "André", "Björk"}
);
```

---

## Success Criteria

### Functional

- [x] Schema supports aggregation configuration
- [x] Schema supports collation in ORDER BY
- [x] SQL generation for GROUP BY/HAVING works
- [x] SQL generation for COLLATE works
- [x] Python decorators support aggregation
- [x] Python decorators support collation

### Quality

- [x] 50+ unit tests (Rust + Python)
- [x] 10+ integration tests with test DB
- [x] Examples demonstrate all features
- [x] Documentation complete

### Performance

- [x] Aggregate queries execute correctly
- [x] Collation sorting performs well
- [x] No regression in existing query performance

---

## Migration from v1

**v1 API** (Python runtime):

```python
# GROUP BY in v1 (runtime)
result = await db.aggregate(
    "sales_facts",
    group_by=["category", "month"],
    aggregates={"total": "SUM(revenue)"},
    having={"total__gt": 10000}
)
```

**v2 API** (compile-time):

```python
# GROUP BY in v2 (compile-time)
@fraiseql.query(
    sql_source="v_sales",
    aggregation={
        "group_by": [{"field": "category"}, {"field": "month"}],
        "aggregates": [{"function": "SUM", "field": "revenue", "alias": "total"}],
        "having": {"total__gt": 10000}
    }
)
def sales_summary() -> list[SalesSummary]:
    pass
```

**Benefits of v2**:

- Compile-time validation
- Type-safe aggregates
- Pre-compiled SQL (faster)
- No runtime Python overhead

---

## Risks & Mitigations

### Risk 1: Schema Breaking Changes

**Impact**: High - Existing schemas may break
**Probability**: Medium
**Mitigation**: Make aggregation optional, maintain backward compatibility

### Risk 2: SQL Generation Complexity

**Impact**: Medium - Complex SQL may have bugs
**Probability**: Medium
**Mitigation**: Comprehensive test suite, leverage v1 implementation

### Risk 3: Database Compatibility

**Impact**: Medium - Collation may vary across databases
**Probability**: Low (PostgreSQL primary target)
**Mitigation**: Document PostgreSQL-specific features, test with multiple PG versions

---

## Next Steps After Completion

1. **Phase 9**: CLI Tool (compile schemas with aggregation)
2. **Phase 10**: Benchmarks (measure aggregate query performance)
3. **Advanced Features** (future):
   - Window functions (ROW_NUMBER, RANK, etc.)
   - Common Table Expressions (CTEs)
   - Lateral joins for complex aggregations

---

**Status**: Ready for implementation
**Assignee**: TBD
**Priority**: High (expands core query capabilities significantly)

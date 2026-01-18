# Analytics Phase 5: Integration & Testing

**Status**: Ready to implement
**Estimated Effort**: 2 days
**Dependencies**: Phases 1-4 complete ✅

---

## Objective

Wire the analytics aggregation system into the GraphQL runtime executor, enabling end-to-end execution of aggregate queries from GraphQL to SQL results.

---

## Context

**What's Done (Phases 1-4)**:

- ✅ Fact table introspection from database schema
- ✅ Auto-generated GraphQL aggregate types
- ✅ Validated execution plans for GROUP BY queries
- ✅ Database-specific SQL generation (PostgreSQL, MySQL, SQLite, SQL Server)

**What's Missing**:

- ❌ Integration with GraphQL executor
- ❌ GraphQL AST parsing for aggregate queries
- ❌ End-to-end pipeline from query → SQL → JSON results
- ❌ Real database tests

**This Phase**: Connect all the pieces together.

---

## Architecture

### Current Flow (Regular Queries)

```
GraphQL Query
    ↓
QueryMatcher.match_query()  ← Matches query to schema
    ↓
QueryPlanner.plan()         ← Creates execution plan
    ↓
DatabaseAdapter.execute_where_query()  ← Executes SQL
    ↓
ResultProjector.project()   ← Projects to GraphQL JSON
    ↓
GraphQL Response
```

### New Flow (Aggregate Queries)

```
GraphQL Aggregate Query
    ↓
[NEW] AggregateQueryParser.parse()  ← Parse aggregate query AST
    ↓
[NEW] Build AggregationRequest      ← Extract groupBy, aggregates, having
    ↓
AggregationPlanner.plan()           ← Validate & optimize (EXISTS)
    ↓
AggregationSqlGenerator.generate()  ← Database-specific SQL (EXISTS)
    ↓
[NEW] DatabaseAdapter.execute_raw_query()  ← Execute aggregation SQL
    ↓
[NEW] AggregationProjector.project()       ← Project SQL to GraphQL JSON
    ↓
GraphQL Response
```

---

## Files to Modify/Create

### 1. Runtime Executor (`runtime/executor.rs`)

**Add method**:

```rust
pub async fn execute_aggregate_query(
    &self,
    query: &str,
    variables: Option<&serde_json::Value>,
) -> Result<String>
```

**Integration point**: Call from `execute()` when query is detected as aggregate query.

### 2. GraphQL Parser (`runtime/aggregate_parser.rs`) - NEW FILE

**Purpose**: Parse GraphQL aggregate queries into `AggregationRequest`

**Key functions**:

```rust
pub struct AggregateQueryParser;

impl AggregateQueryParser {
    /// Parse GraphQL query AST into AggregationRequest
    pub fn parse(
        query_ast: &graphql_parser::query::Document,
        fact_table: &FactTableMetadata,
    ) -> Result<AggregationRequest>;

    /// Extract groupBy arguments
    fn parse_group_by(args: &[Argument]) -> Result<Vec<GroupBySelection>>;

    /// Extract aggregate selections from field selection set
    fn parse_aggregates(selection_set: &[Selection]) -> Result<Vec<AggregateSelection>>;

    /// Extract having conditions
    fn parse_having(args: &[Argument]) -> Result<Vec<HavingCondition>>;
}
```

### 3. Result Projector (`runtime/aggregate_projector.rs`) - NEW FILE

**Purpose**: Project SQL aggregate results to GraphQL JSON

**Key functions**:

```rust
pub struct AggregationProjector;

impl AggregationProjector {
    /// Project SQL rows to GraphQL response
    pub fn project(
        rows: Vec<HashMap<String, serde_json::Value>>,
        plan: &AggregationPlan,
    ) -> Result<serde_json::Value>;
}
```

### 4. Database Adapter Trait (`db/traits.rs`)

**Add method**:

```rust
async fn execute_raw_query(
    &self,
    sql: &str,
) -> Result<Vec<HashMap<String, serde_json::Value>>>;
```

### 5. Integration Tests (`tests/integration/aggregation_test.rs`) - NEW FILE

**Test scenarios**:

- Simple COUNT query
- Multiple measures (SUM, AVG, MIN, MAX)
- GROUP BY dimension (category)
- GROUP BY temporal (day, week, month)
- HAVING filters
- ORDER BY + LIMIT

---

## Implementation Steps

### Step 1: Add Database Adapter Method (30 min)

**File**: `crates/fraiseql-core/src/db/traits.rs`

Add method to `DatabaseAdapter` trait:

```rust
/// Execute raw SQL query and return rows as JSON objects
async fn execute_raw_query(
    &self,
    sql: &str,
) -> Result<Vec<HashMap<String, serde_json::Value>>>;
```

Implement for:

- `PostgresAdapter`
- `MySQLAdapter` (if exists)
- `SQLiteAdapter` (if exists)
- Mock adapter for tests

---

### Step 2: Create Aggregate Query Parser (3-4 hours)

**File**: `crates/fraiseql-core/src/runtime/aggregate_parser.rs`

**GraphQL Query Example**:

```graphql
query {
  sales_aggregate(
    where: { customer_id: { _eq: "uuid-123" } }
    groupBy: { category: true, occurred_at_day: true }
    having: { revenue_sum_gt: 1000 }
    orderBy: { revenue_sum: DESC }
    limit: 10
  ) {
    category
    occurred_at_day
    count
    revenue_sum
    revenue_avg
  }
}
```

**Parse to**:

```rust
AggregationRequest {
    table_name: "tf_sales",
    where_clause: Some(WhereClause { ... }),
    group_by: vec![
        GroupBySelection::Dimension {
            path: "category",
            alias: "category",
        },
        GroupBySelection::TemporalBucket {
            column: "occurred_at",
            bucket: TemporalBucket::Day,
            alias: "occurred_at_day",
        },
    ],
    aggregates: vec![
        AggregateSelection::Count { alias: "count" },
        AggregateSelection::MeasureAggregate {
            measure: "revenue",
            function: AggregateFunction::Sum,
            alias: "revenue_sum",
        },
        AggregateSelection::MeasureAggregate {
            measure: "revenue",
            function: AggregateFunction::Avg,
            alias: "revenue_avg",
        },
    ],
    having: vec![
        HavingCondition {
            aggregate: AggregateSelection::MeasureAggregate { ... },
            operator: HavingOperator::Gt,
            value: json!(1000),
        },
    ],
    order_by: vec![
        OrderByClause {
            field: "revenue_sum",
            direction: OrderDirection::Desc,
        },
    ],
    limit: Some(10),
    offset: None,
}
```

**Parsing Logic**:

1. Use `graphql_parser` crate to parse query AST
2. Find `sales_aggregate` field
3. Extract `where`, `groupBy`, `having`, `orderBy`, `limit` arguments
4. Extract requested fields from selection set (these map to aggregates)

---

### Step 3: Create Aggregation Projector (1 hour)

**File**: `crates/fraiseql-core/src/runtime/aggregate_projector.rs`

**SQL Result Example** (PostgreSQL row):

```json
{
  "category": "Electronics",
  "occurred_at_day": "2025-01-01T00:00:00Z",
  "count": 42,
  "revenue_sum": 5280.50,
  "revenue_avg": 125.73
}
```

**Project to GraphQL**:

```json
{
  "data": {
    "sales_aggregate": [
      {
        "category": "Electronics",
        "occurred_at_day": "2025-01-01T00:00:00Z",
        "count": 42,
        "revenue_sum": 5280.50,
        "revenue_avg": 125.73
      }
    ]
  }
}
```

**Key Logic**:

- SQL returns `Vec<HashMap<String, Value>>`
- Wrap in `{ "data": { "query_name": [...] } }`
- Map SQL column types to GraphQL types (Int, Float, String, DateTime)

---

### Step 4: Wire into Executor (2 hours)

**File**: `crates/fraiseql-core/src/runtime/executor.rs`

Add new method:

```rust
pub async fn execute_aggregate_query(
    &self,
    query: &str,
    variables: Option<&serde_json::Value>,
) -> Result<String> {
    // 1. Parse GraphQL query
    let query_ast = graphql_parser::parse_query(query)?;

    // 2. Identify aggregate query (name ends with "_aggregate")
    let (query_name, fact_table) = self.identify_aggregate_query(&query_ast)?;

    // 3. Get fact table metadata
    let metadata = self.schema.fact_tables.get(&fact_table)
        .ok_or_else(|| FraiseQLError::NotFound { ... })?;

    // 4. Parse into AggregationRequest
    let request = AggregateQueryParser::parse(&query_ast, metadata)?;

    // 5. Generate execution plan
    let plan = AggregationPlanner::plan(request, metadata.clone())?;

    // 6. Generate SQL
    let sql_generator = AggregationSqlGenerator::new(self.adapter.database_type());
    let sql = sql_generator.generate(&plan)?;

    // 7. Execute SQL
    let rows = self.adapter.execute_raw_query(&sql.complete_sql).await?;

    // 8. Project results
    let result = AggregationProjector::project(rows, &plan)?;

    // 9. Wrap in GraphQL envelope
    let response = json!({
        "data": {
            query_name: result
        }
    });

    Ok(serde_json::to_string(&response)?)
}
```

**Integration with existing `execute()` method**:

```rust
pub async fn execute(
    &self,
    query: &str,
    variables: Option<&serde_json::Value>,
) -> Result<String> {
    // Check if this is an aggregate query
    if self.is_aggregate_query(query) {
        return self.execute_aggregate_query(query, variables).await;
    }

    // Existing logic for regular queries
    // ...
}

fn is_aggregate_query(&self, query: &str) -> bool {
    // Simple heuristic: check if query contains "_aggregate("
    query.contains("_aggregate(") || query.contains("_aggregate {")
}
```

---

### Step 5: Write Integration Tests (3-4 hours)

**File**: `tests/integration/aggregation_test.rs`

**Test Structure**:

```rust
#[cfg(test)]
mod tests {
    use fraiseql_core::runtime::Executor;
    use fraiseql_core::db::postgres::PostgresAdapter;

    async fn setup_test_database() -> Arc<PostgresAdapter> {
        // Create test database
        // Load tf_sales schema
        // Insert test data
    }

    #[tokio::test]
    async fn test_simple_count() {
        let adapter = setup_test_database().await;
        let schema = load_test_schema(); // includes tf_sales aggregate types
        let executor = Executor::new(schema, adapter);

        let query = r#"
        query {
          sales_aggregate {
            count
          }
        }
        "#;

        let result = executor.execute(query, None).await.unwrap();
        let json: Value = serde_json::from_str(&result).unwrap();

        assert!(json["data"]["sales_aggregate"].is_array());
        assert_eq!(json["data"]["sales_aggregate"][0]["count"], 100);
    }

    #[tokio::test]
    async fn test_group_by_category() {
        // GROUP BY data->>'category'
    }

    #[tokio::test]
    async fn test_temporal_bucket_day() {
        // GROUP BY DATE_TRUNC('day', occurred_at)
    }

    #[tokio::test]
    async fn test_multiple_aggregates() {
        // COUNT, SUM, AVG, MIN, MAX
    }

    #[tokio::test]
    async fn test_having_filter() {
        // HAVING SUM(revenue) > 1000
    }

    #[tokio::test]
    async fn test_order_by_limit() {
        // ORDER BY revenue_sum DESC LIMIT 10
    }

    #[tokio::test]
    async fn test_all_databases() {
        // Test PostgreSQL, MySQL, SQLite, SQL Server
    }
}
```

**Test Data**:

```sql
-- tf_sales test data
INSERT INTO tf_sales (revenue, data, occurred_at) VALUES
  (100.00, '{"category": "Electronics", "product": "Phone"}', '2025-01-01'),
  (150.00, '{"category": "Electronics", "product": "Laptop"}', '2025-01-01'),
  (50.00, '{"category": "Books", "product": "Novel"}', '2025-01-02'),
  (200.00, '{"category": "Electronics", "product": "Tablet"}', '2025-01-02');
```

**Expected Results**:

```json
{
  "data": {
    "sales_aggregate": [
      {"category": "Electronics", "count": 3, "revenue_sum": 450.00},
      {"category": "Books", "count": 1, "revenue_sum": 50.00}
    ]
  }
}
```

---

## Verification Checklist

Before marking this phase complete, verify:

- [ ] `cargo check` passes with no errors
- [ ] `cargo clippy` passes with no warnings
- [ ] All unit tests pass: `cargo test --lib`
- [ ] Integration tests pass: `cargo test --test aggregation_test`
- [ ] Manual test: Run example aggregate query against test database
- [ ] All 4 databases tested (PostgreSQL, MySQL, SQLite, SQL Server)
- [ ] GROUP BY dimensions work correctly
- [ ] GROUP BY temporal buckets work correctly
- [ ] HAVING filters work correctly
- [ ] ORDER BY + LIMIT work correctly
- [ ] SQL output is valid and optimized
- [ ] Results match expected aggregations

---

## Acceptance Criteria

### Functional Requirements

✅ Execute aggregate queries end-to-end
✅ Parse GraphQL aggregate queries
✅ Generate correct SQL for all databases
✅ Project SQL results to GraphQL JSON
✅ Support all aggregate functions (COUNT, SUM, AVG, MIN, MAX, STDDEV, VARIANCE)
✅ Support GROUP BY dimensions (JSONB paths)
✅ Support GROUP BY temporal buckets (day, week, month, quarter, year)
✅ Support HAVING filters
✅ Support ORDER BY + LIMIT

### Non-Functional Requirements

✅ Tests pass for all 4 databases
✅ No clippy warnings
✅ Code is well-documented
✅ Performance is acceptable (< 100ms for simple queries)

---

## Next Steps After Phase 5

**Phase 6 (Optional)**: Advanced Aggregates (2 days)

- ARRAY_AGG, JSON_AGG, STRING_AGG
- BOOL_AND, BOOL_OR

**Phase 7 (Optional)**: Window Functions (3-4 days)

- ROW_NUMBER, RANK, DENSE_RANK
- LAG, LEAD, FIRST_VALUE, LAST_VALUE
- Running totals, moving averages

**Phase 8**: Integration Tests & Refinement (2 days)

- Comprehensive test suite
- Performance testing
- Bug fixes

**Phase 9**: Documentation (1-2 days)

- Architecture docs
- API documentation
- Usage guides

---

## DO NOT

❌ Don't modify Phases 1-4 code (it's working and tested)
❌ Don't change the `AggregationPlan` structure
❌ Don't change the SQL generation logic
❌ Don't add features beyond Phase 5 scope
❌ Don't optimize prematurely (save for Phase 8)

## DO

✅ Reuse existing modules (AggregationPlanner, AggregationSqlGenerator)
✅ Follow existing code patterns in `runtime/executor.rs`
✅ Write tests for each new function
✅ Document all new public APIs
✅ Test on all 4 databases

---

**Ready to implement!** 🚀

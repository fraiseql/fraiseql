# Analytics Phase 6: Advanced Aggregates

**Status**: Ready to implement
**Estimated Effort**: 2 days
**Dependencies**: Phase 5 complete ✅

---

## Objective

Add advanced aggregation functions for complex analytics: ARRAY_AGG, JSON_AGG, STRING_AGG, BOOL_AND, BOOL_OR.

---

## Context

**What's Done (Phase 5)**:
- ✅ Basic aggregates (COUNT, SUM, AVG, MIN, MAX, STDDEV, VARIANCE)
- ✅ GROUP BY with dimensions and temporal buckets
- ✅ HAVING, ORDER BY, LIMIT/OFFSET
- ✅ Full pipeline: Parse → Plan → SQL → Execute → Project

**What's Missing**:
- ❌ Array aggregation (collect values into arrays)
- ❌ JSON aggregation (build JSON objects/arrays)
- ❌ String aggregation (concatenate with delimiter)
- ❌ Boolean aggregation (AND/OR logic)

**This Phase**: Add these advanced functions for richer analytics capabilities.

---

## Advanced Aggregate Functions

### 1. ARRAY_AGG - Collect values into arrays

**Use Case**: Collect related items per group

**Example**:
```sql
SELECT
  category,
  ARRAY_AGG(product_id ORDER BY revenue DESC) AS top_products
FROM tf_sales
GROUP BY category
```

**Result**:
```json
[
  {"category": "Electronics", "top_products": ["prod_1", "prod_5", "prod_3"]},
  {"category": "Books", "top_products": ["prod_2", "prod_4"]}
]
```

**Database Support**:
- **PostgreSQL**: `ARRAY_AGG(column ORDER BY x)` ✅
- **MySQL**: `JSON_ARRAYAGG(column)` (returns JSON array, not native array)
- **SQLite**: Not supported natively (emulate with `GROUP_CONCAT`)
- **SQL Server**: `STRING_AGG` only (convert to JSON manually)

### 2. JSON_AGG / JSONB_AGG - Aggregate into JSON

**Use Case**: Build nested JSON responses

**Example**:
```sql
SELECT
  category,
  JSON_AGG(
    JSON_BUILD_OBJECT(
      'product', product_id,
      'revenue', revenue
    ) ORDER BY revenue DESC
  ) AS items
FROM tf_sales
GROUP BY category
```

**Result**:
```json
[
  {
    "category": "Electronics",
    "items": [
      {"product": "prod_1", "revenue": 1500},
      {"product": "prod_5", "revenue": 1200}
    ]
  }
]
```

**Database Support**:
- **PostgreSQL**: `JSON_AGG(expr)`, `JSONB_AGG(expr)` ✅
- **MySQL**: `JSON_ARRAYAGG(column)`, `JSON_OBJECTAGG(key, value)` ✅
- **SQLite**: Limited (manual JSON construction)
- **SQL Server**: `FOR JSON PATH` clause

### 3. STRING_AGG - Concatenate strings

**Use Case**: Display comma-separated lists

**Example**:
```sql
SELECT
  category,
  STRING_AGG(product_name, ', ' ORDER BY revenue DESC) AS products
FROM tf_sales
GROUP BY category
```

**Result**:
```json
[
  {"category": "Electronics", "products": "Laptop, Phone, Tablet"},
  {"category": "Books", "products": "Novel, Textbook"}
]
```

**Database Support**:
- **PostgreSQL**: `STRING_AGG(column, delimiter ORDER BY x)` ✅
- **MySQL**: `GROUP_CONCAT(column ORDER BY x SEPARATOR delimiter)` ✅
- **SQLite**: `GROUP_CONCAT(column, delimiter)` (no ORDER BY in older versions)
- **SQL Server**: `STRING_AGG(column, delimiter) WITHIN GROUP (ORDER BY x)` ✅

### 4. BOOL_AND / BOOL_OR - Boolean aggregates

**Use Case**: Check if all/any conditions are true

**Example**:
```sql
SELECT
  category,
  BOOL_AND(is_active) AS all_active,
  BOOL_OR(has_discount) AS any_discounted
FROM tf_sales
GROUP BY category
```

**Result**:
```json
[
  {"category": "Electronics", "all_active": true, "any_discounted": false},
  {"category": "Books", "all_active": false, "any_discounted": true}
]
```

**Database Support**:
- **PostgreSQL**: `BOOL_AND(condition)`, `BOOL_OR(condition)` ✅
- **MySQL**: `MIN(condition)`, `MAX(condition)` (emulate with 0/1)
- **SQLite**: `MIN(condition)`, `MAX(condition)` (emulate with 0/1)
- **SQL Server**: `MIN(CAST(condition AS BIT))`, `MAX(CAST(condition AS BIT))`

---

## Implementation Plan

### Step 1: Extend AggregateFunction Enum (30 min)

**File**: `crates/fraiseql-core/src/compiler/aggregate_types.rs`

Add new variants:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AggregateFunction {
    // Existing
    Count,
    Sum,
    Avg,
    Min,
    Max,
    Stddev,
    Variance,

    // NEW: Advanced aggregates
    ArrayAgg,       // Collect into array
    JsonAgg,        // Aggregate into JSON array
    JsonbAgg,       // Aggregate into JSONB array (PostgreSQL)
    StringAgg,      // Concatenate strings
    BoolAnd,        // Boolean AND
    BoolOr,         // Boolean OR
}
```

Add SQL name mapping:
```rust
impl AggregateFunction {
    pub fn sql_name(&self) -> &'static str {
        match self {
            // ... existing
            Self::ArrayAgg => "ARRAY_AGG",
            Self::JsonAgg => "JSON_AGG",
            Self::JsonbAgg => "JSONB_AGG",
            Self::StringAgg => "STRING_AGG",
            Self::BoolAnd => "BOOL_AND",
            Self::BoolOr => "BOOL_OR",
        }
    }
}
```

### Step 2: Add Advanced Aggregate Selections (1 hour)

**File**: `crates/fraiseql-core/src/compiler/aggregation.rs`

Extend `AggregateSelection` enum:
```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AggregateSelection {
    Count { alias: String },
    CountDistinct { field: String, alias: String },
    MeasureAggregate { measure: String, function: AggregateFunction, alias: String },

    // NEW: Advanced aggregates with additional options
    ArrayAggregate {
        column: String,
        order_by: Option<Vec<OrderByClause>>,
        alias: String,
    },
    JsonAggregate {
        columns: Vec<String>,  // Multiple columns to include in JSON object
        order_by: Option<Vec<OrderByClause>>,
        alias: String,
    },
    StringAggregate {
        column: String,
        delimiter: String,  // e.g., ", "
        order_by: Option<Vec<OrderByClause>>,
        alias: String,
    },
    BoolAggregate {
        column: String,
        function: BoolAggregateFunction,  // AND or OR
        alias: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoolAggregateFunction {
    And,
    Or,
}
```

### Step 3: Update SQL Generator (2-3 hours)

**File**: `crates/fraiseql-core/src/runtime/aggregation.rs`

Add database-specific SQL generation for each function:

**ARRAY_AGG**:
```rust
fn generate_array_agg_sql(&self, column: &str, order_by: &Option<Vec<OrderByClause>>) -> String {
    match self.database_type {
        DatabaseType::PostgreSQL => {
            let mut sql = format!("ARRAY_AGG({})", column);
            if let Some(order) = order_by {
                sql.push_str(&format!(" ORDER BY {}", self.order_by_to_sql(order)));
            }
            sql
        }
        DatabaseType::MySQL => {
            // MySQL doesn't have ARRAY_AGG, use JSON_ARRAYAGG
            format!("JSON_ARRAYAGG({})", column)
        }
        DatabaseType::SQLite => {
            // SQLite: emulate with GROUP_CONCAT, return as JSON array string
            let delimiter = "||CHR(30)||";  // Use record separator character
            format!("'[' || GROUP_CONCAT({}, '{}') || ']'", column, delimiter)
        }
        DatabaseType::SQLServer => {
            // SQL Server: use STRING_AGG and wrap in JSON array
            format!("'[' + STRING_AGG({}, ',') + ']'", column)
        }
    }
}
```

**JSON_AGG**:
```rust
fn generate_json_agg_sql(
    &self,
    columns: &[String],
    order_by: &Option<Vec<OrderByClause>>,
) -> String {
    match self.database_type {
        DatabaseType::PostgreSQL => {
            // Build JSON object with multiple columns
            let fields: Vec<String> = columns
                .iter()
                .map(|col| format!("'{}', {}", col, col))
                .collect();
            let json_obj = format!("JSON_BUILD_OBJECT({})", fields.join(", "));

            let mut sql = format!("JSON_AGG({})", json_obj);
            if let Some(order) = order_by {
                sql.push_str(&format!(" ORDER BY {}", self.order_by_to_sql(order)));
            }
            sql
        }
        DatabaseType::MySQL => {
            // MySQL: JSON_OBJECTAGG for key-value, JSON_ARRAYAGG for arrays
            if columns.len() == 1 {
                format!("JSON_ARRAYAGG({})", columns[0])
            } else {
                // Build array of objects
                format!("JSON_ARRAYAGG(JSON_OBJECT({}))",
                    columns.iter()
                        .map(|col| format!("'{}', {}", col, col))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        }
        DatabaseType::SQLite | DatabaseType::SQLServer => {
            // Limited JSON support - return as JSON string
            format!("JSON_ARRAY({})", columns.join(", "))
        }
    }
}
```

**STRING_AGG**:
```rust
fn generate_string_agg_sql(
    &self,
    column: &str,
    delimiter: &str,
    order_by: &Option<Vec<OrderByClause>>,
) -> String {
    match self.database_type {
        DatabaseType::PostgreSQL => {
            let mut sql = format!("STRING_AGG({}, '{}'", column, delimiter);
            if let Some(order) = order_by {
                sql.push_str(&format!(" ORDER BY {}", self.order_by_to_sql(order)));
            }
            sql.push(')');
            sql
        }
        DatabaseType::MySQL => {
            let mut sql = format!("GROUP_CONCAT({}",  column);
            if let Some(order) = order_by {
                sql.push_str(&format!(" ORDER BY {}", self.order_by_to_sql(order)));
            }
            sql.push_str(&format!(" SEPARATOR '{}')", delimiter));
            sql
        }
        DatabaseType::SQLite => {
            // SQLite GROUP_CONCAT doesn't support ORDER BY in older versions
            format!("GROUP_CONCAT({}, '{}')", column, delimiter)
        }
        DatabaseType::SQLServer => {
            let mut sql = format!("STRING_AGG({}, '{}')", column, delimiter);
            if let Some(order) = order_by {
                sql.push_str(&format!(" WITHIN GROUP (ORDER BY {})", self.order_by_to_sql(order)));
            }
            sql
        }
    }
}
```

**BOOL_AND / BOOL_OR**:
```rust
fn generate_bool_agg_sql(&self, column: &str, function: BoolAggregateFunction) -> String {
    match self.database_type {
        DatabaseType::PostgreSQL => {
            match function {
                BoolAggregateFunction::And => format!("BOOL_AND({})", column),
                BoolAggregateFunction::Or => format!("BOOL_OR({})", column),
            }
        }
        DatabaseType::MySQL | DatabaseType::SQLite => {
            // Emulate with MIN/MAX (treating 0 as false, 1 as true)
            match function {
                BoolAggregateFunction::And => format!("MIN({})", column),
                BoolAggregateFunction::Or => format!("MAX({})", column),
            }
        }
        DatabaseType::SQLServer => {
            // SQL Server: CAST to BIT and use MIN/MAX
            match function {
                BoolAggregateFunction::And => format!("MIN(CAST({} AS BIT))", column),
                BoolAggregateFunction::Or => format!("MAX(CAST({} AS BIT))", column),
            }
        }
    }
}
```

### Step 4: Update Parser (1 hour)

**File**: `crates/fraiseql-core/src/runtime/aggregate_parser.rs`

Add parsing for advanced aggregates:
```rust
fn parse_aggregate_selection(agg_name: &str, metadata: &FactTableMetadata) -> Result<AggregateSelection> {
    // Existing: count, count_distinct, measure_sum, measure_avg, etc.

    // NEW: array_agg_products
    if agg_name.ends_with("_array") {
        let column = agg_name.strip_suffix("_array").unwrap();
        return Ok(AggregateSelection::ArrayAggregate {
            column: column.to_string(),
            order_by: None,
            alias: agg_name.to_string(),
        });
    }

    // NEW: products_json
    if agg_name.ends_with("_json") {
        let column = agg_name.strip_suffix("_json").unwrap();
        return Ok(AggregateSelection::JsonAggregate {
            columns: vec![column.to_string()],
            order_by: None,
            alias: agg_name.to_string(),
        });
    }

    // NEW: products_string
    if agg_name.ends_with("_string") {
        let column = agg_name.strip_suffix("_string").unwrap();
        return Ok(AggregateSelection::StringAggregate {
            column: column.to_string(),
            delimiter: ", ".to_string(),
            order_by: None,
            alias: agg_name.to_string(),
        });
    }

    // NEW: is_active_all, has_discount_any
    if agg_name.ends_with("_all") {
        let column = agg_name.strip_suffix("_all").unwrap();
        return Ok(AggregateSelection::BoolAggregate {
            column: column.to_string(),
            function: BoolAggregateFunction::And,
            alias: agg_name.to_string(),
        });
    }

    if agg_name.ends_with("_any") {
        let column = agg_name.strip_suffix("_any").unwrap();
        return Ok(AggregateSelection::BoolAggregate {
            column: column.to_string(),
            function: BoolAggregateFunction::Or,
            alias: agg_name.to_string(),
        });
    }

    // ... existing code
}
```

### Step 5: Update GraphQL Type Generation (1 hour)

**File**: `crates/fraiseql-core/src/compiler/aggregate_types.rs`

Add GraphQL types for array/JSON results:
```rust
fn generate_aggregate_type_fields(metadata: &FactTableMetadata) -> Vec<GraphQLField> {
    let mut fields = vec![];

    // Existing: count (Int), sum (Float), avg (Float), etc.

    // NEW: Array aggregates
    for measure in &metadata.measures {
        fields.push(GraphQLField {
            name: format!("{}_array", measure.name),
            graphql_type: format!("[{}]", measure.graphql_type()),
            description: format!("Array of {} values", measure.name),
        });
    }

    // NEW: JSON aggregates
    fields.push(GraphQLField {
        name: "items_json".to_string(),
        graphql_type: "JSON".to_string(),
        description: "Aggregated items as JSON array".to_string(),
    });

    // NEW: String aggregates
    for measure in &metadata.measures {
        if measure.is_string_type() {
            fields.push(GraphQLField {
                name: format!("{}_string", measure.name),
                graphql_type: "String".to_string(),
                description: format!("Concatenated {} values", measure.name),
            });
        }
    }

    // NEW: Boolean aggregates
    for filter in &metadata.denormalized_filters {
        if filter.is_boolean_type() {
            fields.push(GraphQLField {
                name: format!("{}_all", filter.name),
                graphql_type: "Boolean".to_string(),
                description: format!("All {} are true", filter.name),
            });

            fields.push(GraphQLField {
                name: format!("{}_any", filter.name),
                graphql_type: "Boolean".to_string(),
                description: format!("Any {} is true", filter.name),
            });
        }
    }

    fields
}
```

### Step 6: Write Tests (2-3 hours)

**File**: `tests/integration/advanced_aggregates_test.rs`

Test scenarios:
1. ✅ ARRAY_AGG with ORDER BY
2. ✅ JSON_AGG with multiple columns
3. ✅ STRING_AGG with custom delimiter
4. ✅ BOOL_AND / BOOL_OR logic
5. ✅ Cross-database compatibility (PostgreSQL, MySQL, SQLite, SQL Server)
6. ✅ Projection of array/JSON results to GraphQL

**Example Test**:
```rust
#[test]
fn test_array_agg_postgres() {
    let metadata = create_test_metadata();
    let request = AggregationRequest {
        table_name: "tf_sales".to_string(),
        group_by: vec![GroupBySelection::Dimension {
            path: "category".to_string(),
            alias: "category".to_string(),
        }],
        aggregates: vec![
            AggregateSelection::ArrayAggregate {
                column: "product_id".to_string(),
                order_by: Some(vec![OrderByClause {
                    field: "revenue".to_string(),
                    direction: OrderDirection::Desc,
                }]),
                alias: "top_products".to_string(),
            },
        ],
        // ...
    };

    let plan = AggregationPlanner::plan(request, metadata).unwrap();
    let generator = AggregationSqlGenerator::new(DatabaseType::PostgreSQL);
    let sql = generator.generate(&plan).unwrap();

    assert!(sql.complete_sql.contains("ARRAY_AGG(product_id ORDER BY revenue DESC)"));
}
```

---

## Verification Checklist

- [ ] `cargo check` passes
- [ ] `cargo clippy` passes with no warnings
- [ ] All unit tests pass
- [ ] Integration tests pass for all 4 databases
- [ ] ARRAY_AGG generates correct SQL
- [ ] JSON_AGG generates correct SQL
- [ ] STRING_AGG generates correct SQL
- [ ] BOOL_AND/BOOL_OR generate correct SQL
- [ ] Parser handles advanced aggregate names
- [ ] GraphQL types include array/JSON fields
- [ ] Results project correctly to GraphQL

---

## Acceptance Criteria

### Functional Requirements
✅ ARRAY_AGG collects values into arrays
✅ JSON_AGG builds JSON objects/arrays
✅ STRING_AGG concatenates with delimiters
✅ BOOL_AND/BOOL_OR perform boolean logic
✅ ORDER BY works with aggregates
✅ All 4 databases supported

### Non-Functional Requirements
✅ Tests pass for all databases
✅ No performance degradation
✅ Code is documented
✅ Error handling for unsupported features

---

## Database Compatibility Matrix

| Function | PostgreSQL | MySQL | SQLite | SQL Server |
|----------|-----------|-------|--------|------------|
| ARRAY_AGG | ✅ Native | 🔄 JSON_ARRAYAGG | 🔄 Emulated | 🔄 STRING_AGG |
| JSON_AGG | ✅ Native | ✅ JSON_ARRAYAGG | 🔄 Limited | 🔄 FOR JSON |
| STRING_AGG | ✅ Native | ✅ GROUP_CONCAT | ✅ GROUP_CONCAT | ✅ STRING_AGG |
| BOOL_AND/OR | ✅ Native | 🔄 MIN/MAX | 🔄 MIN/MAX | 🔄 MIN/MAX CAST |

**Legend**:
- ✅ Native support
- 🔄 Emulated/alternative syntax
- ❌ Not supported

---

## Example Queries

### ARRAY_AGG - Top 5 Products per Category
```graphql
query {
  sales_aggregate(
    groupBy: { category: true }
  ) {
    category
    products_array  # Returns: ["prod_1", "prod_5", "prod_3"]
  }
}
```

### JSON_AGG - Nested Product Details
```graphql
query {
  sales_aggregate(
    groupBy: { category: true }
  ) {
    category
    items_json  # Returns: [{"product": "prod_1", "revenue": 1500}, ...]
  }
}
```

### STRING_AGG - Display List
```graphql
query {
  sales_aggregate(
    groupBy: { category: true }
  ) {
    category
    products_string  # Returns: "Laptop, Phone, Tablet"
  }
}
```

### BOOL_AND/OR - Flags
```graphql
query {
  sales_aggregate(
    groupBy: { category: true }
  ) {
    category
    is_active_all    # Returns: true if all products active
    has_discount_any # Returns: true if any product has discount
  }
}
```

---

**Ready to implement!** 🚀

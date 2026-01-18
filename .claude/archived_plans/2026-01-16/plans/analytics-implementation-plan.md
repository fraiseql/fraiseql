# FraiseQL v2 Analytics Implementation Plan

**Version**: 1.0
**Status**: Ready for Implementation
**Scope**: Complete analytics features from v1 (Phase 1-5)
**Estimated Effort**: 10-15 days
**Date**: 2026-01-12

---

## Overview

This plan implements production-ready analytics capabilities in FraiseQL v2, integrating all features from v1's analytics system:

- **Phase 1-2**: Core aggregations (GROUP BY, HAVING, temporal bucketing)
- **Phase 3**: Advanced aggregates (ARRAY_AGG, JSON_AGG, STRING_AGG)
- **Phase 4**: GraphQL integration (auto-generated types) - **Already in v2!**
- **Phase 5**: Window functions (ROW_NUMBER, RANK, LAG/LEAD, running totals)

---

## Architecture Principles

### No Joins

FraiseQL does NOT support joins. All dimensional data must be denormalized into `data` JSONB column at ETL time.

### Universal Pattern

ALL analytical tables (fact AND aggregate) use:

- **Measures**: SQL columns (INT, DECIMAL, FLOAT) - 10-100x faster aggregation
- **Dimensions**: JSONB `data` column - flexible GROUP BY
- **Denormalized filters**: Indexed SQL columns (customer_id, occurred_at) - fast WHERE

### Database Support

- PostgreSQL (full): JSONB, DATE_TRUNC, FILTER, STDDEV, VARIANCE
- MySQL (basic): JSON_EXTRACT, DATE_FORMAT, CASE WHEN emulation
- SQLite (minimal): json_extract, strftime, CASE WHEN emulation
- SQL Server (enterprise): JSON_VALUE, DATEPART, STDEV/VAR

---

## Implementation Phases

### Phase 1: Fact Table Introspection (2 days)

**Goal**: Detect fact tables and identify measures/dimensions/filters

#### Tasks

**1.1 Create `compiler/fact_table.rs`**

**Module Structure**:

```rust
// compiler/fact_table.rs

use crate::schema::CompiledSchema;
use crate::error::FraiseQLError;

pub struct FactTableDetector;

pub struct FactTableMetadata {
    pub table_name: String,
    pub measures: Vec<MeasureColumn>,
    pub dimensions: DimensionColumn,
    pub denormalized_filters: Vec<FilterColumn>,
}

pub struct MeasureColumn {
    pub name: String,
    pub sql_type: SqlType,  // INT, DECIMAL, FLOAT, BIGINT
    pub nullable: bool,
}

pub struct DimensionColumn {
    pub name: String,  // Default: "data"
    pub paths: Vec<DimensionPath>,
}

pub struct DimensionPath {
    pub name: String,       // e.g., "category"
    pub json_path: String,  // e.g., "data->>'category'" or "data#>>'{customer,segment}'"
    pub data_type: String,  // String, Int, Float
}

pub struct FilterColumn {
    pub name: String,
    pub sql_type: SqlType,
    pub indexed: bool,
}

impl FactTableDetector {
    /// Detect if table is a fact table (tf_* prefix)
    pub fn is_fact_table(table_name: &str) -> bool {
        table_name.starts_with("tf_")
    }

    /// Introspect fact table structure from database
    pub async fn introspect(
        db: &DatabaseConnection,
        table_name: &str
    ) -> Result<FactTableMetadata, FraiseQLError> {
        // 1. Query information_schema.columns to get all columns
        // 2. Identify measures: numeric types (INT, DECIMAL, FLOAT, BIGINT)
        // 3. Identify dimension column: JSONB type (default name: "data")
        // 4. Identify filters: indexed non-measure columns
        // 5. Extract JSONB paths from sample data (optional)
        todo!()
    }

    /// Validate fact table structure
    pub fn validate(metadata: &FactTableMetadata) -> Result<(), FraiseQLError> {
        // 1. Must have at least one measure
        // 2. Must have dimension column (JSONB)
        // 3. Measures must be numeric types
        // 4. No reserved column names
        todo!()
    }
}
```

**SQL Queries**:

```rust
// PostgreSQL introspection
const INTROSPECT_COLUMNS_POSTGRES: &str = r#"
SELECT
    column_name,
    data_type,
    is_nullable,
    column_default
FROM information_schema.columns
WHERE table_name = $1
ORDER BY ordinal_position;
"#;

const INTROSPECT_INDEXES_POSTGRES: &str = r#"
SELECT
    a.attname AS column_name,
    i.indisunique AS is_unique
FROM pg_index i
JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey)
WHERE i.indrelid = $1::regclass;
"#;

// MySQL introspection
const INTROSPECT_COLUMNS_MYSQL: &str = r#"
SELECT
    COLUMN_NAME,
    DATA_TYPE,
    IS_NULLABLE,
    COLUMN_DEFAULT
FROM information_schema.COLUMNS
WHERE TABLE_NAME = ?
ORDER BY ORDINAL_POSITION;
"#;

// SQLite introspection
const INTROSPECT_COLUMNS_SQLITE: &str = r#"
PRAGMA table_info(?);
"#;

// SQL Server introspection
const INTROSPECT_COLUMNS_SQLSERVER: &str = r#"
SELECT
    c.name AS column_name,
    t.name AS data_type,
    c.is_nullable,
    dc.definition AS column_default
FROM sys.columns c
JOIN sys.types t ON c.user_type_id = t.user_type_id
LEFT JOIN sys.default_constraints dc ON c.default_object_id = dc.object_id
WHERE c.object_id = OBJECT_ID(@table_name)
ORDER BY c.column_id;
"#;
```

**Tests**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_fact_table() {
        assert!(FactTableDetector::is_fact_table("tf_sales"));
        assert!(FactTableDetector::is_fact_table("tf_events"));
        assert!(!FactTableDetector::is_fact_table("ta_sales_by_day"));
        assert!(!FactTableDetector::is_fact_table("v_user"));
    }

    #[tokio::test]
    async fn test_introspect_fact_table_postgres() {
        // Test with real PostgreSQL connection
        let db = setup_test_db().await;

        // Create test fact table
        db.execute(r#"
            CREATE TABLE tf_test (
                id BIGSERIAL PRIMARY KEY,
                revenue DECIMAL(10,2) NOT NULL,
                quantity INT NOT NULL,
                data JSONB NOT NULL,
                customer_id UUID NOT NULL,
                occurred_at TIMESTAMPTZ NOT NULL
            );
        "#).await.unwrap();

        let metadata = FactTableDetector::introspect(&db, "tf_test").await.unwrap();

        assert_eq!(metadata.measures.len(), 2);
        assert_eq!(metadata.measures[0].name, "revenue");
        assert_eq!(metadata.measures[1].name, "quantity");
        assert_eq!(metadata.dimensions.name, "data");
        assert_eq!(metadata.denormalized_filters.len(), 2);
    }

    #[test]
    fn test_validate_fact_table() {
        let metadata = FactTableMetadata {
            table_name: "tf_sales".to_string(),
            measures: vec![
                MeasureColumn {
                    name: "revenue".to_string(),
                    sql_type: SqlType::Decimal,
                    nullable: false,
                }
            ],
            dimensions: DimensionColumn {
                name: "data".to_string(),
                paths: vec![],
            },
            denormalized_filters: vec![],
        };

        assert!(FactTableDetector::validate(&metadata).is_ok());
    }

    #[test]
    fn test_validate_missing_measures() {
        let metadata = FactTableMetadata {
            table_name: "tf_sales".to_string(),
            measures: vec![],  // No measures - should fail
            dimensions: DimensionColumn {
                name: "data".to_string(),
                paths: vec![],
            },
            denormalized_filters: vec![],
        };

        assert!(FactTableDetector::validate(&metadata).is_err());
    }
}
```

**Deliverables**:

- [ ] `compiler/fact_table.rs` module
- [ ] Database introspection for PostgreSQL, MySQL, SQLite, SQL Server
- [ ] Validation logic
- [ ] Unit tests (5+ test cases)
- [ ] Integration tests with real databases

**Effort**: 2 days

---

### Phase 2: Aggregate Type Generation (2-3 days)

**Goal**: Auto-generate GraphQL types for aggregations

#### Tasks

**2.1 Create `compiler/aggregate_types.rs`**

**Module Structure**:

```rust
// compiler/aggregate_types.rs

use crate::compiler::fact_table::FactTableMetadata;
use crate::schema::{GraphQLType, GraphQLInputType, GraphQLEnum};

pub struct AggregateTypeGenerator;

impl AggregateTypeGenerator {
    /// Generate {Type}Aggregate result type
    pub fn generate_aggregate_type(
        type_name: &str,
        metadata: &FactTableMetadata,
        database_target: DatabaseTarget,
    ) -> GraphQLType {
        // Example: SalesAggregate
        // Fields:
        // - Grouped dimensions (category: String, region: String, occurred_at_day: Date)
        // - Aggregated measures (count: Int!, revenue_sum: Float, revenue_avg: Float, ...)
        todo!()
    }

    /// Generate {Type}GroupByInput
    pub fn generate_group_by_input(
        type_name: &str,
        metadata: &FactTableMetadata,
    ) -> GraphQLInputType {
        // Example: SalesGroupByInput
        // Fields:
        // - category: Boolean
        // - region: Boolean
        // - occurred_at_day: Boolean
        // - occurred_at_week: Boolean
        // - occurred_at_month: Boolean
        // - occurred_at_quarter: Boolean
        // - occurred_at_year: Boolean
        todo!()
    }

    /// Generate {Type}HavingInput
    pub fn generate_having_input(
        type_name: &str,
        metadata: &FactTableMetadata,
        database_target: DatabaseTarget,
    ) -> GraphQLInputType {
        // Example: SalesHavingInput
        // Fields:
        // - revenue_sum_gt: Float
        // - revenue_sum_gte: Float
        // - revenue_sum_lt: Float
        // - revenue_sum_lte: Float
        // - revenue_sum_eq: Float
        // - count_gte: Int
        // - ...
        todo!()
    }

    /// Generate AggregateFunction enum (database-specific)
    pub fn generate_aggregate_function_enum(
        database_target: DatabaseTarget,
    ) -> GraphQLEnum {
        // PostgreSQL: COUNT, SUM, AVG, MIN, MAX, STDDEV, VARIANCE
        // MySQL: COUNT, SUM, AVG, MIN, MAX
        // SQLite: COUNT, SUM, AVG, MIN, MAX
        // SQL Server: COUNT, SUM, AVG, MIN, MAX, STDEV, VAR
        todo!()
    }

    /// Generate TemporalBucket enum (database-specific)
    pub fn generate_temporal_bucket_enum(
        database_target: DatabaseTarget,
    ) -> GraphQLEnum {
        // PostgreSQL: SECOND, MINUTE, HOUR, DAY, WEEK, MONTH, QUARTER, YEAR
        // MySQL: DAY, WEEK, MONTH, YEAR
        // SQLite: DAY, WEEK, MONTH, YEAR
        // SQL Server: DAY, WEEK, MONTH, QUARTER, YEAR, HOUR, MINUTE
        todo!()
    }
}
```

**Generated GraphQL Schema** (PostgreSQL target):

```graphql
type SalesAggregate {
  # Grouped dimensions
  category: String
  region: String
  occurred_at_day: Date
  occurred_at_week: Date
  occurred_at_month: Date
  occurred_at_quarter: Date
  occurred_at_year: Date

  # Aggregated measures
  count: Int!
  revenue_sum: Float
  revenue_avg: Float
  revenue_min: Float
  revenue_max: Float
  revenue_stddev: Float     # PostgreSQL only
  revenue_variance: Float   # PostgreSQL only
  quantity_sum: Int
  quantity_avg: Float
  quantity_min: Int
  quantity_max: Int
}

input SalesGroupByInput {
  category: Boolean
  region: Boolean
  occurred_at_day: Boolean
  occurred_at_week: Boolean
  occurred_at_month: Boolean
  occurred_at_quarter: Boolean
  occurred_at_year: Boolean
}

input SalesHavingInput {
  revenue_sum_gt: Float
  revenue_sum_gte: Float
  revenue_sum_lt: Float
  revenue_sum_lte: Float
  revenue_sum_eq: Float
  revenue_avg_gt: Float
  revenue_avg_gte: Float
  count_gte: Int
  count_eq: Int
}

enum AggregateFunction {
  COUNT
  COUNT_DISTINCT
  SUM
  AVG
  MIN
  MAX
  STDDEV      # PostgreSQL, SQL Server only
  VARIANCE    # PostgreSQL, SQL Server only
}

enum TemporalBucket {
  SECOND      # PostgreSQL only
  MINUTE      # PostgreSQL, SQL Server only
  HOUR        # PostgreSQL, SQL Server only
  DAY
  WEEK
  MONTH
  QUARTER     # PostgreSQL, SQL Server only
  YEAR
}

type Query {
  sales_aggregate(
    groupBy: SalesGroupByInput!
    where: SalesWhereInput
    having: SalesHavingInput
    orderBy: [OrderByInput!]
    limit: Int
    offset: Int
  ): [SalesAggregate!]!
}
```

**Tests**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_aggregate_type_postgres() {
        let metadata = create_test_metadata();
        let aggregate_type = AggregateTypeGenerator::generate_aggregate_type(
            "Sales",
            &metadata,
            DatabaseTarget::PostgreSQL,
        );

        assert_eq!(aggregate_type.name, "SalesAggregate");
        assert!(aggregate_type.has_field("count"));
        assert!(aggregate_type.has_field("revenue_sum"));
        assert!(aggregate_type.has_field("revenue_stddev"));  // PostgreSQL only
    }

    #[test]
    fn test_generate_aggregate_type_mysql() {
        let metadata = create_test_metadata();
        let aggregate_type = AggregateTypeGenerator::generate_aggregate_type(
            "Sales",
            &metadata,
            DatabaseTarget::MySQL,
        );

        assert!(!aggregate_type.has_field("revenue_stddev"));  // Not in MySQL
    }

    #[test]
    fn test_generate_group_by_input() {
        let metadata = create_test_metadata();
        let input_type = AggregateTypeGenerator::generate_group_by_input(
            "Sales",
            &metadata,
        );

        assert_eq!(input_type.name, "SalesGroupByInput");
        assert!(input_type.has_field("category"));
        assert!(input_type.has_field("occurred_at_day"));
    }

    #[test]
    fn test_generate_having_input() {
        let metadata = create_test_metadata();
        let input_type = AggregateTypeGenerator::generate_having_input(
            "Sales",
            &metadata,
            DatabaseTarget::PostgreSQL,
        );

        assert_eq!(input_type.name, "SalesHavingInput");
        assert!(input_type.has_field("revenue_sum_gt"));
        assert!(input_type.has_field("count_gte"));
    }

    #[test]
    fn test_aggregate_function_enum_postgres() {
        let enum_type = AggregateTypeGenerator::generate_aggregate_function_enum(
            DatabaseTarget::PostgreSQL,
        );

        assert!(enum_type.has_value("STDDEV"));
        assert!(enum_type.has_value("VARIANCE"));
    }

    #[test]
    fn test_aggregate_function_enum_mysql() {
        let enum_type = AggregateTypeGenerator::generate_aggregate_function_enum(
            DatabaseTarget::MySQL,
        );

        assert!(!enum_type.has_value("STDDEV"));  // Not in MySQL
    }

    #[test]
    fn test_temporal_bucket_enum_postgres() {
        let enum_type = AggregateTypeGenerator::generate_temporal_bucket_enum(
            DatabaseTarget::PostgreSQL,
        );

        assert!(enum_type.has_value("SECOND"));
        assert!(enum_type.has_value("QUARTER"));
    }

    #[test]
    fn test_temporal_bucket_enum_mysql() {
        let enum_type = AggregateTypeGenerator::generate_temporal_bucket_enum(
            DatabaseTarget::MySQL,
        );

        assert!(!enum_type.has_value("SECOND"));  // Not in MySQL
        assert!(!enum_type.has_value("QUARTER"));  // Not in MySQL
    }
}
```

**Deliverables**:

- [ ] `compiler/aggregate_types.rs` module
- [ ] Type generation for all databases
- [ ] Database-specific enum generation
- [ ] Unit tests (8+ test cases)
- [ ] Integration tests with schema compilation

**Effort**: 2-3 days

---

### Phase 3: Aggregation Execution Plan (2 days)

**Goal**: Generate execution plans for GROUP BY queries

#### Tasks

**3.1 Create `compiler/aggregation.rs`**

**Module Structure**:

```rust
// compiler/aggregation.rs

use crate::compiler::fact_table::FactTableMetadata;

pub struct AggregationPlanGenerator;

pub struct AggregationExecutionPlan {
    pub table: String,
    pub measures: Vec<AggregateMeasure>,
    pub dimensions: Vec<GroupByDimension>,
    pub where_clause: Option<WhereClause>,
    pub having_clause: Option<HavingClause>,
    pub order_by: Vec<OrderByClause>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

pub struct AggregateMeasure {
    pub function: AggregateFunction,
    pub column: String,
    pub alias: String,
    pub distinct: bool,
    pub filter: Option<WhereClause>,  // For FILTER (WHERE ...)
}

pub enum AggregateFunction {
    Count,
    CountDistinct,
    Sum,
    Avg,
    Min,
    Max,
    Stddev,      // PostgreSQL, SQL Server
    Variance,    // PostgreSQL, SQL Server
    ArrayAgg,    // Phase 3
    JsonAgg,     // Phase 3
    StringAgg,   // Phase 3
}

pub struct GroupByDimension {
    pub kind: DimensionKind,
    pub alias: String,
}

pub enum DimensionKind {
    JsonbPath {
        path: String,  // data->>'category'
    },
    Temporal {
        column: String,
        bucket: TemporalBucket,
    },
}

pub enum TemporalBucket {
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Quarter,
    Year,
}

pub struct HavingClause {
    pub conditions: Vec<HavingCondition>,
}

pub struct HavingCondition {
    pub aggregate_alias: String,
    pub operator: ComparisonOperator,
    pub value: serde_json::Value,
}

impl AggregationPlanGenerator {
    /// Generate aggregation execution plan from GraphQL query
    pub fn generate_plan(
        metadata: &FactTableMetadata,
        query: &AggregateQuery,
        database_target: DatabaseTarget,
    ) -> Result<AggregationExecutionPlan, FraiseQLError> {
        // 1. Parse groupBy input
        // 2. Parse aggregate functions
        // 3. Validate measures exist
        // 4. Validate dimensions exist
        // 5. Generate execution plan
        todo!()
    }

    /// Validate aggregation plan
    pub fn validate_plan(
        plan: &AggregationExecutionPlan,
        metadata: &FactTableMetadata,
        database_target: DatabaseTarget,
    ) -> Result<(), FraiseQLError> {
        // 1. Validate aggregate functions supported by database
        // 2. Validate temporal buckets supported by database
        // 3. Validate measures are numeric
        // 4. Validate HAVING references aggregates, not raw columns
        todo!()
    }
}
```

**Tests**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_simple_aggregation_plan() {
        let metadata = create_test_metadata();
        let query = AggregateQuery {
            group_by: vec!["category".to_string()],
            aggregates: vec![
                Aggregate {
                    function: AggregateFunction::Sum,
                    field: "revenue".to_string(),
                    alias: "revenue_sum".to_string(),
                    distinct: false,
                },
            ],
            where_clause: None,
            having: None,
        };

        let plan = AggregationPlanGenerator::generate_plan(
            &metadata,
            &query,
            DatabaseTarget::PostgreSQL,
        ).unwrap();

        assert_eq!(plan.table, "tf_sales");
        assert_eq!(plan.dimensions.len(), 1);
        assert_eq!(plan.measures.len(), 1);
    }

    #[test]
    fn test_generate_temporal_aggregation_plan() {
        let metadata = create_test_metadata();
        let query = AggregateQuery {
            group_by: vec!["occurred_at_day".to_string()],
            aggregates: vec![
                Aggregate {
                    function: AggregateFunction::Sum,
                    field: "revenue".to_string(),
                    alias: "revenue_sum".to_string(),
                    distinct: false,
                },
            ],
            where_clause: None,
            having: None,
        };

        let plan = AggregationPlanGenerator::generate_plan(
            &metadata,
            &query,
            DatabaseTarget::PostgreSQL,
        ).unwrap();

        match &plan.dimensions[0].kind {
            DimensionKind::Temporal { column, bucket } => {
                assert_eq!(column, "occurred_at");
                assert!(matches!(bucket, TemporalBucket::Day));
            }
            _ => panic!("Expected temporal dimension"),
        }
    }

    #[test]
    fn test_validate_unsupported_aggregate_function() {
        let metadata = create_test_metadata();
        let plan = AggregationExecutionPlan {
            table: "tf_sales".to_string(),
            measures: vec![
                AggregateMeasure {
                    function: AggregateFunction::Stddev,  // Not in MySQL
                    column: "revenue".to_string(),
                    alias: "revenue_stddev".to_string(),
                    distinct: false,
                    filter: None,
                },
            ],
            dimensions: vec![],
            where_clause: None,
            having_clause: None,
            order_by: vec![],
            limit: None,
            offset: None,
        };

        let result = AggregationPlanGenerator::validate_plan(
            &plan,
            &metadata,
            DatabaseTarget::MySQL,  // MySQL doesn't support STDDEV
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_validate_having_references_aggregate() {
        let metadata = create_test_metadata();
        let plan = AggregationExecutionPlan {
            table: "tf_sales".to_string(),
            measures: vec![
                AggregateMeasure {
                    function: AggregateFunction::Sum,
                    column: "revenue".to_string(),
                    alias: "revenue_sum".to_string(),
                    distinct: false,
                    filter: None,
                },
            ],
            dimensions: vec![],
            where_clause: None,
            having_clause: Some(HavingClause {
                conditions: vec![
                    HavingCondition {
                        aggregate_alias: "revenue_sum".to_string(),
                        operator: ComparisonOperator::GreaterThan,
                        value: serde_json::json!(1000),
                    },
                ],
            }),
            order_by: vec![],
            limit: None,
            offset: None,
        };

        let result = AggregationPlanGenerator::validate_plan(
            &plan,
            &metadata,
            DatabaseTarget::PostgreSQL,
        );

        assert!(result.is_ok());
    }
}
```

**Deliverables**:

- [ ] `compiler/aggregation.rs` module
- [ ] Execution plan generation
- [ ] Validation logic
- [ ] Unit tests (5+ test cases)

**Effort**: 2 days

---

### Phase 4: Runtime Aggregation SQL Generation (2-3 days)

**Goal**: Lower execution plans to database-specific SQL

#### Tasks

**4.1 Create `runtime/aggregation.rs`**

**Module Structure**:

```rust
// runtime/aggregation.rs

use crate::compiler::aggregation::AggregationExecutionPlan;

pub struct AggregationSqlGenerator;

impl AggregationSqlGenerator {
    /// Generate PostgreSQL aggregation SQL
    pub fn generate_postgres(plan: &AggregationExecutionPlan) -> String {
        let mut sql = String::new();

        // SELECT clause
        sql.push_str("SELECT ");

        // Add dimensions
        for dim in &plan.dimensions {
            match &dim.kind {
                DimensionKind::JsonbPath { path } => {
                    sql.push_str(&format!("{} AS {}, ", path, dim.alias));
                }
                DimensionKind::Temporal { column, bucket } => {
                    let func = temporal_function_postgres(bucket);
                    sql.push_str(&format!("{}('{}', {}) AS {}, ", func, bucket_to_string(bucket), column, dim.alias));
                }
            }
        }

        // Add measures
        for measure in &plan.measures {
            let func = aggregate_function_to_sql(&measure.function);
            if measure.distinct {
                sql.push_str(&format!("{}(DISTINCT {}) AS {}, ", func, measure.column, measure.alias));
            } else {
                sql.push_str(&format!("{}({}) AS {}, ", func, measure.column, measure.alias));
            }

            // Add FILTER clause if present (PostgreSQL only)
            if let Some(filter) = &measure.filter {
                sql.push_str(&format!(" FILTER (WHERE {}), ", generate_where_clause(filter)));
            }
        }

        // Remove trailing comma
        sql.pop();
        sql.pop();

        // FROM clause
        sql.push_str(&format!(" FROM {}", plan.table));

        // WHERE clause
        if let Some(where_clause) = &plan.where_clause {
            sql.push_str(&format!(" WHERE {}", generate_where_clause(where_clause)));
        }

        // GROUP BY clause
        if !plan.dimensions.is_empty() {
            sql.push_str(" GROUP BY ");
            for dim in &plan.dimensions {
                match &dim.kind {
                    DimensionKind::JsonbPath { path } => {
                        sql.push_str(&format!("{}, ", path));
                    }
                    DimensionKind::Temporal { column, bucket } => {
                        let func = temporal_function_postgres(bucket);
                        sql.push_str(&format!("{}('{}', {}), ", func, bucket_to_string(bucket), column));
                    }
                }
            }
            sql.pop();
            sql.pop();
        }

        // HAVING clause
        if let Some(having) = &plan.having_clause {
            sql.push_str(&format!(" HAVING {}", generate_having_clause(having)));
        }

        // ORDER BY clause
        if !plan.order_by.is_empty() {
            sql.push_str(&format!(" ORDER BY {}", generate_order_by(&plan.order_by)));
        }

        // LIMIT / OFFSET
        if let Some(limit) = plan.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }
        if let Some(offset) = plan.offset {
            sql.push_str(&format!(" OFFSET {}", offset));
        }

        sql
    }

    /// Generate MySQL aggregation SQL
    pub fn generate_mysql(plan: &AggregationExecutionPlan) -> String {
        // Similar to PostgreSQL but:
        // - Use JSON_EXTRACT instead of ->>, #>>
        // - Use DATE_FORMAT instead of DATE_TRUNC
        // - Emulate FILTER with CASE WHEN
        todo!()
    }

    /// Generate SQLite aggregation SQL
    pub fn generate_sqlite(plan: &AggregationExecutionPlan) -> String {
        // Similar to PostgreSQL but:
        // - Use json_extract instead of ->>
        // - Use strftime instead of DATE_TRUNC
        // - Emulate FILTER with CASE WHEN
        todo!()
    }

    /// Generate SQL Server aggregation SQL
    pub fn generate_sqlserver(plan: &AggregationExecutionPlan) -> String {
        // Similar to PostgreSQL but:
        // - Use JSON_VALUE instead of ->>
        // - Use DATEPART instead of DATE_TRUNC
        // - Emulate FILTER with CASE WHEN
        // - Use STDEV/VAR instead of STDDEV/VARIANCE
        todo!()
    }
}

fn temporal_function_postgres(bucket: &TemporalBucket) -> &'static str {
    "DATE_TRUNC"
}

fn bucket_to_string(bucket: &TemporalBucket) -> &'static str {
    match bucket {
        TemporalBucket::Second => "second",
        TemporalBucket::Minute => "minute",
        TemporalBucket::Hour => "hour",
        TemporalBucket::Day => "day",
        TemporalBucket::Week => "week",
        TemporalBucket::Month => "month",
        TemporalBucket::Quarter => "quarter",
        TemporalBucket::Year => "year",
    }
}

fn aggregate_function_to_sql(func: &AggregateFunction) -> &'static str {
    match func {
        AggregateFunction::Count => "COUNT",
        AggregateFunction::CountDistinct => "COUNT",
        AggregateFunction::Sum => "SUM",
        AggregateFunction::Avg => "AVG",
        AggregateFunction::Min => "MIN",
        AggregateFunction::Max => "MAX",
        AggregateFunction::Stddev => "STDDEV",
        AggregateFunction::Variance => "VARIANCE",
        _ => panic!("Unsupported aggregate function"),
    }
}
```

**Example Generated SQL** (PostgreSQL):

```sql
-- Simple aggregation
SELECT
    data->>'category' AS category,
    SUM(revenue) AS revenue_sum,
    COUNT(*) AS count
FROM tf_sales
GROUP BY data->>'category'
HAVING SUM(revenue) > $1
ORDER BY revenue_sum DESC
LIMIT 20;

-- Temporal aggregation
SELECT
    DATE_TRUNC('day', occurred_at) AS occurred_at_day,
    data->>'category' AS category,
    SUM(revenue) AS revenue_sum,
    AVG(revenue) AS revenue_avg,
    STDDEV(revenue) AS revenue_stddev
FROM tf_sales
WHERE occurred_at >= $1
GROUP BY DATE_TRUNC('day', occurred_at), data->>'category'
ORDER BY occurred_at_day ASC, category ASC;

-- Conditional aggregates (FILTER)
SELECT
    data->>'category' AS category,
    COUNT(*) AS total_count,
    SUM(revenue) FILTER (WHERE data->>'payment_method' = 'credit_card') AS revenue_cc,
    SUM(revenue) FILTER (WHERE data->>'payment_method' = 'paypal') AS revenue_paypal
FROM tf_sales
GROUP BY data->>'category';
```

**Example Generated SQL** (MySQL):

```sql
-- Simple aggregation (MySQL)
SELECT
    JSON_EXTRACT(data, '$.category') AS category,
    SUM(revenue) AS revenue_sum,
    COUNT(*) AS count
FROM tf_sales
GROUP BY JSON_EXTRACT(data, '$.category')
HAVING SUM(revenue) > ?
ORDER BY revenue_sum DESC
LIMIT 20;

-- Conditional aggregates (CASE WHEN emulation)
SELECT
    JSON_EXTRACT(data, '$.category') AS category,
    COUNT(*) AS total_count,
    SUM(CASE WHEN JSON_EXTRACT(data, '$.payment_method') = 'credit_card' THEN revenue ELSE 0 END) AS revenue_cc,
    SUM(CASE WHEN JSON_EXTRACT(data, '$.payment_method') = 'paypal' THEN revenue ELSE 0 END) AS revenue_paypal
FROM tf_sales
GROUP BY JSON_EXTRACT(data, '$.category');
```

**Tests**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_simple_aggregation_postgres() {
        let plan = create_simple_aggregation_plan();
        let sql = AggregationSqlGenerator::generate_postgres(&plan);

        assert!(sql.contains("SELECT"));
        assert!(sql.contains("data->>'category' AS category"));
        assert!(sql.contains("SUM(revenue) AS revenue_sum"));
        assert!(sql.contains("GROUP BY data->>'category'"));
    }

    #[test]
    fn test_generate_temporal_aggregation_postgres() {
        let plan = create_temporal_aggregation_plan();
        let sql = AggregationSqlGenerator::generate_postgres(&plan);

        assert!(sql.contains("DATE_TRUNC('day', occurred_at)"));
        assert!(sql.contains("GROUP BY DATE_TRUNC('day', occurred_at)"));
    }

    #[test]
    fn test_generate_having_clause_postgres() {
        let plan = create_aggregation_with_having();
        let sql = AggregationSqlGenerator::generate_postgres(&plan);

        assert!(sql.contains("HAVING SUM(revenue) > $1"));
    }

    #[test]
    fn test_generate_conditional_aggregate_postgres() {
        let plan = create_conditional_aggregate_plan();
        let sql = AggregationSqlGenerator::generate_postgres(&plan);

        assert!(sql.contains("FILTER (WHERE"));
    }

    #[test]
    fn test_generate_aggregation_mysql() {
        let plan = create_simple_aggregation_plan();
        let sql = AggregationSqlGenerator::generate_mysql(&plan);

        assert!(sql.contains("JSON_EXTRACT(data, '$.category')"));
        assert!(!sql.contains("->>")); // No PostgreSQL operators
    }

    #[tokio::test]
    async fn test_execute_aggregation_postgres() {
        let db = setup_test_db().await;

        // Insert test data
        insert_test_sales_data(&db).await;

        let plan = create_simple_aggregation_plan();
        let sql = AggregationSqlGenerator::generate_postgres(&plan);

        let results = db.execute(&sql).await.unwrap();

        assert!(results.len() > 0);
        assert!(results[0].contains_key("category"));
        assert!(results[0].contains_key("revenue_sum"));
    }
}
```

**Deliverables**:

- [ ] `runtime/aggregation.rs` module
- [ ] SQL generation for PostgreSQL
- [ ] SQL generation for MySQL
- [ ] SQL generation for SQLite
- [ ] SQL generation for SQL Server
- [ ] Unit tests (5+ test cases)
- [ ] Integration tests with real databases (4 databases)

**Effort**: 2-3 days

---

### Phase 5: Temporal Bucketing (1 day)

**Goal**: Database-specific temporal bucketing functions

#### Tasks

**5.1 Create `runtime/temporal.rs`**

**Module Structure**:

```rust
// runtime/temporal.rs

pub struct TemporalBucketGenerator;

impl TemporalBucketGenerator {
    /// Generate PostgreSQL DATE_TRUNC
    pub fn generate_postgres(column: &str, bucket: &TemporalBucket) -> String {
        format!("DATE_TRUNC('{}', {})", bucket_to_string(bucket), column)
    }

    /// Generate MySQL DATE_FORMAT
    pub fn generate_mysql(column: &str, bucket: &TemporalBucket) -> String {
        let format = match bucket {
            TemporalBucket::Day => "%Y-%m-%d",
            TemporalBucket::Week => "%Y-%u",  // Year-Week
            TemporalBucket::Month => "%Y-%m",
            TemporalBucket::Year => "%Y",
            _ => panic!("Unsupported temporal bucket for MySQL: {:?}", bucket),
        };
        format!("DATE_FORMAT({}, '{}')", column, format)
    }

    /// Generate SQLite strftime
    pub fn generate_sqlite(column: &str, bucket: &TemporalBucket) -> String {
        let format = match bucket {
            TemporalBucket::Day => "%Y-%m-%d",
            TemporalBucket::Week => "%Y-W%W",  // Year-Week
            TemporalBucket::Month => "%Y-%m",
            TemporalBucket::Year => "%Y",
            _ => panic!("Unsupported temporal bucket for SQLite: {:?}", bucket),
        };
        format!("strftime('{}', {})", format, column)
    }

    /// Generate SQL Server DATEPART
    pub fn generate_sqlserver(column: &str, bucket: &TemporalBucket) -> String {
        let part = match bucket {
            TemporalBucket::Second => "SECOND",
            TemporalBucket::Minute => "MINUTE",
            TemporalBucket::Hour => "HOUR",
            TemporalBucket::Day => "DAY",
            TemporalBucket::Week => "WEEK",
            TemporalBucket::Month => "MONTH",
            TemporalBucket::Quarter => "QUARTER",
            TemporalBucket::Year => "YEAR",
        };
        format!("DATEPART({}, {})", part, column)
    }
}
```

**Tests**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_postgres_day_bucket() {
        let sql = TemporalBucketGenerator::generate_postgres("occurred_at", &TemporalBucket::Day);
        assert_eq!(sql, "DATE_TRUNC('day', occurred_at)");
    }

    #[test]
    fn test_mysql_month_bucket() {
        let sql = TemporalBucketGenerator::generate_mysql("occurred_at", &TemporalBucket::Month);
        assert_eq!(sql, "DATE_FORMAT(occurred_at, '%Y-%m')");
    }

    #[test]
    fn test_sqlite_week_bucket() {
        let sql = TemporalBucketGenerator::generate_sqlite("occurred_at", &TemporalBucket::Week);
        assert_eq!(sql, "strftime('%Y-W%W', occurred_at)");
    }

    #[test]
    fn test_sqlserver_quarter_bucket() {
        let sql = TemporalBucketGenerator::generate_sqlserver("occurred_at", &TemporalBucket::Quarter);
        assert_eq!(sql, "DATEPART(QUARTER, occurred_at)");
    }

    #[tokio::test]
    async fn test_temporal_bucketing_postgres() {
        let db = setup_test_db().await;

        // Test actual temporal bucketing
        let sql = format!(
            "SELECT {}, COUNT(*) FROM tf_sales GROUP BY {}",
            TemporalBucketGenerator::generate_postgres("occurred_at", &TemporalBucket::Day),
            TemporalBucketGenerator::generate_postgres("occurred_at", &TemporalBucket::Day)
        );

        let results = db.execute(&sql).await.unwrap();
        assert!(results.len() > 0);
    }
}
```

**Deliverables**:

- [ ] `runtime/temporal.rs` module
- [ ] Temporal functions for all databases
- [ ] Unit tests (4+ test cases)
- [ ] Integration tests with real databases

**Effort**: 1 day

---

### Phase 6: Advanced Aggregates (Phase 3 - Optional, 2 days)

**Goal**: Implement ARRAY_AGG, JSON_AGG, STRING_AGG

#### Tasks

**6.1 Add Advanced Aggregate Functions**

**New Functions**:

```rust
pub enum AggregateFunction {
    // ... existing ...

    // Phase 3: Advanced aggregates
    ArrayAgg,      // PostgreSQL, MySQL (JSON_ARRAYAGG)
    JsonAgg,       // PostgreSQL, MySQL (JSON_OBJECTAGG)
    StringAgg,     // PostgreSQL, MySQL (GROUP_CONCAT), SQL Server
    BoolAnd,       // PostgreSQL
    BoolOr,        // PostgreSQL
}
```

**PostgreSQL**:

```sql
SELECT
    customer_id,
    ARRAY_AGG(product_id) AS products,
    JSON_AGG(data) AS orders,
    STRING_AGG(category, ', ') AS categories
FROM tf_sales
GROUP BY customer_id;
```

**MySQL**:

```sql
SELECT
    customer_id,
    JSON_ARRAYAGG(product_id) AS products,
    JSON_OBJECTAGG(order_id, data) AS orders,
    GROUP_CONCAT(category SEPARATOR ', ') AS categories
FROM tf_sales
GROUP BY customer_id;
```

**Deliverables**:

- [ ] Add advanced aggregate functions to `AggregateFunction` enum
- [ ] Update SQL generation for PostgreSQL
- [ ] Update SQL generation for MySQL
- [ ] Unit tests
- [ ] Integration tests

**Effort**: 2 days (optional, can defer)

---

### Phase 7: Window Functions (Phase 5 - 3-4 days)

**Goal**: Implement ROW_NUMBER, RANK, LAG/LEAD, running totals

#### Tasks

**7.1 Create `compiler/window_functions.rs`**

**Module Structure**:

```rust
// compiler/window_functions.rs

pub struct WindowFunctionPlanGenerator;

pub struct WindowExecutionPlan {
    pub table: String,
    pub select: Vec<String>,
    pub windows: Vec<WindowFunction>,
    pub where_clause: Option<WhereClause>,
    pub order_by: Vec<OrderByClause>,
    pub limit: Option<u32>,
}

pub struct WindowFunction {
    pub function: WindowFunctionType,
    pub alias: String,
    pub partition_by: Vec<String>,
    pub order_by: Vec<OrderByClause>,
    pub frame: Option<WindowFrame>,
}

pub enum WindowFunctionType {
    // Ranking
    RowNumber,
    Rank,
    DenseRank,
    Ntile { n: u32 },
    PercentRank,
    CumeDist,

    // Value
    Lag { field: String, offset: i32, default: Option<serde_json::Value> },
    Lead { field: String, offset: i32, default: Option<serde_json::Value> },
    FirstValue { field: String },
    LastValue { field: String },
    NthValue { field: String, n: u32 },

    // Aggregate as window
    Sum { field: String },
    Avg { field: String },
    Count { field: Option<String> },
    Min { field: String },
    Max { field: String },
}

pub struct WindowFrame {
    pub frame_type: FrameType,
    pub start: FrameBoundary,
    pub end: FrameBoundary,
    pub exclusion: Option<FrameExclusion>,  // PostgreSQL only
}

pub enum FrameType {
    Rows,
    Range,
    Groups,  // PostgreSQL only
}

pub enum FrameBoundary {
    UnboundedPreceding,
    NPreceding(u32),
    CurrentRow,
    NFollowing(u32),
    UnboundedFollowing,
}

pub enum FrameExclusion {
    CurrentRow,
    Group,
    Ties,
    NoOthers,
}
```

**7.2 Create `runtime/window.rs`**

**Module Structure**:

```rust
// runtime/window.rs

pub struct WindowSqlGenerator;

impl WindowSqlGenerator {
    /// Generate PostgreSQL window function SQL
    pub fn generate_postgres(plan: &WindowExecutionPlan) -> String {
        let mut sql = String::new();

        // SELECT clause
        sql.push_str("SELECT ");

        // Add regular columns
        for col in &plan.select {
            sql.push_str(&format!("{}, ", col));
        }

        // Add window functions
        for window in &plan.windows {
            sql.push_str(&generate_window_function_postgres(window));
            sql.push_str(", ");
        }

        sql.pop();
        sql.pop();

        // FROM clause
        sql.push_str(&format!(" FROM {}", plan.table));

        // WHERE clause
        if let Some(where_clause) = &plan.where_clause {
            sql.push_str(&format!(" WHERE {}", generate_where_clause(where_clause)));
        }

        // ORDER BY clause
        if !plan.order_by.is_empty() {
            sql.push_str(&format!(" ORDER BY {}", generate_order_by(&plan.order_by)));
        }

        // LIMIT
        if let Some(limit) = plan.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }

        sql
    }
}

fn generate_window_function_postgres(window: &WindowFunction) -> String {
    let func = match &window.function {
        WindowFunctionType::RowNumber => "ROW_NUMBER()".to_string(),
        WindowFunctionType::Rank => "RANK()".to_string(),
        WindowFunctionType::DenseRank => "DENSE_RANK()".to_string(),
        WindowFunctionType::Ntile { n } => format!("NTILE({})", n),
        WindowFunctionType::Lag { field, offset, default } => {
            let default_str = default.as_ref()
                .map(|v| format!(", {}", v))
                .unwrap_or_default();
            format!("LAG({}, {}{})", field, offset, default_str)
        }
        WindowFunctionType::Lead { field, offset, default } => {
            let default_str = default.as_ref()
                .map(|v| format!(", {}", v))
                .unwrap_or_default();
            format!("LEAD({}, {}{})", field, offset, default_str)
        }
        WindowFunctionType::Sum { field } => format!("SUM({})", field),
        // ... other functions
        _ => panic!("Unsupported window function"),
    };

    let mut sql = format!("{} OVER (", func);

    // PARTITION BY
    if !window.partition_by.is_empty() {
        sql.push_str("PARTITION BY ");
        sql.push_str(&window.partition_by.join(", "));
        sql.push_str(" ");
    }

    // ORDER BY
    if !window.order_by.is_empty() {
        sql.push_str("ORDER BY ");
        sql.push_str(&generate_order_by(&window.order_by));
        sql.push_str(" ");
    }

    // Frame clause
    if let Some(frame) = &window.frame {
        sql.push_str(&generate_frame_clause_postgres(frame));
    }

    sql.push_str(")");
    sql.push_str(&format!(" AS {}", window.alias));

    sql
}

fn generate_frame_clause_postgres(frame: &WindowFrame) -> String {
    let frame_type = match frame.frame_type {
        FrameType::Rows => "ROWS",
        FrameType::Range => "RANGE",
        FrameType::Groups => "GROUPS",
    };

    let start = boundary_to_string(&frame.start);
    let end = boundary_to_string(&frame.end);

    let mut sql = format!("{} BETWEEN {} AND {}", frame_type, start, end);

    if let Some(exclusion) = &frame.exclusion {
        let excl = match exclusion {
            FrameExclusion::CurrentRow => "EXCLUDE CURRENT ROW",
            FrameExclusion::Group => "EXCLUDE GROUP",
            FrameExclusion::Ties => "EXCLUDE TIES",
            FrameExclusion::NoOthers => "EXCLUDE NO OTHERS",
        };
        sql.push_str(&format!(" {}", excl));
    }

    sql
}

fn boundary_to_string(boundary: &FrameBoundary) -> String {
    match boundary {
        FrameBoundary::UnboundedPreceding => "UNBOUNDED PRECEDING".to_string(),
        FrameBoundary::NPreceding(n) => format!("{} PRECEDING", n),
        FrameBoundary::CurrentRow => "CURRENT ROW".to_string(),
        FrameBoundary::NFollowing(n) => format!("{} FOLLOWING", n),
        FrameBoundary::UnboundedFollowing => "UNBOUNDED FOLLOWING".to_string(),
    }
}
```

**Example Generated SQL** (PostgreSQL):

```sql
-- Running total
SELECT
    data->>'category' AS category,
    occurred_at,
    revenue,
    SUM(revenue) OVER (
        PARTITION BY data->>'category'
        ORDER BY occurred_at ASC
        ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW
    ) AS running_total
FROM tf_sales
ORDER BY category, occurred_at;

-- 7-day moving average
SELECT
    data->>'category' AS category,
    occurred_at,
    revenue,
    AVG(revenue) OVER (
        PARTITION BY data->>'category'
        ORDER BY occurred_at ASC
        ROWS BETWEEN 6 PRECEDING AND CURRENT ROW
    ) AS moving_avg_7d
FROM tf_sales;

-- Ranking
SELECT
    data->>'category' AS category,
    revenue,
    ROW_NUMBER() OVER (
        PARTITION BY data->>'category'
        ORDER BY revenue DESC
    ) AS rank_by_revenue
FROM tf_sales;

-- LAG/LEAD
SELECT
    data->>'category' AS category,
    occurred_at,
    revenue,
    LAG(revenue, 1) OVER (
        PARTITION BY data->>'category'
        ORDER BY occurred_at ASC
    ) AS prev_day_revenue,
    LEAD(revenue, 1) OVER (
        PARTITION BY data->>'category'
        ORDER BY occurred_at ASC
    ) AS next_day_revenue
FROM tf_sales;
```

**Tests**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_row_number() {
        let plan = create_row_number_plan();
        let sql = WindowSqlGenerator::generate_postgres(&plan);

        assert!(sql.contains("ROW_NUMBER() OVER"));
        assert!(sql.contains("PARTITION BY"));
        assert!(sql.contains("ORDER BY"));
    }

    #[test]
    fn test_generate_running_total() {
        let plan = create_running_total_plan();
        let sql = WindowSqlGenerator::generate_postgres(&plan);

        assert!(sql.contains("SUM(revenue) OVER"));
        assert!(sql.contains("ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW"));
    }

    #[test]
    fn test_generate_moving_average() {
        let plan = create_moving_average_plan();
        let sql = WindowSqlGenerator::generate_postgres(&plan);

        assert!(sql.contains("AVG(revenue) OVER"));
        assert!(sql.contains("ROWS BETWEEN 6 PRECEDING AND CURRENT ROW"));
    }

    #[test]
    fn test_generate_lag_lead() {
        let plan = create_lag_lead_plan();
        let sql = WindowSqlGenerator::generate_postgres(&plan);

        assert!(sql.contains("LAG(revenue, 1) OVER"));
        assert!(sql.contains("LEAD(revenue, 1) OVER"));
    }

    #[tokio::test]
    async fn test_execute_window_function_postgres() {
        let db = setup_test_db().await;
        insert_test_sales_data(&db).await;

        let plan = create_row_number_plan();
        let sql = WindowSqlGenerator::generate_postgres(&plan);

        let results = db.execute(&sql).await.unwrap();

        assert!(results.len() > 0);
        assert!(results[0].contains_key("rank_by_revenue"));
    }
}
```

**Deliverables**:

- [ ] `compiler/window_functions.rs` module
- [ ] `runtime/window.rs` module
- [ ] Window function SQL generation for PostgreSQL
- [ ] Window function SQL generation for MySQL (8.0+)
- [ ] Window function SQL generation for SQLite (3.25+)
- [ ] Window function SQL generation for SQL Server
- [ ] Unit tests (8+ test cases)
- [ ] Integration tests with real databases

**Effort**: 3-4 days

---

### Phase 8: Integration & Wiring (1-2 days)

**Goal**: Wire all modules into compiler and runtime

#### Tasks

**8.1 Update `compiler/mod.rs`**

```rust
// compiler/mod.rs

pub mod fact_table;
pub mod aggregate_types;
pub mod aggregation;
pub mod window_functions;

// ... existing modules ...

use fact_table::FactTableDetector;
use aggregate_types::AggregateTypeGenerator;
use aggregation::AggregationPlanGenerator;
use window_functions::WindowFunctionPlanGenerator;
```

**8.2 Update `compiler/validator.rs`**

Add validation for:

- [ ] Fact table structure
- [ ] Aggregation queries
- [ ] HAVING clause references
- [ ] Window function validity

**8.3 Update `compiler/codegen.rs`**

Add codegen for:

- [ ] Aggregate types
- [ ] GroupBy input types
- [ ] Having input types
- [ ] Window function types

**8.4 Update `runtime/mod.rs`**

```rust
// runtime/mod.rs

pub mod aggregation;
pub mod temporal;
pub mod window;

// ... existing modules ...

use aggregation::AggregationSqlGenerator;
use temporal::TemporalBucketGenerator;
use window::WindowSqlGenerator;
```

**8.5 Update `runtime/executor.rs`**

Add execution handlers for:

- [ ] Aggregation execution plans
- [ ] Window function execution plans

**Deliverables**:

- [ ] All modules wired into compiler
- [ ] All modules wired into runtime
- [ ] Validation rules added
- [ ] Codegen updated
- [ ] Executor updated

**Effort**: 1-2 days

---

### Phase 9: Integration Tests (2-3 days)

**Goal**: Comprehensive testing across all databases

#### Test Coverage

**Fact Table Tests**:

- [ ] Introspect PostgreSQL fact table
- [ ] Introspect MySQL fact table
- [ ] Introspect SQLite fact table
- [ ] Introspect SQL Server fact table
- [ ] Validate fact table structure
- [ ] Detect measures, dimensions, filters

**Aggregation Tests**:

- [ ] Simple aggregation (GROUP BY single dimension)
- [ ] Multi-dimensional aggregation
- [ ] Temporal aggregation (day, week, month, quarter, year)
- [ ] HAVING clause filtering
- [ ] Conditional aggregates (FILTER / CASE WHEN)
- [ ] Statistical functions (STDDEV, VARIANCE) - PostgreSQL/SQL Server only
- [ ] Execute on PostgreSQL
- [ ] Execute on MySQL
- [ ] Execute on SQLite
- [ ] Execute on SQL Server

**Window Function Tests**:

- [ ] ROW_NUMBER, RANK, DENSE_RANK
- [ ] Running totals
- [ ] Moving averages
- [ ] LAG/LEAD
- [ ] Execute on PostgreSQL
- [ ] Execute on MySQL 8.0+
- [ ] Execute on SQLite 3.25+
- [ ] Execute on SQL Server

**End-to-End Tests**:

- [ ] Complete GraphQL query → SQL → Results
- [ ] Type generation from fact table
- [ ] Query execution with all features

**Deliverables**:

- [ ] 30+ integration tests
- [ ] 4 database targets tested
- [ ] CI/CD integration

**Effort**: 2-3 days

---

## Summary

### Total Implementation Effort

| Phase | Component | Effort | Priority |
|-------|-----------|--------|----------|
| 1 | Fact Table Introspection | 2 days | ⭐⭐⭐ High |
| 2 | Aggregate Type Generation | 2-3 days | ⭐⭐⭐ High |
| 3 | Aggregation Execution Plan | 2 days | ⭐⭐⭐ High |
| 4 | Runtime Aggregation SQL | 2-3 days | ⭐⭐⭐ High |
| 5 | Temporal Bucketing | 1 day | ⭐⭐⭐ High |
| 6 | Advanced Aggregates | 2 days | ⭐⭐ Medium (Optional) |
| 7 | Window Functions | 3-4 days | ⭐⭐ Medium |
| 8 | Integration & Wiring | 1-2 days | ⭐⭐⭐ High |
| 9 | Integration Tests | 2-3 days | ⭐⭐⭐ High |

**Total**:

- **Core (Phase 1-5, 8-9)**: 12-16 days
- **With Window Functions (Phase 7)**: 15-20 days
- **Full (all phases)**: 17-22 days

### Recommended Approach

**Sprint 1** (1 week): Phase 1-3

- Fact table introspection
- Aggregate type generation
- Execution plan generation

**Sprint 2** (1 week): Phase 4-5

- Runtime SQL generation (all databases)
- Temporal bucketing

**Sprint 3** (1 week): Phase 8-9

- Integration & wiring
- Testing

**Sprint 4** (Optional, 1 week): Phase 7

- Window functions

---

## Next Steps

1. **Review this plan** - Confirm scope and priorities
2. **Set up development environment** - Database connections for testing
3. **Start with Phase 1** - Create fact_table.rs module
4. **TDD approach** - Write tests first, implement to pass

**Ready to begin?**

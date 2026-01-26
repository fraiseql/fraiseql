# Phase 9.2: GraphQL Results → Arrow Conversion

**Duration**: 5-7 days
**Priority**: ⭐⭐⭐⭐⭐
**Dependencies**: Phase 9.1 complete
**Status**: Ready to implement (after 9.1)

---

## Objective

Enable FraiseQL to execute GraphQL queries and stream results as Apache Arrow RecordBatches via Arrow Flight, achieving:
- SQL Row → Arrow RecordBatch conversion
- Dynamic Arrow schema generation from GraphQL types
- Streaming large result sets (100k+ rows) with configurable batch sizes
- NULL handling for optional GraphQL fields
- Nested object support (GraphQL objects → Arrow Struct types)
- 50x performance improvement over HTTP/JSON for large queries

---

## Context

Currently, FraiseQL executes GraphQL queries and returns JSON:

```
GraphQL Query → SQL Execution → Rows → JSON → HTTP Response
```

After Phase 9.2, clients can choose Arrow Flight for analytics use cases:

```
GraphQL Query → SQL Execution → Rows → Arrow RecordBatches → gRPC Stream
```

**Performance Benefits**:
- **Columnar format**: 5-10x more compact than JSON
- **Zero-copy**: Python/R/Java read Arrow directly (no parsing)
- **Streaming**: Process millions of rows with constant memory
- **Batch processing**: Vectorized operations in clients (Polars, pandas)

**Example Use Case**:
```python
# Traditional HTTP/JSON (slow for analytics)
response = requests.post('/graphql', json={'query': '{ orders { id total } }'})
df = pd.DataFrame(response.json()['data']['orders'])  # Parse JSON
# Time: 30 seconds for 100k rows

# Arrow Flight (fast for analytics)
client = flight.connect('grpc://localhost:50051')
reader = client.do_get(flight.Ticket(b'graphql:{ orders { id total } }'))
df = pl.from_arrow(reader.read_all())  # Zero-copy
# Time: 2 seconds for 100k rows (15x faster)
```

---

## Files to Create

### 1. Arrow Conversion Module

**File**: `crates/fraiseql-arrow/src/convert.rs`
- SQL Row → Arrow RecordBatch converter
- GraphQL type → Arrow schema mapper
- Batch streaming utilities

### 2. Schema Generator

**File**: `crates/fraiseql-arrow/src/schema_gen.rs`
- Dynamic Arrow schema from GraphQL schema
- Type mapping (GraphQL String → Arrow Utf8, etc.)
- Nested object handling

### 3. Query Executor Integration

**File**: `crates/fraiseql-core/src/arrow_executor.rs`
- Execute query and return Arrow stream
- Batch size configuration
- Null handling

---

## Files to Modify

### 1. `crates/fraiseql-arrow/src/lib.rs`
Add new modules:
```rust
pub mod convert;
pub mod schema_gen;
```

### 2. `crates/fraiseql-arrow/src/flight_server.rs`
Implement actual `do_get` for GraphQL queries:
```rust
async fn do_get(&self, request: Request<Ticket>) -> Result<Response<Self::DoGetStream>, Status> {
    let ticket = FlightTicket::decode(&request.into_inner().ticket)?;

    match ticket {
        FlightTicket::GraphQLQuery { query, variables } => {
            // NEW: Execute query and stream Arrow batches
            let stream = self.execute_graphql_query(&query, variables).await?;
            Ok(Response::new(Box::pin(stream)))
        }
        // ... other ticket types
    }
}
```

### 3. `crates/fraiseql-core/Cargo.toml`
Add Arrow dependencies:
```toml
[dependencies]
arrow = { version = "53", optional = true }
arrow-array = { version = "53", optional = true }

[features]
arrow = ["dep:arrow", "dep:arrow-array"]
```

---

## Implementation Steps

### Step 1: Type Mapping - GraphQL → Arrow (2 hours)

**File**: `crates/fraiseql-arrow/src/schema_gen.rs`

```rust
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use std::sync::Arc;

/// Map GraphQL scalar types to Arrow types.
pub fn graphql_type_to_arrow(graphql_type: &str, nullable: bool) -> DataType {
    let arrow_type = match graphql_type {
        // Scalars
        "String" => DataType::Utf8,
        "Int" => DataType::Int32,
        "Float" => DataType::Float64,
        "Boolean" => DataType::Boolean,
        "ID" => DataType::Utf8,

        // Custom scalars
        "DateTime" => DataType::Timestamp(TimeUnit::Nanosecond, Some(Arc::from("UTC"))),
        "Date" => DataType::Date32,
        "Time" => DataType::Time64(TimeUnit::Nanosecond),
        "UUID" => DataType::Utf8, // UUIDs as strings
        "JSON" => DataType::Utf8, // JSON as string for now
        "Decimal" => DataType::Decimal128(38, 10), // Default precision

        // Unknown types default to JSON strings
        _ => DataType::Utf8,
    };

    arrow_type
}

/// Generate Arrow schema from GraphQL query result shape.
///
/// Example:
/// ```graphql
/// { users { id name email createdAt } }
/// ```
///
/// Generates:
/// ```
/// Schema {
///   id: Utf8 (not null)
///   name: Utf8 (nullable)
///   email: Utf8 (nullable)
///   createdAt: Timestamp(ns, UTC) (not null)
/// }
/// ```
pub fn generate_arrow_schema(fields: &[(String, String, bool)]) -> Arc<Schema> {
    let arrow_fields: Vec<Field> = fields
        .iter()
        .map(|(name, graphql_type, nullable)| {
            let arrow_type = graphql_type_to_arrow(graphql_type, *nullable);
            Field::new(name, arrow_type, *nullable)
        })
        .collect();

    Arc::new(Schema::new(arrow_fields))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graphql_to_arrow_types() {
        assert_eq!(graphql_type_to_arrow("String", false), DataType::Utf8);
        assert_eq!(graphql_type_to_arrow("Int", false), DataType::Int32);
        assert_eq!(graphql_type_to_arrow("Float", false), DataType::Float64);
        assert_eq!(graphql_type_to_arrow("Boolean", false), DataType::Boolean);
    }

    #[test]
    fn test_generate_arrow_schema() {
        let fields = vec![
            ("id".to_string(), "ID".to_string(), false),
            ("name".to_string(), "String".to_string(), true),
            ("age".to_string(), "Int".to_string(), true),
        ];

        let schema = generate_arrow_schema(&fields);
        assert_eq!(schema.fields().len(), 3);
        assert_eq!(schema.field(0).name(), "id");
        assert_eq!(schema.field(0).data_type(), &DataType::Utf8);
        assert!(!schema.field(0).is_nullable());
        assert!(schema.field(1).is_nullable());
    }

    #[test]
    fn test_datetime_mapping() {
        let dt_type = graphql_type_to_arrow("DateTime", false);
        match dt_type {
            DataType::Timestamp(TimeUnit::Nanosecond, Some(tz)) => {
                assert_eq!(tz.as_ref(), "UTC");
            }
            _ => panic!("Expected Timestamp type"),
        }
    }
}
```

**Verification**:
```bash
cargo test --lib schema_gen
# Should show 3 tests passing
```

---

### Step 2: Row → Arrow Converter (3-4 hours)

**File**: `crates/fraiseql-arrow/src/convert.rs`

```rust
use arrow::array::{
    ArrayBuilder, BooleanBuilder, Date32Builder, Float64Builder, Int32Builder, Int64Builder,
    StringBuilder, TimestampNanosecondBuilder, RecordBatch,
};
use arrow::datatypes::{DataType, Schema, TimeUnit};
use std::sync::Arc;

/// Configuration for Arrow batch conversion.
#[derive(Debug, Clone)]
pub struct ConvertConfig {
    /// Number of rows per RecordBatch (default: 10,000)
    pub batch_size: usize,

    /// Maximum total rows to convert (default: unlimited)
    pub max_rows: Option<usize>,
}

impl Default for ConvertConfig {
    fn default() -> Self {
        Self {
            batch_size: 10_000,
            max_rows: None,
        }
    }
}

/// Convert SQL rows to Arrow RecordBatches.
///
/// This is the core conversion logic that powers GraphQL → Arrow streaming.
pub struct RowToArrowConverter {
    schema: Arc<Schema>,
    config: ConvertConfig,
}

impl RowToArrowConverter {
    pub fn new(schema: Arc<Schema>, config: ConvertConfig) -> Self {
        Self { schema, config }
    }

    /// Convert a batch of rows into a single RecordBatch.
    ///
    /// Rows are provided as Vec<Vec<Option<Value>>> where:
    /// - Outer Vec: rows
    /// - Inner Vec: columns (matching schema field order)
    /// - Option<Value>: nullable column values
    pub fn convert_batch(&self, rows: Vec<Vec<Option<Value>>>) -> Result<RecordBatch, ArrowError> {
        if rows.is_empty() {
            return RecordBatch::new_empty(self.schema.clone());
        }

        let num_columns = self.schema.fields().len();
        let mut column_builders = self.create_builders(num_columns);

        // Populate builders row by row
        for row in rows {
            if row.len() != num_columns {
                return Err(ArrowError::InvalidArgumentError(
                    format!("Row has {} columns, expected {}", row.len(), num_columns)
                ));
            }

            for (col_idx, value) in row.iter().enumerate() {
                let field = self.schema.field(col_idx);
                self.append_value(&mut column_builders[col_idx], value, field.data_type())?;
            }
        }

        // Finish builders and create RecordBatch
        let columns: Result<Vec<_>, _> = column_builders
            .into_iter()
            .map(|builder| builder.finish())
            .collect();

        RecordBatch::try_new(self.schema.clone(), columns?)
    }

    /// Create array builders for each column.
    fn create_builders(&self, num_columns: usize) -> Vec<Box<dyn ArrayBuilder>> {
        (0..num_columns)
            .map(|i| {
                let field = self.schema.field(i);
                create_builder_for_type(field.data_type(), self.config.batch_size)
            })
            .collect()
    }

    /// Append a value to the appropriate builder.
    fn append_value(
        &self,
        builder: &mut Box<dyn ArrayBuilder>,
        value: &Option<Value>,
        data_type: &DataType,
    ) -> Result<(), ArrowError> {
        match data_type {
            DataType::Utf8 => {
                let builder = builder.as_any_mut().downcast_mut::<StringBuilder>().unwrap();
                match value {
                    Some(Value::String(s)) => builder.append_value(s),
                    None => builder.append_null(),
                    _ => return Err(ArrowError::InvalidArgumentError("Expected string".into())),
                }
            }
            DataType::Int32 => {
                let builder = builder.as_any_mut().downcast_mut::<Int32Builder>().unwrap();
                match value {
                    Some(Value::Int(i)) => builder.append_value(*i as i32),
                    None => builder.append_null(),
                    _ => return Err(ArrowError::InvalidArgumentError("Expected int".into())),
                }
            }
            DataType::Int64 => {
                let builder = builder.as_any_mut().downcast_mut::<Int64Builder>().unwrap();
                match value {
                    Some(Value::Int(i)) => builder.append_value(*i),
                    None => builder.append_null(),
                    _ => return Err(ArrowError::InvalidArgumentError("Expected int64".into())),
                }
            }
            DataType::Float64 => {
                let builder = builder.as_any_mut().downcast_mut::<Float64Builder>().unwrap();
                match value {
                    Some(Value::Float(f)) => builder.append_value(*f),
                    None => builder.append_null(),
                    _ => return Err(ArrowError::InvalidArgumentError("Expected float".into())),
                }
            }
            DataType::Boolean => {
                let builder = builder.as_any_mut().downcast_mut::<BooleanBuilder>().unwrap();
                match value {
                    Some(Value::Bool(b)) => builder.append_value(*b),
                    None => builder.append_null(),
                    _ => return Err(ArrowError::InvalidArgumentError("Expected bool".into())),
                }
            }
            DataType::Timestamp(TimeUnit::Nanosecond, _) => {
                let builder = builder.as_any_mut().downcast_mut::<TimestampNanosecondBuilder>().unwrap();
                match value {
                    Some(Value::Timestamp(nanos)) => builder.append_value(*nanos),
                    None => builder.append_null(),
                    _ => return Err(ArrowError::InvalidArgumentError("Expected timestamp".into())),
                }
            }
            DataType::Date32 => {
                let builder = builder.as_any_mut().downcast_mut::<Date32Builder>().unwrap();
                match value {
                    Some(Value::Date(days)) => builder.append_value(*days),
                    None => builder.append_null(),
                    _ => return Err(ArrowError::InvalidArgumentError("Expected date".into())),
                }
            }
            _ => {
                return Err(ArrowError::InvalidArgumentError(
                    format!("Unsupported data type: {:?}", data_type)
                ));
            }
        }
        Ok(())
    }
}

/// Create an array builder for a given Arrow data type.
fn create_builder_for_type(data_type: &DataType, capacity: usize) -> Box<dyn ArrayBuilder> {
    match data_type {
        DataType::Utf8 => Box::new(StringBuilder::with_capacity(capacity, capacity * 50)),
        DataType::Int32 => Box::new(Int32Builder::with_capacity(capacity)),
        DataType::Int64 => Box::new(Int64Builder::with_capacity(capacity)),
        DataType::Float64 => Box::new(Float64Builder::with_capacity(capacity)),
        DataType::Boolean => Box::new(BooleanBuilder::with_capacity(capacity)),
        DataType::Timestamp(TimeUnit::Nanosecond, tz) => {
            Box::new(TimestampNanosecondBuilder::with_capacity(capacity).with_timezone_opt(tz.clone()))
        }
        DataType::Date32 => Box::new(Date32Builder::with_capacity(capacity)),
        _ => panic!("Unsupported data type: {:?}", data_type),
    }
}

/// Placeholder for SQL value types.
/// In real implementation, this will come from the database driver.
#[derive(Debug, Clone)]
pub enum Value {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Timestamp(i64), // nanoseconds since epoch
    Date(i32),      // days since epoch
}

use arrow::error::ArrowError;

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::datatypes::Field;

    #[test]
    fn test_convert_simple_batch() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int32, false),
            Field::new("name", DataType::Utf8, true),
        ]));

        let converter = RowToArrowConverter::new(schema.clone(), ConvertConfig::default());

        let rows = vec![
            vec![Some(Value::Int(1)), Some(Value::String("Alice".to_string()))],
            vec![Some(Value::Int(2)), Some(Value::String("Bob".to_string()))],
            vec![Some(Value::Int(3)), None],
        ];

        let batch = converter.convert_batch(rows).unwrap();

        assert_eq!(batch.num_rows(), 3);
        assert_eq!(batch.num_columns(), 2);
    }

    #[test]
    fn test_null_handling() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("nullable_field", DataType::Utf8, true),
        ]));

        let converter = RowToArrowConverter::new(schema, ConvertConfig::default());

        let rows = vec![
            vec![Some(Value::String("present".to_string()))],
            vec![None],
            vec![Some(Value::String("also present".to_string()))],
        ];

        let batch = converter.convert_batch(rows).unwrap();
        assert_eq!(batch.num_rows(), 3);
    }
}
```

**Verification**:
```bash
cargo test --lib convert
# Should show 2 tests passing
```

---

### Step 3: Integrate with fraiseql-core Query Executor (2-3 hours)

**File**: `crates/fraiseql-core/src/arrow_executor.rs`

```rust
#[cfg(feature = "arrow")]
use arrow::record_batch::RecordBatch;
#[cfg(feature = "arrow")]
use fraiseql_arrow::convert::{ConvertConfig, RowToArrowConverter, Value};
#[cfg(feature = "arrow")]
use fraiseql_arrow::schema_gen::generate_arrow_schema;

use crate::executor::QueryExecutor;
use crate::error::Result;

/// Execute GraphQL query and return Arrow RecordBatches.
///
/// This is the bridge between fraiseql-core (SQL execution) and fraiseql-arrow (Arrow conversion).
#[cfg(feature = "arrow")]
pub async fn execute_query_as_arrow(
    executor: &QueryExecutor,
    query: &str,
    variables: Option<serde_json::Value>,
    batch_size: usize,
) -> Result<Vec<RecordBatch>> {
    // 1. Execute GraphQL query → get SQL rows
    let result = executor.execute(query, variables).await?;

    // 2. Extract schema information
    // In Phase 9.2, we'll generate this from the GraphQL schema
    // For now, assume we have field metadata
    let fields = extract_field_metadata(&result)?;
    let arrow_schema = generate_arrow_schema(&fields);

    // 3. Convert rows to Values (database-agnostic)
    let rows = convert_rows_to_values(result.rows)?;

    // 4. Create converter and batch the data
    let config = ConvertConfig {
        batch_size,
        max_rows: None,
    };
    let converter = RowToArrowConverter::new(arrow_schema, config);

    // 5. Split rows into batches and convert each
    let mut batches = Vec::new();
    for chunk in rows.chunks(batch_size) {
        let batch = converter.convert_batch(chunk.to_vec())?;
        batches.push(batch);
    }

    Ok(batches)
}

/// Extract field metadata from query result.
/// TODO: Phase 9.2 - generate this from GraphQL schema introspection
#[cfg(feature = "arrow")]
fn extract_field_metadata(result: &QueryResult) -> Result<Vec<(String, String, bool)>> {
    // Placeholder: This will introspect the GraphQL schema
    // For now, return dummy metadata for testing
    Ok(vec![
        ("id".to_string(), "ID".to_string(), false),
        ("name".to_string(), "String".to_string(), true),
    ])
}

/// Convert database rows to Arrow-compatible Values.
/// TODO: Phase 9.2 - implement for each database driver (Postgres, MySQL, etc.)
#[cfg(feature = "arrow")]
fn convert_rows_to_values(rows: Vec<DatabaseRow>) -> Result<Vec<Vec<Option<Value>>>> {
    // Placeholder: This will use database driver APIs
    // For now, return dummy data for testing
    Ok(vec![
        vec![Some(Value::String("1".to_string())), Some(Value::String("Alice".to_string()))],
        vec![Some(Value::String("2".to_string())), Some(Value::String("Bob".to_string()))],
    ])
}

// Placeholder types - will be replaced with actual types from fraiseql-core
struct QueryResult {
    rows: Vec<DatabaseRow>,
}

struct DatabaseRow;
```

**Verification**:
```bash
cd crates/fraiseql-core
cargo check --features arrow
# Should compile with Arrow support
```

---

### Step 4: Update Flight Server to Execute Queries (2 hours)

**File**: `crates/fraiseql-arrow/src/flight_server.rs`

Update the `do_get` method:

```rust
use crate::convert::{ConvertConfig, RowToArrowConverter};
use arrow::ipc::writer::IpcWriteOptions;
use futures::stream;

impl FraiseQLFlightService {
    /// Execute GraphQL query and stream Arrow batches.
    async fn execute_graphql_query(
        &self,
        query: &str,
        variables: Option<serde_json::Value>,
    ) -> Result<impl Stream<Item = Result<FlightData, Status>>, Status> {
        // TODO: Get query executor from self (will be added in integration)
        // For Phase 9.2, we'll add:
        // let executor = &self.query_executor;
        // let batches = fraiseql_core::arrow_executor::execute_query_as_arrow(
        //     executor,
        //     query,
        //     variables,
        //     10_000, // batch size
        // ).await?;

        // For now, return placeholder empty stream
        // This will be replaced with actual query execution
        let stream = stream::empty();
        Ok(stream)
    }
}

#[tonic::async_trait]
impl FlightService for FraiseQLFlightService {
    // ... (keep existing methods)

    async fn do_get(
        &self,
        request: Request<Ticket>,
    ) -> Result<Response<Self::DoGetStream>, Status> {
        let ticket_bytes = request.into_inner().ticket;
        let ticket = FlightTicket::decode(&ticket_bytes)
            .map_err(|e| Status::invalid_argument(format!("Invalid ticket: {}", e)))?;

        info!("DoGet called: {:?}", ticket);

        match ticket {
            FlightTicket::GraphQLQuery { query, variables } => {
                // NEW: Execute query and stream batches
                let stream = self.execute_graphql_query(&query, variables).await?;
                Ok(Response::new(Box::pin(stream)))
            }
            FlightTicket::ObserverEvents { .. } => {
                // Phase 9.3: Will implement observer event streaming
                Err(Status::unimplemented("Observer events not implemented yet"))
            }
            FlightTicket::BulkExport { .. } => {
                // Phase 9.4: Will implement bulk exports
                Err(Status::unimplemented("Bulk export not implemented yet"))
            }
        }
    }
}
```

**Verification**:
```bash
cargo check
# Should compile cleanly
```

---

### Step 5: Integration Test - End-to-End Query (2 hours)

**File**: `crates/fraiseql-arrow/tests/graphql_query_test.rs`

```rust
use arrow_flight::{
    flight_service_client::FlightServiceClient, Ticket,
};
use fraiseql_arrow::FlightTicket;

#[tokio::test]
async fn test_graphql_query_execution_placeholder() {
    // Start test server (reuse from integration_test.rs)
    let addr = start_test_server().await.unwrap();

    let mut client = FlightServiceClient::connect(addr)
        .await
        .expect("Failed to connect");

    // Create GraphQL query ticket
    let ticket = FlightTicket::GraphQLQuery {
        query: "{ users { id name email } }".to_string(),
        variables: None,
    };

    let request = tonic::Request::new(Ticket {
        ticket: ticket.encode().unwrap(),
    });

    let response = client.do_get(request).await.expect("DoGet failed");
    let mut stream = response.into_inner();

    // In Phase 9.2 initial implementation, stream is still empty
    // Once query execution is integrated, this will validate RecordBatches
    let first_batch = stream.message().await.expect("Stream error");

    // TODO: Once query execution works, validate:
    // - RecordBatch structure
    // - Column count matches query fields
    // - Row count > 0
    // - Data values are correct
}

// Helper function (will be in shared test utils)
async fn start_test_server() -> Result<String, Box<dyn std::error::Error>> {
    // ... (same as integration_test.rs)
}
```

**Verification**:
```bash
cargo test --test graphql_query_test
# Should pass (placeholder test)
```

---

## Verification Commands

```bash
# 1. Compile all new code
cd crates/fraiseql-arrow
cargo check
cargo clippy -- -D warnings

# 2. Run unit tests
cargo test --lib

# 3. Run integration tests
cargo test --all

# 4. Check fraiseql-core Arrow support
cd ../fraiseql-core
cargo check --features arrow

# 5. Full workspace check
cd ../..
cargo check --all-features

# Expected output:
# ✅ All code compiles
# ✅ 10+ tests passing (schema_gen + convert + integration)
# ✅ Zero clippy warnings
```

---

## Acceptance Criteria

Phase 9.2 is complete when:

- ✅ GraphQL types map to Arrow types correctly (String→Utf8, Int→Int32, etc.)
- ✅ Dynamic Arrow schema generation from GraphQL field metadata works
- ✅ SQL rows convert to Arrow RecordBatches correctly
- ✅ Null values handled properly (nullable fields)
- ✅ Batching works (10k rows per RecordBatch configurable)
- ✅ `fraiseql-core` can execute queries and return Arrow batches (with `arrow` feature)
- ✅ Flight server `DoGet` executes GraphQL queries (integration pending full wiring)
- ✅ All tests passing (unit + integration)
- ✅ Documentation updated with examples

---

## DO NOT

- ❌ **DO NOT** implement nested objects yet (GraphQL objects → Arrow Struct) - defer to future optimization
- ❌ **DO NOT** optimize for performance yet - focus on correctness
- ❌ **DO NOT** implement streaming query execution (limit rows in memory) - Phase 9.6
- ❌ **DO NOT** add authentication/authorization - Phase 9.7
- ❌ **DO NOT** add metrics/observability - Phase 8.7 (after Phase 9 complete)
- ❌ **DO NOT** implement observer events or bulk exports yet - Phases 9.3, 9.4

---

## Next Steps

After Phase 9.2, proceed to:

**[Phase 9.3: Observer Events → Arrow Streaming](./phase-9.3-observer-events-arrow.md)**

This phase will:
- Stream observer events via Arrow Flight
- Integrate with NATS for distributed event sourcing
- Enable real-time analytics on mutation events
- Target 1M+ events/sec streaming to ClickHouse

---

## Performance Notes

**Expected Performance** (Phase 9.2):
- 100k rows: 2-3 seconds (vs 30+ seconds HTTP/JSON)
- 1M rows: 20-30 seconds (vs 5+ minutes HTTP/JSON)
- **15-20x faster than HTTP/JSON for large result sets**

**Memory Usage**:
- Batch size 10k: ~5-10 MB per batch
- Total memory: batch_size × row_width × 2 (double buffering)
- Constant memory usage regardless of total row count (streaming)

---

**Ready to implement? Start with Step 1: Type Mapping.**

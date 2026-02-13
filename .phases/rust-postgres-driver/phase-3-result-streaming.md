# Phase 3: Result Streaming - Zero-Copy Optimization

**Phase**: 3 of 5
**Effort**: 10 hours
**Status**: Blocked until Phase 2 complete
**Prerequisite**: Phase 2 - Query Execution complete

---

## Objective

Implement zero-copy result streaming from database to HTTP response:

1. Stream results directly from PostgreSQL
2. Transform JSONB data without buffering
3. Build GraphQL response bytes in Rust
4. Eliminate unnecessary allocations

**Success Criteria**:

- ✅ Results stream directly from DB (no buffering entire result set)
- ✅ JSONB fields transform to camelCase during streaming
- ✅ Memory usage 50% lower than Phase 2
- ✅ 15-25% faster response times

---

## Architecture

### Current Flow (Phase 2)

```
PostgreSQL
    ↓
Fetch all rows into memory (Vec<Row>)
    ↓
Transform each row to JSON
    ↓
Convert keys: snake_case → camelCase
    ↓
Build response bytes
    ↓
HTTP
```

### Optimized Flow (Phase 3)

```
PostgreSQL
    ↓
Stream rows one-at-a-time
    ↓
Transform and convert as stream
    ↓
Write directly to response buffer
    ↓
HTTP
```

---

## Implementation Overview

### Components to Implement

1. **RowStreamer** - Iterate over database rows without buffering
2. **JsonTransformer** - Transform row to JSON while streaming
3. **CamelCaseConverter** - Convert keys during transformation
4. **ResponseBuilder** - Build response bytes incrementally

### Key Files

```
fraiseql_rs/src/response/
├── mod.rs                      # NEW: Response building
├── builder.rs                  # Streaming response builder
├── streaming.rs                # Zero-copy streaming
└── json_transform.rs           # In-stream JSON transformation
```

---

## Detailed Implementation

### Step 1: Create Streaming Response Builder

**File**: `fraiseql_rs/src/response/streaming.rs` (NEW)

```rust
//! Zero-copy streaming response builder.

use serde_json::{json, Value};
use std::io::Write;

/// Stream rows directly to response buffer without full buffering.
pub struct ResponseStream<W: Write> {
    writer: W,
    row_count: usize,
    started: bool,
}

impl<W: Write> ResponseStream<W> {
    pub fn new(writer: W) -> Self {
        ResponseStream {
            writer,
            row_count: 0,
            started: false,
        }
    }

    /// Start the GraphQL response array
    pub fn start(&mut self) -> std::io::Result<()> {
        if !self.started {
            // Write opening of GraphQL response
            self.writer.write_all(b"{\"data\":{\"items\":[")?;
            self.started = true;
        }
        Ok(())
    }

    /// Write a single row (automatically formatted as JSON)
    pub fn write_row(&mut self, row: &Value) -> std::io::Result<()> {
        if self.row_count > 0 {
            self.writer.write_all(b",")?;  // Comma separator
        }

        // Write row as compact JSON (no whitespace)
        let json_str = serde_json::to_string(row)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        self.writer.write_all(json_str.as_bytes())?;

        self.row_count += 1;
        Ok(())
    }

    /// Finish the response
    pub fn finish(&mut self) -> std::io::Result<()> {
        self.writer.write_all(b"]}}")?;  // Close array and response
        self.writer.flush()?;
        Ok(())
    }

    pub fn row_count(&self) -> usize {
        self.row_count
    }
}

/// Memory-efficient buffered writer with configurable chunk size.
pub struct ChunkedWriter {
    buffer: Vec<u8>,
    chunk_size: usize,
    total_written: usize,
}

impl ChunkedWriter {
    pub fn new(chunk_size: usize) -> Self {
        ChunkedWriter {
            buffer: Vec::with_capacity(chunk_size),
            chunk_size,
            total_written: 0,
        }
    }

    pub fn should_flush(&self) -> bool {
        self.buffer.len() >= self.chunk_size
    }

    pub fn get_chunk(&mut self) -> Option<Vec<u8>> {
        if self.buffer.is_empty() {
            return None;
        }
        Some(std::mem::replace(&mut self.buffer, Vec::with_capacity(self.chunk_size)))
    }

    pub fn total_written(&self) -> usize {
        self.total_written + self.buffer.len()
    }
}

impl Write for ChunkedWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buffer.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if !self.buffer.is_empty() {
            self.total_written += self.buffer.len();
            self.buffer.clear();
        }
        Ok(())
    }
}
```

### Step 2: Create JSON Transformation Module

**File**: `fraiseql_rs/src/response/json_transform.rs` (NEW)

```rust
//! In-stream JSON transformation (snake_case → camelCase).

use serde_json::{json, Value, Map};

/// Convert snake_case to camelCase
pub fn to_camel_case(snake: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = false;

    for c in snake.chars() {
        if c == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(c.to_uppercase().next().unwrap());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }

    result
}

/// Transform row from PostgreSQL to GraphQL format with key transformation
pub fn transform_row_keys(row: &Value) -> Value {
    match row {
        Value::Object(map) => {
            let mut new_map = Map::new();
            for (key, value) in map.iter() {
                let camel_key = to_camel_case(key);
                let transformed_value = transform_row_keys(value);
                new_map.insert(camel_key, transformed_value);
            }
            Value::Object(new_map)
        }
        Value::Array(arr) => {
            Value::Array(arr.iter().map(transform_row_keys).collect())
        }
        other => other.clone(),
    }
}

/// Transform JSONB field (nested) to camelCase
pub fn transform_jsonb_field(field_str: &str) -> Result<Value, serde_json::Error> {
    let value: Value = serde_json::from_str(field_str)?;
    Ok(transform_row_keys(&value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snake_to_camel() {
        assert_eq!(to_camel_case("user_id"), "userId");
        assert_eq!(to_camel_case("first_name"), "firstName");
        assert_eq!(to_camel_case("simple"), "simple");
        assert_eq!(to_camel_case("_private"), "_private");
    }

    #[test]
    fn test_transform_keys() {
        let row = json!({
            "user_id": 123,
            "first_name": "John",
            "nested_object": {
                "user_email": "john@example.com"
            }
        });

        let transformed = transform_row_keys(&row);
        assert_eq!(transformed["userId"], 123);
        assert_eq!(transformed["firstName"], "John");
        assert_eq!(transformed["nestedObject"]["userEmail"], "john@example.com");
    }
}
```

### Step 3: Create Streaming Query Executor

**File**: `fraiseql_rs/src/db/query_stream.rs` (NEW)

```rust
//! Streaming query execution without buffering entire result set.

use tokio_postgres::Client;
use serde_json::{json, Value};
use futures::stream::Stream;
use super::types::DatabaseError;

/// Execute query and return stream of rows
pub async fn stream_query_rows(
    client: &Client,
    sql: &str,
    params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
) -> Result<impl Stream<Item = Result<Value, DatabaseError>>, DatabaseError> {
    // Use portal-based cursor for streaming
    let statement = client.prepare(sql)
        .await
        .map_err(|e| DatabaseError::QueryError(format!("Prepare failed: {}", e)))?;

    // Create named portal for server-side cursor
    let portal_name = format!("portal_{}", uuid::Uuid::new_v4());

    // NOTE: Implementation requires futures::stream or tokio::sync::mpsc
    // This is a simplified outline

    todo!("Implement streaming via tokio-postgres portal API")
}

/// Row-by-row streaming with memory efficiency
pub struct RowStream {
    portal: String,
    chunk_size: usize,
    exhausted: bool,
}

impl RowStream {
    pub fn new(portal: String) -> Self {
        RowStream {
            portal,
            chunk_size: 1000,  // Fetch 1000 rows at a time
            exhausted: false,
        }
    }

    /// Get next batch of rows without loading all into memory
    pub async fn next_batch(&mut self, client: &Client) -> Result<Vec<Value>, DatabaseError> {
        if self.exhausted {
            return Ok(Vec::new());
        }

        // FETCH FORWARD chunk_size FROM portal
        let fetch_sql = format!("FETCH FORWARD {} FROM {}", self.chunk_size, self.portal);
        let rows = client.query(&fetch_sql, &[])
            .await
            .map_err(|e| DatabaseError::QueryError(format!("Fetch failed: {}", e)))?;

        if rows.len() < self.chunk_size {
            self.exhausted = true;
        }

        // Convert rows to JSON
        let mut json_rows = Vec::new();
        for row in rows {
            let mut obj = serde_json::Map::new();
            for (idx, column) in row.columns().iter().enumerate() {
                let col_name = column.name().to_string();
                let value = convert_row_value(&row, idx)?;
                obj.insert(col_name, value);
            }
            json_rows.push(Value::Object(obj));
        }

        Ok(json_rows)
    }
}

fn convert_row_value(row: &tokio_postgres::Row, idx: usize) -> Result<Value, DatabaseError> {
    let col = row.columns().get(idx).ok_or(DatabaseError::QueryError("Invalid column".to_string()))?;

    match col.type_().oid() {
        25 => Ok(Value::String(row.get(idx))),  // text
        23 => Ok(Value::Number(row.get::<_, i32>(idx).into())),  // int4
        20 => Ok(Value::Number(row.get::<_, i64>(idx).into())),  // int8
        114 | 3802 => {  // json, jsonb
            let json_str: String = row.get(idx);
            serde_json::from_str(&json_str)
                .map_err(|e| DatabaseError::QueryError(format!("JSON parse error: {}", e)))
        }
        _ => Ok(Value::String(row.try_get::<_, String>(idx).unwrap_or_default())),
    }
}
```

### Step 4: Integration with Python

**File**: `src/fraiseql/core/rust_pipeline.py` (MODIFY)

```python
"""Integrate Rust streaming backend with Python GraphQL layer."""

import asyncio
from typing import AsyncIterator
from fraiseql._fraiseql_rs import execute_query_streaming

async def execute_graphql_query_streaming(
    query_def: dict,
) -> AsyncIterator[bytes]:
    """Execute GraphQL query with streaming results.

    Yields: Chunks of JSON bytes as they're ready
    """
    async for chunk in execute_query_streaming(query_def):
        yield chunk
```

---

## Verification

### Benchmarks

```bash
# Memory usage comparison (Phase 2 vs Phase 3)
cargo run --release --example memory_benchmark

# Throughput comparison
cargo bench --bench pipeline

# Large result set test (10K+ rows)
cargo test --release test_streaming_large_results
```

### Tests

```bash
# Streaming tests
cargo test -p fraiseql_rs --lib response::streaming

# JSON transformation tests
cargo test -p fraiseql_rs --lib response::json_transform

# Integration tests
uv run pytest tests/integration/streaming/ -v
```

### Performance Validation

```bash
# Measure memory reduction
/usr/bin/time -v cargo test --release 2>&1 | grep "Maximum resident set size"

# Measure latency improvement
ab -n 1000 -c 10 http://localhost:8000/graphql
```

---

## Success Metrics

- [ ] Memory usage 50% lower for large result sets (1000+ rows)
- [ ] Response time 15-25% faster
- [ ] All 5991+ tests passing
- [ ] No regressions in JSONB handling
- [ ] Streaming handles 100K+ row result sets without memory spike

---

## Next Phase

👉 Proceed to **Phase 4: Full Integration** after verification

---

**Status**: ✅ Ready for Phase 2 completion
**Duration**: 10 hours
**Branch**: `feature/rust-postgres-driver`

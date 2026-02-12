# Phase 8: JSON to Arrow Conversion for Historical Events

## Objective
Convert `HistoricalEvent` JSON data to Apache Arrow format for efficient columnar storage and processing.

## Success Criteria
- [ ] `HistoricalEvent` instances convert to Arrow RecordBatches
- [ ] All event fields properly mapped (id, event_type, entity_type, entity_id, data, user_id, tenant_id, timestamp)
- [ ] JSON data field converts to nested Arrow struct
- [ ] Schema inference from historical events
- [ ] Batch conversion with configurable batch size
- [ ] `cargo clippy -p fraiseql-arrow` clean
- [ ] `cargo test -p fraiseql-arrow` passes

## TDD Cycles

### Cycle 1: Define Arrow Schema for Historical Events

**File**: `crates/fraiseql-arrow/src/event_schema.rs` (new file)

- **RED**: Write test expecting Arrow schema for events
- **GREEN**: Define schema:
  ```rust
  pub fn historical_event_schema() -> Schema {
      Schema::new(vec![
          Field::new("id", DataType::Utf8, false),
          Field::new("event_type", DataType::Utf8, false),
          Field::new("entity_type", DataType::Utf8, false),
          Field::new("entity_id", DataType::Utf8, false),
          Field::new("data", DataType::Utf8, true),  // JSON as string for now
          Field::new("user_id", DataType::Utf8, true),
          Field::new("tenant_id", DataType::Utf8, true),
          Field::new("timestamp", DataType::Timestamp(TimeUnit::Microsecond, None), false),
      ])
  }
  ```
- **REFACTOR**: Consider nested struct for complex data fields
- **CLEANUP**: Verify schema compatibility, commit

### Cycle 2: Implement Event to Arrow Conversion

**File**: `crates/fraiseql-arrow/src/event_schema.rs`

- **RED**: Write test converting HistoricalEvent to Arrow
- **GREEN**: Implement conversion:
  ```rust
  pub fn event_to_arrow(event: &HistoricalEvent) -> Result<RecordBatch> {
      let schema = historical_event_schema();
      // Build Arrow arrays from event fields
      // Return RecordBatch
  }
  ```
- **REFACTOR**: Handle batch conversion for multiple events
- **CLEANUP**: Test null/empty values, commit

### Cycle 3: Test Arrow Conversion

**File**: `crates/fraiseql-arrow/tests/event_conversion.rs`

- **RED**: Write comprehensive test matrix
- **GREEN**: Verify round-trip fidelity
- **REFACTOR**: Add performance tests (large batches)
- **CLEANUP**: All tests pass, commit

## Dependencies
- None (independent of all other phases)

## Status
[ ] Not Started

# Phase 4: JSON to Arrow Conversion for Historical Events

## Objective
Convert historical events from JSON bytes to Arrow IPC `RecordBatch` format in
the Flight server's `do_get` response path.

## Success Criteria
- [ ] Historical events returned as Arrow `RecordBatch` in IPC format
- [ ] Schema matches `entity_event_arrow_schema()` (8 columns)
- [ ] Type conversions correct: UUID→Utf8, DateTime→Timestamp, Value→JSON string
- [ ] Empty event lists produce a schema-only response (0 rows)
- [ ] `cargo clippy -p fraiseql-arrow` clean
- [ ] `cargo test -p fraiseql-arrow` passes

## Background

### Current State

**File:** `crates/fraiseql-arrow/src/flight_server.rs:908-917`

Historical events are currently serialized as plain JSON in `FlightData.data_body`
with `app_metadata: "application/json"`:
```rust
let json_data = serde_json::json!(events);
let json_str = json_data.to_string();
let flight_data = FlightData {
    data_body: json_str.into_bytes().into(),
    app_metadata: b"application/json".to_vec().into(),
    ..Default::default()
};
```

### Arrow Schema (8 columns)

**File:** `crates/fraiseql-arrow/src/event_schema.rs:48-63`

```rust
entity_event_arrow_schema() → Schema:
  event_id    : Utf8                            (NOT NULL)
  event_type  : Utf8                            (NOT NULL)
  entity_type : Utf8                            (NOT NULL)
  entity_id   : Utf8                            (NOT NULL)
  timestamp   : Timestamp(Microsecond, "UTC")   (NOT NULL)
  data        : Utf8                            (NOT NULL)  — JSON as string
  user_id     : Utf8                            (NULLABLE)
  tenant_id   : Utf8                            (NULLABLE)
```

### HistoricalEvent Struct (8 fields)

**File:** `crates/fraiseql-arrow/src/event_storage.rs:16-34`

```rust
pub struct HistoricalEvent {
    pub id:          Uuid,               // → event_id (Utf8 via .to_string())
    pub event_type:  String,             // → event_type (Utf8, direct)
    pub entity_type: String,             // → entity_type (Utf8, direct)
    pub entity_id:   Uuid,               // → entity_id (Utf8 via .to_string())
    pub data:        serde_json::Value,  // → data (Utf8 via serde_json::to_string())
    pub user_id:     Option<String>,     // → user_id (Utf8, nullable)
    pub tenant_id:   Option<String>,     // → tenant_id (Utf8, nullable)
    pub timestamp:   DateTime<Utc>,      // → timestamp (Microsecond via .timestamp_micros())
}
```

### Type Conversions Required

| Struct Field | Rust Type | Arrow Type | Conversion |
|-------------|-----------|------------|------------|
| `id` | `Uuid` | `Utf8` | `.to_string()` |
| `event_type` | `String` | `Utf8` | direct |
| `entity_type` | `String` | `Utf8` | direct |
| `entity_id` | `Uuid` | `Utf8` | `.to_string()` |
| `timestamp` | `DateTime<Utc>` | `Timestamp(Microsecond)` | `.timestamp_micros()` |
| `data` | `serde_json::Value` | `Utf8` | `serde_json::to_string()` |
| `user_id` | `Option<String>` | `Utf8` (nullable) | `.as_deref()` |
| `tenant_id` | `Option<String>` | `Utf8` (nullable) | `.as_deref()` |

### Existing Infrastructure

- `record_batch_to_flight_data()` (flight_server.rs:~2437) — encodes `RecordBatch` to `FlightData`
- `schema_to_flight_data()` (flight_server.rs:~2467) — encodes schema to initial `FlightData`
- `RowToArrowConverter` (convert.rs) — generic row-to-batch converter (could reuse, but direct
  Arrow builder arrays are simpler for a fixed schema)

## TDD Cycles

### Cycle 1: `events_to_record_batch()` Conversion Function

**File:** `crates/fraiseql-arrow/src/event_schema.rs` (add to existing file)

- **RED**: Write conversion test:
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use chrono::Utc;
      use uuid::Uuid;

      #[test]
      fn test_events_to_record_batch() {
          let events = vec![
              HistoricalEvent {
                  id:          Uuid::new_v4(),
                  event_type:  "INSERT".to_string(),
                  entity_type: "Order".to_string(),
                  entity_id:   Uuid::new_v4(),
                  data:        serde_json::json!({"total": 99.99}),
                  user_id:     Some("user-1".to_string()),
                  tenant_id:   None,
                  timestamp:   Utc::now(),
              },
              HistoricalEvent {
                  id:          Uuid::new_v4(),
                  event_type:  "UPDATE".to_string(),
                  entity_type: "Order".to_string(),
                  entity_id:   Uuid::new_v4(),
                  data:        serde_json::json!({"total": 150.0}),
                  user_id:     None,
                  tenant_id:   Some("tenant-a".to_string()),
                  timestamp:   Utc::now(),
              },
          ];

          let batch = events_to_record_batch(&events).unwrap();
          assert_eq!(batch.num_rows(), 2);
          assert_eq!(batch.num_columns(), 8);
          assert_eq!(batch.schema(), entity_event_arrow_schema());
      }

      #[test]
      fn test_events_to_record_batch_empty() {
          let batch = events_to_record_batch(&[]).unwrap();
          assert_eq!(batch.num_rows(), 0);
          assert_eq!(batch.num_columns(), 8);
      }

      #[test]
      fn test_events_nullable_fields() {
          let event = HistoricalEvent {
              id:          Uuid::new_v4(),
              event_type:  "INSERT".to_string(),
              entity_type: "User".to_string(),
              entity_id:   Uuid::new_v4(),
              data:        serde_json::json!({}),
              user_id:     None,
              tenant_id:   None,
              timestamp:   Utc::now(),
          };
          let batch = events_to_record_batch(&[event]).unwrap();
          // user_id and tenant_id columns should have nulls
          assert!(batch.column(6).is_null(0));  // user_id
          assert!(batch.column(7).is_null(0));  // tenant_id
      }
  }
  ```

- **GREEN**: Implement using Arrow builder arrays:
  ```rust
  use arrow::array::{StringArray, TimestampMicrosecondArray};
  use arrow::record_batch::RecordBatch;

  pub fn events_to_record_batch(
      events: &[HistoricalEvent],
  ) -> Result<RecordBatch, arrow::error::ArrowError> {
      let schema = entity_event_arrow_schema();

      let event_ids: StringArray = events.iter()
          .map(|e| Some(e.id.to_string()))
          .collect();
      let event_types: StringArray = events.iter()
          .map(|e| Some(e.event_type.as_str()))
          .collect();
      let entity_types: StringArray = events.iter()
          .map(|e| Some(e.entity_type.as_str()))
          .collect();
      let entity_ids: StringArray = events.iter()
          .map(|e| Some(e.entity_id.to_string()))
          .collect();
      let timestamps: TimestampMicrosecondArray = events.iter()
          .map(|e| Some(e.timestamp.timestamp_micros()))
          .collect::<TimestampMicrosecondArray>()
          .with_timezone("UTC");
      let data: StringArray = events.iter()
          .map(|e| Some(serde_json::to_string(&e.data).unwrap_or_default()))
          .collect();
      let user_ids: StringArray = events.iter()
          .map(|e| e.user_id.as_deref())
          .collect();
      let tenant_ids: StringArray = events.iter()
          .map(|e| e.tenant_id.as_deref())
          .collect();

      RecordBatch::try_new(schema, vec![
          Arc::new(event_ids),
          Arc::new(event_types),
          Arc::new(entity_types),
          Arc::new(entity_ids),
          Arc::new(timestamps),
          Arc::new(data),
          Arc::new(user_ids),
          Arc::new(tenant_ids),
      ])
  }
  ```

- **REFACTOR**: Verify column order matches the schema field order exactly
- **CLEANUP**: Clippy, test, commit

---

### Cycle 2: Integrate into Flight Server `do_get`

**File:** `crates/fraiseql-arrow/src/flight_server.rs`

- **RED**: Write an integration test that calls `do_get` for historical events
  and decodes the response as Arrow IPC (not JSON). Verify the `FlightData`
  stream starts with a schema message followed by batch data.

- **GREEN**: At line ~908, replace the JSON serialization:
  ```rust
  // Convert events to Arrow RecordBatch
  let batch = events_to_record_batch(&events)?;
  let schema = entity_event_arrow_schema();

  // Stream: schema first, then batch data
  let schema_data = schema_to_flight_data(&schema)?;
  messages.push(Ok(schema_data));

  if batch.num_rows() > 0 {
      let batch_data = record_batch_to_flight_data(&batch)?;
      messages.push(Ok(batch_data));
  }
  ```

- **REFACTOR**: Handle the empty events case gracefully (schema-only response
  with 0 rows is valid Arrow IPC)

- **CLEANUP**: Clippy, full test suite, commit

## Dependencies
- None (independent of Phase 5)

## Status
[ ] Not Started

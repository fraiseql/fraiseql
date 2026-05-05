---
title: Test Extraction — fraiseql-wire
status: planned
---

# Phase 35: `fraiseql-wire`

## Objective

Extract inline tests from all 28 files in `fraiseql-wire`.

## Files by subsystem

### stream/ (6 files)

| File |
|------|
| `stream/typed_stream.rs` |
| `stream/json_stream.rs` |
| `stream/memory_estimator.rs` |
| `stream/filter.rs` |
| `stream/chunking.rs` |
| `stream/adaptive_chunking.rs` |

→ `stream/tests.rs`

### metrics/ (5 files)

| File |
|------|
| `metrics/mod.rs` |
| `metrics/labels.rs` |
| `metrics/histograms.rs` |
| `metrics/gauges.rs` |
| `metrics/counters.rs` |

→ `metrics/tests.rs`

### operators/ (4 files)

| File |
|------|
| `operators/where_operator.rs` |
| `operators/sql_gen.rs` |
| `operators/order_by.rs` |
| `operators/field.rs` |

→ `operators/tests.rs`

### connection/ (3 files)

| File |
|------|
| `connection/transport.rs` |
| `connection/tls.rs` |
| `connection/state.rs` |

> `connection/conn/tests.rs` already exists — skip that subdirectory.

→ `connection/tests.rs`

### protocol/ (2 files)

| File |
|------|
| `protocol/encode.rs` |
| `protocol/decode.rs` |

→ `protocol/tests.rs`

### client/ (2 files)

| File |
|------|
| `client/query_builder.rs` |
| `client/connection_string.rs` |

→ `client/tests.rs`

### util/ (2 files)

| File |
|------|
| `util/oid.rs` |
| `util/bytes.rs` |

→ `util/tests.rs`

### auth/ (1 file)

| File |
|------|
| `auth/scram.rs` |

→ `auth/tests.rs`

### json/ (1 file)

| File |
|------|
| `json/validate.rs` |

→ `json/tests.rs`

### error.rs (1 file, top-level)

→ `src/tests.rs` with declaration in `lib.rs`

## Commit

```
refactor(wire): extract inline tests to tests.rs
```

## Verification

```bash
cargo clippy -p fraiseql-wire --all-targets --all-features -- -D warnings
cargo nextest run -p fraiseql-wire --lib
```

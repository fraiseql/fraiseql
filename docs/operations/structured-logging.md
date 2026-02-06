<!-- Skip to main content -->
---

title: Structured JSON Logging in FraiseQL v2
description: FraiseQL v2 provides a comprehensive structured logging system that outputs all logs in JSON format, making them easy to parse, aggregate, and analyze with log
keywords: ["deployment", "scaling", "performance", "monitoring", "troubleshooting"]
tags: ["documentation", "reference"]
---

# Structured JSON Logging in FraiseQL v2

## Overview

FraiseQL v2 provides a comprehensive structured logging system that outputs all logs in JSON format, making them easy to parse, aggregate, and analyze with log aggregation systems like ELK, Splunk, DataDog, New Relic, and others.

## Key Features

- **JSON Output**: Every log entry is a valid JSON object with consistent structure
- **Request Context**: Automatic tracking of request IDs, operations, and user information
- **Performance Metrics**: Built-in capture of duration, complexity, and cache hit information
- **Error Details**: Structured error information with type, message, and optional stack traces
- **Request Correlation**: Trace individual requests through the entire stack
- **Source Location**: Track which file and line generated each log entry

## Architecture

### Core Components

#### RequestId

Unique identifier for each request, automatically generated using UUID v4.

```rust
<!-- Code example in RUST -->
use fraiseql_server::RequestId;

let request_id = RequestId::new();
println!("{}", request_id); // e.g., "550e8400-e29b-41d4-a716-446655440000"
```text
<!-- Code example in TEXT -->

#### RequestContext

Tracks request-level information for correlation and analysis.

```rust
<!-- Code example in RUST -->
use fraiseql_server::RequestContext;

let context = RequestContext::new()
    .with_operation("GetUsers".to_string())
    .with_user_id("user123".to_string())
    .with_client_ip("192.168.1.1".to_string())
    .with_api_version("v1".to_string());
```text
<!-- Code example in TEXT -->

#### StructuredLogEntry

The main log entry structure containing message, context, metrics, and error information.

```rust
<!-- Code example in RUST -->
use fraiseql_server::{StructuredLogEntry, LogLevel, LogMetrics, RequestContext};

let entry = StructuredLogEntry::new(LogLevel::Info, "Query executed successfully".to_string())
    .with_request_context(context)
    .with_metrics(LogMetrics::new().with_duration_ms(42.5));

println!("{}", entry.to_json_string());
```text
<!-- Code example in TEXT -->

#### RequestLogger

Convenience wrapper for contextual logging within a request scope.

```rust
<!-- Code example in RUST -->
use fraiseql_server::RequestLogger;

let logger = RequestLogger::new(context);
let entry = logger.info("Processing GraphQL query");
println!("{}", entry.to_json_string());
```text
<!-- Code example in TEXT -->

## Usage Examples

### Basic Logging

```rust
<!-- Code example in RUST -->
use fraiseql_server::{StructuredLogEntry, LogLevel};

// Create a log entry
let entry = StructuredLogEntry::new(
    LogLevel::Info,
    "Server started successfully".to_string()
);

// Print as JSON
println!("{}", entry.to_json_string());
```text
<!-- Code example in TEXT -->

**Output:**

```json
<!-- Code example in JSON -->
{
  "timestamp": "2024-01-16T15:30:45.123Z",
  "level": "INFO",
  "message": "Server started successfully",
  "request_context": null,
  "metrics": null,
  "error": null,
  "source": null,
  "context": null
}
```text
<!-- Code example in TEXT -->

### Request Context Logging

```rust
<!-- Code example in RUST -->
use fraiseql_server::{StructuredLogEntry, LogLevel, RequestContext};

let context = RequestContext::new()
    .with_operation("GetUser".to_string())
    .with_user_id("user_42".to_string())
    .with_client_ip("203.0.113.42".to_string());

let entry = StructuredLogEntry::new(
    LogLevel::Info,
    "GraphQL query executed".to_string()
)
.with_request_context(context);

println!("{}", entry.to_json_string());
```text
<!-- Code example in TEXT -->

**Output:**

```json
<!-- Code example in JSON -->
{
  "timestamp": "2024-01-16T15:30:45.456Z",
  "level": "INFO",
  "message": "GraphQL query executed",
  "request_context": {
    "request_id": "550e8400-e29b-41d4-a716-446655440000",
    "operation": "GetUser",
    "user_id": "user_42",
    "client_ip": "203.0.113.42",
    "api_version": null
  },
  "metrics": null,
  "error": null,
  "source": null,
  "context": null
}
```text
<!-- Code example in TEXT -->

### Performance Metrics Logging

```rust
<!-- Code example in RUST -->
use fraiseql_server::{StructuredLogEntry, LogLevel, LogMetrics};

let metrics = LogMetrics::new()
    .with_duration_ms(123.45)
    .with_db_queries(3)
    .with_complexity(5)
    .with_cache_hit(true);

let entry = StructuredLogEntry::new(
    LogLevel::Info,
    "Query completed".to_string()
)
.with_metrics(metrics);

println!("{}", entry.to_json_string());
```text
<!-- Code example in TEXT -->

**Output:**

```json
<!-- Code example in JSON -->
{
  "timestamp": "2024-01-16T15:30:45.789Z",
  "level": "INFO",
  "message": "Query completed",
  "metrics": {
    "duration_ms": 123.45,
    "complexity": 5,
    "items_processed": null,
    "cache_hit": true,
    "db_queries": 3
  }
}
```text
<!-- Code example in TEXT -->

### Error Logging

```rust
<!-- Code example in RUST -->
use fraiseql_server::{StructuredLogEntry, LogLevel, ErrorDetails};

let error = ErrorDetails::new(
    "DatabaseError".to_string(),
    "Connection timeout after 30s".to_string()
)
.with_code("DB_TIMEOUT".to_string());

let entry = StructuredLogEntry::new(
    LogLevel::Error,
    "Database query failed".to_string()
)
.with_error(error);

println!("{}", entry.to_json_string());
```text
<!-- Code example in TEXT -->

**Output:**

```json
<!-- Code example in JSON -->
{
  "timestamp": "2024-01-16T15:30:46.123Z",
  "level": "ERROR",
  "message": "Database query failed",
  "error": {
    "error_type": "DatabaseError",
    "message": "Connection timeout after 30s",
    "code": "DB_TIMEOUT",
    "stack_trace": null
  }
}
```text
<!-- Code example in TEXT -->

### Request Logger (Convenience API)

```rust
<!-- Code example in RUST -->
use fraiseql_server::{RequestLogger, RequestContext};

let context = RequestContext::new()
    .with_operation("UpdateUser".to_string())
    .with_user_id("user_42".to_string());

let logger = RequestLogger::new(context);

// Log with implicit request context
let entry = logger.info("User update initiated");
println!("{}", entry.to_json_string());
```text
<!-- Code example in TEXT -->

## Integration with Log Aggregation Systems

### ELK Stack (Elasticsearch, Logstash, Kibana)

Configure Logstash to parse JSON logs:

```conf
<!-- Code example in CONF -->
input {
  stdin { }
}

filter {
  json {
    source => "message"
  }
}

output {
  elasticsearch {
    hosts => ["localhost:9200"]
    index => "FraiseQL-%{+YYYY.MM.dd}"
  }
}
```text
<!-- Code example in TEXT -->

### Splunk

Enable HTTP Event Collector (HEC) and configure application to send JSON logs:

```bash
<!-- Code example in BASH -->
curl -k https://splunk-host:8088/services/collector \
  -H "Authorization: Splunk your-hec-token" \
  -d '{"sourcetype": "json", "event": {"level": "INFO", ...}}'
```text
<!-- Code example in TEXT -->

### DataDog

Configure tracing to send structured logs:

```rust
<!-- Code example in RUST -->
// Using datadog-statsd crate
let statsd = statsd::Client::new("127.0.0.1:8125", "FraiseQL")
    .expect("Failed to create statsd client");

// Logs are automatically picked up by DataDog agent
```text
<!-- Code example in TEXT -->

### CloudWatch (AWS)

Send JSON logs to CloudWatch:

```rust
<!-- Code example in RUST -->
// Using rusoto_logs crate
let logs_client = CloudWatchLogsClient::new(Region::UsEast1);

let put_log_events_request = PutLogEventsRequest {
    log_group_name: "/aws/FraiseQL/server".to_string(),
    log_stream_name: "main".to_string(),
    log_events: vec![
        InputLogEvent {
            message: json_log_entry.to_json_string(),
            timestamp: Some(Utc::now().timestamp_millis()),
        }
    ],
    ..Default::default()
};
```text
<!-- Code example in TEXT -->

## Log Levels and Severity

FraiseQL uses standard log levels:

| Level | Usage | Example |
|-------|-------|---------|
| `TRACE` | Extremely detailed debugging | Individual field parsing, cache lookups |
| `DEBUG` | Detailed debugging information | Request middleware logging, schema validation |
| `INFO` | General informational messages | Query execution, connection pool status |
| `WARN` | Warning conditions that should be reviewed | Deprecated API usage, cache invalidation |
| `ERROR` | Error conditions requiring attention | Query failures, database errors |

## Field Reference

### StructuredLogEntry Fields

| Field | Type | Description |
|-------|------|-------------|
| `timestamp` | String (ISO 8601) | When the log entry was created |
| `level` | String | Log severity level |
| `message` | String | Human-readable log message |
| `request_context` | RequestContext | Request correlation and context |
| `metrics` | LogMetrics | Performance and operational metrics |
| `error` | ErrorDetails | Error information (if applicable) |
| `source` | SourceLocation | Source code location |
| `context` | JSON Object | Custom context fields |

### RequestContext Fields

| Field | Type | Description |
|-------|------|-------------|
| `request_id` | UUID | Unique request identifier |
| `operation` | String | GraphQL operation name |
| `user_id` | String | Authenticated user identifier |
| `client_ip` | String | Client IP address |
| `api_version` | String | API version used |

### LogMetrics Fields

| Field | Type | Description |
|-------|------|-------------|
| `duration_ms` | Number | Operation duration in milliseconds |
| `complexity` | Integer | Query complexity score |
| `items_processed` | Integer | Number of items processed |
| `cache_hit` | Boolean | Whether result came from cache |
| `db_queries` | Integer | Number of database queries executed |

## Performance Considerations

### Log Output Performance

- JSON serialization: ~1-2 microseconds per entry
- Lock-free logging design: No contention on high throughput
- Async logging recommended for high-volume scenarios

### Storage and Transmission

Estimate log volume:

- Average JSON log entry: ~400-600 bytes
- 1,000 requests/second: ~400-600 MB/hour
- Use compression (gzip) for storage and transmission

### Best Practices

1. **Set Appropriate Log Levels**: Use DEBUG/TRACE only for development, INFO for production
2. **Include Request Context**: Always correlate logs to requests for easier debugging
3. **Monitor Log Volume**: Watch for log explosion scenarios
4. **Use Log Aggregation**: Never rely on local log files in production
5. **Sanitize Sensitive Data**: Remove PII before logging

## Testing

All logging components are fully tested:

```bash
<!-- Code example in BASH -->
# Run logging tests
cargo test -p FraiseQL-server --lib logging

# Run middleware tests
cargo test -p FraiseQL-server --lib middleware::logging
```text
<!-- Code example in TEXT -->

## Future Enhancements

- Structured tracing integration with `tracing-subscriber`
- Sampling strategies for high-volume scenarios
- Metrics export to Prometheus from logs
- Automatic PII redaction
- Custom field injection via middleware

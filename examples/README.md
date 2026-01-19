# FraiseQL Examples

This directory contains example projects demonstrating FraiseQL usage patterns and best practices.

## Quick Start Examples

### 1. Basic Query Example

The most basic example showing how to:

- Load a compiled schema
- Create an executor
- Execute a simple GraphQL query

```bash
cd examples/basic-query
cargo run
```

**What it demonstrates:**

- Schema loading from JSON
- Creating an Executor
- Executing a basic `{ users { id name } }` query
- Parsing results

### 2. Subscription Example

Real-time subscription support with WebSocket:

```bash
cd examples/subscriptions
cargo run
```

**What it demonstrates:**

- Setting up WebSocket connection
- Subscribing to real-time events
- Handling subscription messages
- Disconnecting gracefully

### 3. Error Handling Example

Comprehensive error handling patterns:

```bash
cd examples/error-handling
cargo run
```

**What it demonstrates:**

- Handling query validation errors
- Database connection errors
- Timeout errors
- Custom error messages
- Error recovery patterns

### 4. Performance Example

Measuring and optimizing query performance:

```bash
cd examples/performance
cargo run
```

**What it demonstrates:**

- Query tracing and timing
- Result caching
- Batch query execution
- Performance monitoring
- Identifying bottlenecks

### 5. Authentication Example

Implementing JWT authentication:

```bash
cd examples/authentication
cargo run
```

**What it demonstrates:**

- OIDC configuration
- JWT validation
- User context extraction
- Field-level authorization
- Protected queries

### 6. Complex Queries Example

Advanced query patterns:

```bash
cd examples/complex-queries
cargo run
```

**What it demonstrates:**

- Nested field selection
- Variable binding
- Aggregations
- Window functions
- Complex filtering

## Project Structure

Each example follows this structure:

```
examples/
├── basic-query/           # Simple query execution
│   ├── Cargo.toml
│   ├── src/
│   │   └── main.rs
│   └── schema.compiled.json
├── subscriptions/         # Real-time subscriptions
├── error-handling/        # Error patterns
├── performance/           # Performance optimization
├── authentication/        # JWT and OIDC
├── complex-queries/       # Advanced queries
└── README.md              # This file
```

## Running Examples

### Prerequisites

```bash
# PostgreSQL running
docker run -d \
  -e POSTGRES_PASSWORD=password \
  -e POSTGRES_DB=fraiseql_examples \
  -p 5432:5432 \
  postgres:15

# Set database URL
export DATABASE_URL="postgresql://postgres:password@localhost:5432/fraiseql_examples"
```

### Running Any Example

```bash
cd examples/example-name
cargo run

# Or with logging
RUST_LOG=debug cargo run

# Or with specific database
DATABASE_URL="postgresql://..." cargo run
```

## Common Patterns

### Error Handling

```rust
use fraiseql_core::error::FraiseQLError;

fn handle_error(err: &FraiseQLError) {
    match err {
        FraiseQLError::Parse { message, location } => {
            eprintln!("Parse error: {} at {:?}", message, location);
        }
        FraiseQLError::Validation { message, path } => {
            eprintln!("Validation error: {} at {:?}", message, path);
        }
        FraiseQLError::Database { message, code } => {
            eprintln!("Database error: {} ({})", message, code.unwrap_or("unknown"));
        }
        FraiseQLError::Timeout => {
            eprintln!("Query timeout - increase timeout or optimize query");
        }
    }
}
```

### Performance Monitoring

```rust
use fraiseql_core::runtime::query_tracing::QueryTraceBuilder;

fn monitor_query(query: &str) {
    let mut trace = QueryTraceBuilder::new("query_123", query);

    // Execute query
    let phase_start = std::time::Instant::now();
    execute_query();
    trace.record_phase_success("execute", phase_start.elapsed().as_micros() as u64);

    // Get metrics
    let finished = trace.finish(true, None, Some(100)).unwrap();
    println!("Query took {} μs", finished.total_duration_us);
    println!("Slowest phase: {:?}", finished.slowest_phase());
}
```

### SQL Logging

```rust
use fraiseql_core::runtime::sql_logger::SqlQueryLogBuilder;

fn log_sql_query(query: &str) {
    let builder = SqlQueryLogBuilder::new("query_123", query, 2)
        .with_slow_threshold(10_000); // 10ms slow threshold

    let log = builder.finish_success(Some(100));
    println!("{}", log.to_log_string());
}
```

### Rate Limiting

```rust
use fraiseql_server::middleware::{RateLimiter, RateLimitConfig};

async fn check_rate_limit() {
    let config = RateLimitConfig {
        enabled: true,
        rps_per_ip: 100,
        rps_per_user: 1000,
        burst_size: 500,
        cleanup_interval_secs: 300,
    };

    let limiter = RateLimiter::new(config);
    
    // Check IP rate limit
    if limiter.check_ip_limit("192.168.1.1").await {
        println!("Request allowed");
    } else {
        println!("Rate limit exceeded");
    }
}
```

## Next Steps

- Review [Architecture documentation](../docs/architecture/)
- Read [Developer Guide](../docs/DEVELOPER_GUIDE.md)
- Check [Profiling Guide](../docs/PROFILING_GUIDE.md)
- Check [Linting Guide](../docs/LINTING.md)

## Contributing

Have a useful example pattern? Submit a PR!

Requirements for example PRs:

- Working example code
- Clear documentation
- Follows project style guide
- Includes error handling
- Has tests if applicable

## Questions?

- Check example source code comments
- Review relevant documentation
- Open an issue with your question
- Ask in project Slack channel

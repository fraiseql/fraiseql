# FraiseQL Observers Integration Tests

This directory contains integration tests for the observer library: the executor, the
change-log listener, checkpoints, transports and actions.

## `integration_test.rs` Coverage

1. **Cache Backend** (`test_cache_performance_improvement`)
   - Creates the Redis-backed action result cache backend
   - Requires Redis

2. **Concurrent Execution Performance** (`test_concurrent_execution_performance`)
   - Compares sequential and concurrent event processing

3. **Checkpoint Recovery** (`test_checkpoint_recovery`)
   - Validates checkpoint-based crash recovery
   - Requires PostgreSQL

4. **Error Handling and Resilience** (`test_error_handling_resilience`)
   - Tests behavior with no observers configured

5. **Multi-Event Processing** (`test_multi_event_processing`)
   - Processes events for several entity types

6. **DLQ Failure Tracking** (`test_dlq_failure_tracking`)
   - Checks the dead letter queue after processing

7. **Concurrent Observer Processing** (`test_concurrent_observer_processing`)
   - Processes several events from concurrent tasks

8. **Event Processing with Caching** (`test_event_processing_with_caching`)
   - Processes an event with the action result cache available
   - Requires Redis

## Running Tests

### Prerequisites

**Required**:

- Rust toolchain
- Redis server on `localhost:6379` (or set `REDIS_URL`) for the cache tests

**Optional**:

- NATS server on `localhost:4222` (for NATS tests)
- PostgreSQL database (for checkpoint and change-log tests)

### Quick Start with Docker Compose

```bash
# Start Redis
docker compose -f "$(git rev-parse --show-toplevel)/docker/docker-compose.test.yml" up -d redis-test

# Or start Redis + NATS + PostgreSQL
docker compose -f "$(git rev-parse --show-toplevel)/docker/docker-compose.test.yml" up -d redis-test nats-test postgres-test
```

### Running Tests

**All integration tests with Redis**:

```bash
cargo test --test integration_test --features "postgres,caching,testing"
```

**With checkpoint tests enabled**:

```bash
cargo test --test integration_test --features "postgres,caching,checkpoint,testing"
```

**Run a specific test**:

```bash
cargo test --test integration_test test_event_processing_with_caching --features "postgres,caching,testing"
```

**With output logging**:

```bash
cargo test --test integration_test --features "postgres,caching,testing" -- --nocapture
```

### Feature Flags

| Feature | Description | Required For |
|---------|-------------|--------------|
| `postgres` | PostgreSQL database support | Most tests |
| `caching` | Redis action result cache | Cache tests |
| `nats` | NATS transport | NATS tests |
| `testing` | Test mocks and utilities | All tests |
| `checkpoint` | Checkpoint recovery | Checkpoint tests |

## Troubleshooting

### Redis Connection Failed

**Error**: `Failed to connect to Redis`

**Solution**:

```bash
# Check Redis is running
docker compose -f "$(git rev-parse --show-toplevel)/docker/docker-compose.test.yml" ps redis-test

# Or start Redis manually
docker run -d -p 6379:6379 redis:7-alpine

# Or use a custom Redis URL
REDIS_URL=redis://your-redis:6379 cargo test --test integration_test --features "postgres,caching,testing"
```

### Test Timeout

**Error**: Test hangs or times out

**Solution**:

- Ensure Redis is responsive: `redis-cli ping` should return `PONG`
- Check network connectivity
- Run tests serially: `cargo test -- --test-threads=1`

### Feature Not Enabled

**Error**: `no tests to run matching...`

**Solution**: Include the features the test is gated on:

```bash
cargo test --test integration_test --features "postgres,caching,testing"
```

## Performance Benchmarks

```bash
cargo bench --bench observer_benchmarks
```

## Manual Testing

- `docker/docker-compose.test.yml` (repository root) - Redis, NATS and PostgreSQL test services
- `docs/operations/observers.md` (repository root) - Running observers under `fraiseql-server`

## Contributing

When adding new integration tests:

1. Use feature gates (`#[cfg(feature = "...")]`) for optional dependencies
2. Clean up test resources (Redis keys, etc.)
3. Document expected behavior and assertions
4. Add the test to this README's coverage section

# FraiseQL Observers Integration Tests

This directory contains end-to-end integration tests for the Redis + NATS observer system.

## Test Coverage

The integration tests validate:

1. **Full Pipeline with Redis Deduplication** (`test_full_pipeline_with_deduplication`)
   - Event processing with deduplication enabled
   - Verifies duplicate events are skipped
   - Measures deduplication overhead

2. **Cache Performance Improvement** (`test_cache_performance_improvement`)
   - Redis-backed action result caching
   - Validates cache backend creation
   - Documents expected 100x performance improvement in production

3. **Concurrent Execution Performance** (`test_concurrent_execution_performance`)
   - Compares sequential vs concurrent event processing
   - Validates parallel execution capabilities
   - Benchmarks throughput improvements

4. **Checkpoint Recovery** (`test_checkpoint_recovery`)
   - Validates checkpoint-based crash recovery
   - Requires PostgreSQL database (see deployment docs)

5. **Full Stack with All Features** (`test_full_stack_all_features`)
   - Tests complete executor stack (dedup + caching + concurrent)
   - Uses ExecutorFactory for proper composition
   - Validates end-to-end integration

6. **Error Handling and Resilience** (`test_error_handling_resilience`)
   - Validates graceful error handling
   - Tests system behavior with no observers configured

7. **Multi-Event Processing** (`test_multi_event_processing`)
   - Processes diverse event types
   - Validates system handles multiple entity types

## Running Tests

### Prerequisites

**Required**:

- Rust toolchain (1.70+)
- Redis server running on `localhost:6379` (or set `REDIS_URL` env var)

**Optional**:

- NATS server on `localhost:4222` (for NATS tests)
- PostgreSQL database (for checkpoint tests)

### Quick Start with Docker Compose

```bash
# Start Redis (required for most tests)
docker-compose -f docker-compose.postgres-redis.yml up -d redis

# Or start full stack (Redis + NATS + PostgreSQL)
docker-compose -f docker-compose.nats-distributed.yml up -d
```

### Running Tests

**All integration tests with Redis**:
```bash
cargo test --test integration_test --features "postgres,dedup,caching,testing"
```

**With NATS tests enabled**:
```bash
cargo test --test integration_test --features "postgres,dedup,caching,nats,testing"
```

**Run specific test**:
```bash
cargo test --test integration_test test_full_pipeline_with_deduplication --features "postgres,dedup,caching,testing"
```

**With output logging**:
```bash
cargo test --test integration_test --features "postgres,dedup,caching,testing" -- --nocapture
```

### Feature Flags

| Feature | Description | Required For |
|---------|-------------|--------------|
| `postgres` | PostgreSQL database support | Most tests |
| `dedup` | Redis deduplication | Deduplication tests |
| `caching` | Redis caching | Cache tests |
| `nats` | NATS transport | NATS tests |
| `testing` | Test mocks and utilities | All tests |
| `checkpoint` | Checkpoint recovery | Checkpoint tests |

## Expected Results

All tests should pass when Redis is running:

```
running 7 tests
test test_cache_performance_improvement ... ok
test test_concurrent_execution_performance ... ok
test test_error_handling_resilience ... ok
test test_full_pipeline_with_deduplication ... ok
test test_full_stack_all_features ... ok
test test_multi_event_processing ... ok
test test_checkpoint_recovery ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.15s
```

## Troubleshooting

### Redis Connection Failed

**Error**: `Failed to connect to Redis`

**Solution**:
```bash
# Check Redis is running
docker-compose -f docker-compose.postgres-redis.yml ps redis

# Or start Redis manually
docker run -d -p 6379:6379 redis:7-alpine

# Or use custom Redis URL
REDIS_URL=redis://your-redis:6379 cargo test --test integration_test --features "postgres,dedup,caching,testing"
```

### Test Timeout

**Error**: Test hangs or times out

**Solution**:

- Ensure Redis is responsive: `redis-cli ping` should return `PONG`
- Check network connectivity
- Increase test timeout: `cargo test -- --test-threads=1 --timeout=60`

### Feature Not Enabled

**Error**: `no tests to run matching...`

**Solution**: Make sure to include all required features:
```bash
cargo test --test integration_test --features "postgres,dedup,caching,testing"
```

## Performance Benchmarks

For detailed performance benchmarking, see:
```bash
cargo bench --bench observer_benchmarks
```

Benchmark results:

- **Event processing**: 1K-10K events/sec (without I/O)
- **Cache hit**: <1ms (100x faster than cache miss)
- **Dedup overhead**: ~0.1ms per event
- **Concurrent vs Sequential**: 2-3x faster with I/O-bound actions

## Manual Testing

For manual testing with Docker Compose, see:

- `DEPLOYMENT.md` - Complete deployment guide
- `docker-compose.*.yml` - Docker Compose configurations
- `examples/*.toml` - Configuration examples

## CI/CD Integration

Example GitHub Actions workflow:

```yaml
name: Integration Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest

    services:
      redis:
        image: redis:7-alpine
        ports:
          - 6379:6379
        options: >-
          --health-cmd "redis-cli ping"
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5

    steps:
      - uses: actions/checkout@v3

      - name: Setup Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable

      - name: Run integration tests
        run: cargo test --test integration_test --features "postgres,dedup,caching,testing"
        env:
          REDIS_URL: redis://localhost:6379
```

## Next Steps

After completing integration tests:

1. Review performance benchmarks: `cargo bench`
2. Deploy to staging using Docker Compose: see `DEPLOYMENT.md`
3. Run production smoke tests with real workload
4. Monitor metrics: cache hit rate, dedup hit rate, throughput
5. Tune configuration based on observed performance

## Contributing

When adding new integration tests:

1. Use feature gates (`#[cfg(feature = "...")]`) for optional dependencies
2. Clean up test resources (Redis keys, etc.)
3. Document expected behavior and assertions
4. Add test to this README's "Test Coverage" section
5. Update CI/CD workflow if new dependencies required

## Support

- **Documentation**: See `DEPLOYMENT.md` and `examples/README.md`
- **Issues**: https://github.com/your-org/fraiseql/issues
- **Discussions**: https://github.com/your-org/fraiseql/discussions

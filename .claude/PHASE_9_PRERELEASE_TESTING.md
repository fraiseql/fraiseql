# Phase 9: Pre-Release Testing & Verification

**Objective**: Run all tests, benchmarks, and E2E verification before declaring Phase 9 production-ready.

**Status**: 📋 PENDING (Not yet executed)

**Estimated Time**: 2-3 hours

---

## Critical: This Must Be Done Before Release

The Phase 9 implementation is code-complete and compiles cleanly, but the following have **NOT been verified with actual execution**:

- ❌ Phase 9.7 benchmarks (code exists but not run)
- ❌ ClickHouse integration (migrations created but not applied to running instance)
- ❌ Elasticsearch integration (code exists but not tested against running instance)
- ❌ E2E pipeline (Arrow → ClickHouse → Query verified)
- ❌ Stress tests (1M row throughput claims unverified)
- ❌ Chaos tests (failure scenarios untested)

---

## Pre-Release Checklist

### Phase 1: Environment Setup (15 min)

```bash
# Start all services
docker-compose -f docker-compose.test.yml up -d
docker-compose -f docker-compose.clickhouse.yml up -d
docker-compose -f docker-compose.elasticsearch.yml up -d

# Verify all services healthy
docker-compose -f docker-compose.test.yml ps
docker-compose -f docker-compose.clickhouse.yml ps
docker-compose -f docker-compose.elasticsearch.yml ps

# All should show "healthy" in STATUS
```

**Verification**:
- [ ] PostgreSQL 5433 responding
- [ ] NATS 4222 responding
- [ ] Redis 6379 responding
- [ ] ClickHouse 8123 responding
- [ ] Elasticsearch 9200 responding

---

### Phase 2: Compilation & Linting (10 min)

```bash
# Full clean build with all features
cargo clean
cargo build --all-features

# Strict linting
cargo clippy --all-targets --all-features -- -D warnings

# Type checking
cargo check --all-features
```

**Verification**:
- [ ] Zero compilation errors
- [ ] Zero clippy warnings
- [ ] All features compile

---

### Phase 3: Unit Tests (10 min)

```bash
# Run all observer tests
cargo test -p fraiseql-observers --features clickhouse

# Run all arrow tests
cargo test -p fraiseql-arrow --features clickhouse

# Run all core tests
cargo test -p fraiseql-core --all-features
```

**Expected Results**:
- [ ] All observer tests pass (255+ tests)
- [ ] All arrow tests pass (8+ clickhouse tests)
- [ ] All core tests pass

---

### Phase 4: Integration Tests (30 min)

#### 4.1: ClickHouse Migrations

```bash
# Apply migrations manually to verify schema
docker exec fraiseql-clickhouse clickhouse-client << 'EOF'
-- Check table exists
SELECT name FROM system.tables WHERE database='default' AND name='fraiseql_events';

-- Verify columns
DESCRIBE TABLE fraiseql_events;

-- Check materialized views
SELECT name FROM system.tables WHERE name LIKE 'fraiseql_%';

-- Verify indexes
SELECT * FROM system.indexes WHERE table='fraiseql_events';

-- Check TTL
SHOW CREATE TABLE fraiseql_events;
EOF
```

**Verification**:
- [ ] fraiseql_events table exists with 8 columns
- [ ] Indexes on event_type, entity_type, org_id created
- [ ] TTL configured for 90 days
- [ ] 3 materialized views created (hourly, org_daily, event_type_stats)

#### 4.2: Elasticsearch Index Templates

```bash
# Verify index templates
curl -s http://localhost:9200/_index_template | jq '.index_templates[] | select(.name | contains("fraiseql"))'

# Check ILM policy
curl -s http://localhost:9200/_ilm/policy | jq '.policies | keys'
```

**Verification**:
- [ ] fraiseql-events template exists
- [ ] fraiseql-requests template exists (if created)
- [ ] ILM policy configured

#### 4.3: Run E2E Pipeline Test

```bash
# Run test that generates events → stores in ClickHouse → queries back
cargo test --test integration_test --features clickhouse -- --ignored --nocapture
```

**Verification**:
- [ ] Events generated successfully
- [ ] ClickHouse inserted rows successfully
- [ ] Query returns expected data
- [ ] Materialized views populated

---

### Phase 5: Stress Tests (45 min)

#### 5.1: Million Row Performance

```bash
# Run 1M row stress test
cargo test --test stress_tests --features clickhouse million_row -- --ignored --nocapture

# Expected output:
# - Throughput: >100,000 rows/sec
# - Total time: <60 seconds for 1M rows
# - Memory: <500MB
```

**Verification**:
- [ ] 1M rows inserted without errors
- [ ] Throughput meets or exceeds 100k rows/sec
- [ ] Memory usage under 500MB
- [ ] ClickHouse still responsive after insert

#### 5.2: Sustained Load Test (10k events/sec for 5 minutes)

```bash
# Run sustained load test
cargo test --test stress_tests --features clickhouse sustained_load_10k -- --ignored --nocapture

# Expected output:
# - Sustained 10k events/sec for 5 minutes
# - No dropped events
# - No memory growth
```

**Verification**:
- [ ] 3M events (10k/sec × 300 sec) inserted without loss
- [ ] Memory remains constant (no leaks)
- [ ] Latency stable throughout test

---

### Phase 6: Chaos Tests (30 min)

Test failure scenarios to verify resilience:

```bash
# Run ClickHouse crash scenario
cargo test --test chaos_tests --features clickhouse clickhouse_crash -- --ignored --nocapture

# Run Elasticsearch unavailability
cargo test --test chaos_tests --features clickhouse elasticsearch_unavailable -- --ignored --nocapture

# Run NATS partition
cargo test --test chaos_tests --features clickhouse nats_partition -- --ignored --nocapture
```

**Verification**:
- [ ] ClickHouse crash: Sink recovers and flushes on restart
- [ ] Elasticsearch down: Graceful degradation, no crash
- [ ] NATS partition: Local buffer maintains, sync on recovery
- [ ] All failure modes logged clearly

---

### Phase 7: Benchmarks (45 min)

Run actual performance benchmarks:

```bash
# Run all benchmarks with real timing
cd crates/fraiseql-arrow
cargo bench --bench arrow_flight_benchmarks --features clickhouse -- --nocapture

# Capture output to file
cargo bench --bench arrow_flight_benchmarks --features clickhouse > /tmp/phase9_benchmarks.txt 2>&1
```

**Expected Results**:
```
Query Performance (rows to time):
  100 rows:     <100ms (target: <10ms is ideal)
  1,000 rows:   <200ms (target: <50ms is ideal)
  10,000 rows:  <3s (target: <300ms is ideal)
  100,000 rows: <30s (target: <2s is ideal)
  1,000,000 rows: <300s (target: <10s is ideal)

Throughput:
  JSON:  100-500 rows/sec
  Arrow: 100k-1M rows/sec (100-5000x improvement)

Memory (1M rows):
  JSON: 2.5GB
  Arrow: 100MB (25x reduction)
```

**Verification**:
- [ ] Benchmarks execute without panicking
- [ ] Real numbers captured (update documentation if different)
- [ ] Performance meets or exceeds Phase 9 targets
- [ ] Outliers identified and explained

---

### Phase 8: Data Flow End-to-End (30 min)

Verify complete flow: Event → NATS → Arrow Bridge → ClickHouse → Query

```bash
# 1. Generate test events
cargo run --example clickhouse_sink --features clickhouse

# Expected output:
# ✅ Configuration validated
# ✅ Sink created
# ✅ Generated N test events
# ✅ Events streamed to ClickHouse
# ✅ Verification query successful: X rows in fraiseql_events
```

**Verification**:
- [ ] Example runs without errors
- [ ] Events visible in ClickHouse:
  ```sql
  SELECT COUNT(*) FROM fraiseql_events;
  ```
- [ ] Materialized views updated:
  ```sql
  SELECT COUNT(*) FROM fraiseql_events_hourly;
  SELECT COUNT(*) FROM fraiseql_org_daily;
  ```

---

### Phase 9: Documentation Accuracy (15 min)

Verify all documentation is accurate:

```bash
# 1. Check getting-started tutorial works
# Follow steps in docs/arrow-flight/getting-started.md
# - Can you run the first query example?
# - Can you stream observer events?

# 2. Check migration guide is accurate
# Follow Phase 1 in docs/arrow-flight/migration-guide.md
# - Arrow Flight starts successfully
# - HTTP still works
# - Can connect to port 50051

# 3. Verify all code examples compile
# Extract all Rust/Python examples and try to run them
```

**Verification**:
- [ ] Getting-started tutorial works as-is
- [ ] Migration guide Phase 1 succeeds
- [ ] All code examples in docs are accurate
- [ ] No broken links in documentation

---

### Phase 10: Cleanup & Final Verification (10 min)

```bash
# Stop all services
docker-compose -f docker-compose.test.yml down
docker-compose -f docker-compose.clickhouse.yml down
docker-compose -f docker-compose.elasticsearch.yml down

# Verify repo is clean
git status
# Should show: On branch feature/phase-1-foundation, nothing to commit

# List all Phase 9 commits
git log --oneline | grep "phase-9\|Phase 9" | head -20
```

**Verification**:
- [ ] All services stopped cleanly
- [ ] No uncommitted changes
- [ ] Phase 9 commits documented

---

## Results Document

After running all tests, create a results summary:

**File**: `/home/lionel/code/fraiseql/.claude/PHASE_9_RELEASE_RESULTS.md`

Include:
- ✅/❌ Checklist completion
- Actual vs target performance numbers
- Any failures and root causes
- Recommendations before production use
- Sign-off statement

---

## Go/No-Go Criteria

### Must Pass (Blocking)
- [ ] All unit tests pass
- [ ] ClickHouse schema applied successfully
- [ ] Elasticsearch templates applied successfully
- [ ] E2E pipeline test passes
- [ ] No panics or crashes in tests
- [ ] Code compiles with zero warnings

### Should Pass (High Priority)
- [ ] Stress tests pass (1M rows)
- [ ] Sustained load test passes
- [ ] Chaos tests pass (resilience verified)
- [ ] Performance benchmarks match targets
- [ ] Documentation examples work

### Nice to Have
- [ ] Code coverage >80%
- [ ] Performance exceeds targets by >20%
- [ ] All CLI examples work
- [ ] Load test memory stable

---

## If Tests Fail

**Step 1**: Identify failure type
```bash
# Capture full output
RUST_LOG=debug cargo test --features clickhouse -- --nocapture > /tmp/test_failure.log 2>&1

# Find error
grep -i "error\|panic\|failed" /tmp/test_failure.log
```

**Step 2**: Root cause analysis
- Is it a code bug?
- Is it environmental (Docker not running)?
- Is it a data/schema issue?
- Is it a timeout issue?

**Step 3**: Document and fix
- Create issue in git
- Record failure mode
- Fix root cause
- Re-run tests

**Step 4**: Update roadmap
- Document what failed
- Document what was fixed
- Update this checklist

---

## Timeline

| Phase | Task | Time | Status |
|-------|------|------|--------|
| 1 | Environment Setup | 15 min | ⏳ |
| 2 | Compilation & Lint | 10 min | ⏳ |
| 3 | Unit Tests | 10 min | ⏳ |
| 4 | Integration Tests | 30 min | ⏳ |
| 5 | Stress Tests | 45 min | ⏳ |
| 6 | Chaos Tests | 30 min | ⏳ |
| 7 | Benchmarks | 45 min | ⏳ |
| 8 | E2E Data Flow | 30 min | ⏳ |
| 9 | Documentation | 15 min | ⏳ |
| 10 | Cleanup | 10 min | ⏳ |
| **Total** | | **240 min (4 hours)** | ⏳ |

---

## Success Criteria

Phase 9 is **release-ready** when:

1. ✅ All blockers pass
2. ✅ Most high-priority items pass
3. ✅ Actual performance >= target performance
4. ✅ All documentation verified
5. ✅ No critical bugs discovered
6. ✅ Results document created and signed off

---

## Next Step

Run this checklist end-to-end and report results. Update this document with actual findings, not assumed findings.

**Do not consider Phase 9 production-ready until this testing is complete.**

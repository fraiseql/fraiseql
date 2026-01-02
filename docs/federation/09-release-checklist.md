# Federation Release Checklist

## Pre-Deployment Verification (24 hours before release)

### Code Quality Gate

- [ ] **All tests pass**
  ```bash
  pytest tests/federation/ -v
  # Expected: 100% pass rate (15+ tests)
  ```

- [ ] **No breaking changes to public API**
  ```bash
  # Check signature compatibility
  git diff origin/dev -- src/fraiseql/federation/*.py | grep -E "^[-+]\s*(def|async def|class)"
  ```

- [ ] **Linting passes**
  ```bash
  ruff check src/fraiseql/federation/
  ruff format --check src/fraiseql/federation/
  ```

- [ ] **Type checking passes**
  ```bash
  mypy src/fraiseql/federation/ --strict
  ```

- [ ] **Security audit complete**
  ```bash
  # Check for potential security issues
  bandit -r src/fraiseql/federation/
  ```

### Documentation Gate

- [ ] **All docstrings present and complete**
  - [ ] Public classes documented
  - [ ] Public methods documented
  - [ ] Type hints complete
  - [ ] Return values documented

- [ ] **Code examples work**
  ```bash
  # Test all code examples in docs
  python -m doctest docs/federation/*.md -v
  ```

- [ ] **README updated**
  - [ ] Version number updated
  - [ ] New features documented
  - [ ] Known limitations listed

- [ ] **Changelog updated**
  - [ ] All changes listed
  - [ ] Categorized (Features, Fixes, Breaking)
  - [ ] Credits/contributors mentioned

### Dependency Gate

- [ ] **Dependencies up to date**
  ```bash
  # Check for security vulnerabilities
  safety check --json
  # Check for outdated packages
  pip list --outdated
  ```

- [ ] **Python version compatibility verified**
  ```bash
  # Minimum: Python 3.11
  python3.11 -m pytest tests/federation/
  ```

- [ ] **No new major dependencies added**
  - [ ] Added dependencies reviewed by team
  - [ ] License compatibility verified

### Performance Gate

- [ ] **Benchmark baseline established**
  ```bash
  pytest tests/federation/test_dataloader_performance.py --benchmark-only
  # Expected: 7-10x improvement over non-batched queries
  ```

- [ ] **Memory usage acceptable**
  ```bash
  # Peak memory during batch load: < 100MB for 1000 entities
  python -m memory_profiler tests/federation/test_dataloader_performance.py
  ```

- [ ] **No performance regressions**
  ```bash
  pytest tests/federation/ --benchmark-compare=baseline
  # Expected: No regressions > 10%
  ```

---

## Pre-Deployment Testing (7 days before release)

### Unit Test Coverage

- [ ] **DataLoader tests: 100% pass rate**
  ```bash
  pytest tests/federation/test_dataloader.py -v --cov=src/fraiseql/federation/dataloader
  # Expected: > 95% coverage
  ```

- [ ] **Batch executor tests: 100% pass rate**
  ```bash
  pytest tests/federation/test_batch_executor.py -v --cov=src/fraiseql/federation/batch_executor
  # Expected: > 95% coverage
  ```

- [ ] **Entity registry tests: 100% pass rate**
  ```bash
  pytest tests/federation/test_entities.py -v --cov=src/fraiseql/federation/entities
  # Expected: > 90% coverage
  ```

- [ ] **Error handling tests pass**
  ```bash
  pytest tests/federation/ -k "error" -v
  # Expected: All error cases handled gracefully
  ```

### Integration Test Coverage

- [ ] **Router integration tests pass**
  ```bash
  pytest tests/federation/test_apollo_router_integration.py -v
  # Expected: Federation SDL generation correct
  ```

- [ ] **Multi-service federation tests pass**
  ```bash
  pytest tests/federation/test_service_integration.py -v
  # Expected: Cross-service entity references work
  ```

- [ ] **Database integration tests pass**
  ```bash
  pytest tests/federation/test_db_integration.py -v
  # Expected: Correct JSONB view handling
  ```

### Stress Testing

- [ ] **Load test under 1000 concurrent requests**
  ```bash
  locust -f tests/federation/locustfile.py --headless -u 1000 -r 100 -t 60s
  # Expected: p99 latency < 500ms
  ```

- [ ] **Batch size tested up to 1000 entities**
  ```bash
  pytest tests/federation/test_batch_executor.py::test_large_batch -v
  # Expected: Completes in < 1s
  ```

- [ ] **Memory stable under continuous load**
  ```bash
  # Run for 30 minutes, monitor memory
  watch -n 1 'ps aux | grep federation_test'
  # Expected: No memory growth
  ```

### Edge Case Testing

- [ ] **Missing entity handling**
  ```bash
  pytest tests/federation/ -k "missing" -v
  # Expected: Returns null or error gracefully
  ```

- [ ] **Timeout handling**
  ```bash
  pytest tests/federation/ -k "timeout" -v
  # Expected: Requests timeout gracefully
  ```

- [ ] **Circular reference handling**
  ```bash
  pytest tests/federation/ -k "circular" -v
  # Expected: No infinite loops
  ```

- [ ] **Concurrent batch collision handling**
  ```bash
  pytest tests/federation/ -k "concurrent" -v
  # Expected: Batches correctly combined
  ```

---

## Deployment Safety Checks (4 hours before release)

### Environment Verification

- [ ] **Staging environment matches production**
  - [ ] Database: Same schema, same row count
  - [ ] Config: Same environment variables
  - [ ] Services: Same version dependencies

- [ ] **Backup created**
  ```bash
  # Full database backup
  pg_dump production_db | gzip > federation_release_backup_$(date +%Y%m%d).sql.gz
  # File location: /backups/federation/
  ```

- [ ] **Rollback plan tested**
  - [ ] Previous version verified working
  - [ ] Rollback scripts tested in staging
  - [ ] Team trained on rollback procedure

### Monitoring Setup

- [ ] **Prometheus metrics configured**
  ```yaml
  - federation_queries_total
  - federation_batch_size_avg
  - federation_error_rate
  - federation_latency_p99
  ```

- [ ] **Alert rules configured**
  ```yaml
  - Alert: error_rate > 1%
  - Alert: p99_latency > 1s
  - Alert: federation_disabled (automatic)
  ```

- [ ] **Log aggregation working**
  - [ ] Federation logs routing to splunk/datadog
  - [ ] Error logs tagged with severity
  - [ ] Request IDs correlating across services

- [ ] **Health check endpoint ready**
  ```bash
  curl -s http://localhost:4001/__health/federation
  # Expected: {"status": "healthy", "federation_enabled": true}
  ```

### Team Readiness

- [ ] **Deployment team briefing completed**
  - [ ] Deployment plan reviewed
  - [ ] Roles and responsibilities clear
  - [ ] Communication plan established

- [ ] **On-call support briefed**
  - [ ] Federation architecture explained
  - [ ] Common issues documented
  - [ ] Rollback procedure practiced

- [ ] **Emergency contact list prepared**
  - [ ] Tech lead
  - [ ] Database admin
  - [ ] DevOps engineer
  - [ ] Product manager

---

## Deployment Phase (Release Day)

### Pre-Flight Check (1 hour before)

- [ ] **All systems normal**
  ```bash
  # Check health of all services
  for service in user-service post-service product-service; do
    curl -s http://$service:4000/__health
  done
  ```

- [ ] **Database query performance acceptable**
  ```sql
  -- Check slow query log
  SHOW GLOBAL STATUS LIKE 'Slow_queries';
  -- Expected: < 5 slow queries/minute
  ```

- [ ] **Current error rate baseline**
  ```bash
  # Record current metrics
  curl -s http://prometheus:9090/api/v1/query?query=rate(http_requests_total%5B5m%5D)
  ```

- [ ] **Communication channels open**
  - [ ] Slack channel: #federation-deployment
  - [ ] War room ready
  - [ ] Team members online

### Canary Deployment (Phase 1)

- [ ] **Deploy to canary (5% traffic)**
  ```bash
  kubectl set image deployment/user-service-canary \
    user-service=fraiseql/federation:v1.0.0 \
    --record
  ```

- [ ] **Monitor canary metrics (15 minutes)**
  ```bash
  # Watch error rate, latency, throughput
  kubectl logs -f deployment/user-service-canary -c federation
  ```

- [ ] **Canary health checks pass**
  - [ ] Response time: < 100ms (p99)
  - [ ] Error rate: < 0.1%
  - [ ] Memory usage: < 500MB

- [ ] **Proceed to Phase 2 or rollback**
  - [ ] If healthy: Increase to 25%
  - [ ] If issues: Execute immediate rollback

### Gradual Rollout (Phase 2-4)

**Phase 2: 25% traffic (30 minutes)**
- [ ] Monitor metrics
- [ ] No issues detected
- [ ] Proceed to Phase 3

**Phase 3: 50% traffic (30 minutes)**
- [ ] Monitor metrics
- [ ] No issues detected
- [ ] Proceed to Phase 4

**Phase 4: 100% traffic (remaining services)**
- [ ] Monitor metrics
- [ ] All services updated
- [ ] Enable full federation

### Post-Deployment Validation (30 minutes)

- [ ] **Metrics look healthy**
  ```bash
  # Check key metrics are in expected range
  - federation_enabled: true
  - error_rate: < 0.5%
  - p99_latency: < 500ms
  - batch_effectiveness: > 80%
  ```

- [ ] **Queries are batched**
  ```bash
  # Check DataLoader is actually being used
  SELECT COUNT(*) as total_queries FROM audit_log
    WHERE timestamp > now() - interval '5 minutes'
    AND operation = 'federation_batch'
  # Expected: > 80% of queries are batched
  ```

- [ ] **No memory leaks**
  ```bash
  # Memory usage stable
  watch -n 1 'curl http://localhost:4001/metrics | grep memory'
  # Expected: No growth over 30 minutes
  ```

- [ ] **Error logs reviewed**
  ```bash
  # Check for any unusual errors
  tail -100 /var/log/federation.log | grep ERROR
  # Expected: No critical errors
  ```

---

## Post-Deployment Monitoring (7 days)

### Hour 1-4: Intensive Monitoring

- [ ] **Every 15 minutes: Check key metrics**
  - Error rate trend
  - Latency trend
  - Memory usage trend
  - Query count trend

- [ ] **Every 30 minutes: Check logs**
  - No critical errors
  - No unexpected warnings
  - Service integrations healthy

- [ ] **Team available for quick rollback**
  - On-call engineer monitoring
  - Team in Slack channel
  - Rollback command ready

### Day 1: Continuous Monitoring

- [ ] **Hourly check of metrics**
  - Error rate: Stable or improving
  - Latency: Consistent
  - Memory: No growth
  - Batching: Effective

- [ ] **User feedback monitored**
  - Any performance complaints?
  - Any data consistency issues?
  - Any usability problems?

- [ ] **Database performance**
  ```sql
  -- Check query patterns changed as expected
  SELECT operation, COUNT(*) FROM audit_log
    WHERE timestamp > now() - interval '1 day'
    GROUP BY operation;
  ```

### Days 2-7: Daily Monitoring

- [ ] **Daily metric review**
  - Performance improvements visible
  - Error rate at or below baseline
  - Memory usage stable
  - Batching effectiveness > 80%

- [ ] **Daily log review**
  - No warnings or errors
  - Federation working as expected
  - No database issues

- [ ] **Weekly team sync**
  - Discuss metrics and trends
  - Address any concerns
  - Plan next steps

- [ ] **Compare with baseline**
  ```bash
  # Expected improvements from Week 5
  - Query count: 80-95% reduction
  - Response time: 7-10x improvement
  - Database load: 70-80% reduction
  ```

---

## Rollback Procedures

### Automatic Rollback Triggers

Federation is automatically disabled if:
- Error rate exceeds 5%
- p99 latency exceeds 2 seconds
- DataLoader batch timeout exceeds 5 seconds
- Memory usage exceeds 1GB

### Manual Rollback Steps

**Step 1: Disable Federation (Immediate)**
```bash
# Set feature flag to disabled
export FEDERATION_ENABLED=false
# Or update config
kubectl set env deployment/user-service FEDERATION_ENABLED=false
```

**Step 2: Restart Services**
```bash
# Restart in controlled manner
kubectl rollout restart deployment/user-service
# Watch for successful startup
kubectl rollout status deployment/user-service
```

**Step 3: Verify Rollback**
```bash
# Check services are responding
for service in user-service post-service product-service; do
  curl -s http://$service:4000/__health
done

# Check error rate has returned to baseline
curl -s http://prometheus:9090/api/v1/query?query=rate(http_requests_total%5B5m%5D)
```

**Step 4: Post-Mortem**
```markdown
# Rollback Analysis

## What went wrong
[Description of issue that caused rollback]

## Timeline
- HH:MM - Issue detected
- HH:MM - Federation disabled
- HH:MM - Services restarted
- HH:MM - Confirmed stable

## Root cause
[Analysis of the problem]

## Fix
[Solution being implemented]

## Next steps
[Plan to re-deploy after fix]
```

---

## Success Criteria

Release is considered successful if:

**Performance:**
- ✅ Query count reduced by 80-95%
- ✅ Response time 7-10x faster
- ✅ Database load reduced by 70-80%

**Reliability:**
- ✅ Error rate at or below 0.5%
- ✅ p99 latency < 500ms
- ✅ No memory leaks
- ✅ No data consistency issues

**Operations:**
- ✅ Monitoring alert-free
- ✅ Logs clean
- ✅ Team confident in operation

---

## Sign-Off

**Deployment Lead:** _________________ Date: _____

**Tech Lead:** _________________ Date: _____

**Product Manager:** _________________ Date: _____

---

## Related Documents

- [07-testing-guide.md](07-testing-guide.md) - Test patterns
- [08-migration-guide.md](08-migration-guide.md) - Service migration
- [10-go-live.md](10-go-live.md) - Go-live strategy and monitoring

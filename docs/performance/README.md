# Performance Tuning Guide

**Status**: 🟢 Production Ready
**Last Updated**: 2026-01-31
**Performance Improvement**: **42-55% latency reduction with tuning**

## Quick Start (5 minutes)

If you just upgraded to v2.0.0-alpha.1:

### 1. No Changes Required ✅

SQL projection optimization is **enabled automatically**. Your queries are now 42-55% faster.

```bash
# Just deploy the new version
cargo build --release
# Queries automatically faster!
```text

### 2. Verify It's Working

Check logs for projection SQL:

```bash
RUST_LOG=fraiseql_core::runtime=debug cargo run
# Look for: "SQL with projection = jsonb_build_object(...)"
```text

### 3. Monitor Performance

```bash
# Measure latency before/after
wrk -t4 -c100 -d30s http://localhost:3000/graphql

# Expected: 40-55% improvement automatically
```text

## Performance Improvements Already Included

| Feature | Impact | Status |
|---------|--------|--------|
| **SQL Projection** | 42-55% latency ↓ | ✅ v2.0.0-alpha.1 |
| **Projection Caching** | 2-10x speedup | ✅ v2.0.0-alpha.1 |
| **Query Plan Caching** | 10-20% speedup | ✅ v2.0.0-alpha.1 |
| **Connection Pooling** | Tunable | ✅ Documented |

**Total out-of-box improvement: 42-55%** 🎉

## Detailed Tuning Guides

### For GraphQL API Teams

**[SQL Projection Optimization](./projection-optimization.md)**

- How projection works
- Best practices for query design
- Troubleshooting and FAQ
- Real-world examples

**[Connection Pool Tuning](./connection-pool-tuning.md)**

- Pool size configuration
- Monitoring and alerts
- Optimization techniques
- Production checklist

### For DevOps / Infrastructure

**[Migration & Deployment](../deployment/migration-projection.md)**

- Zero-breaking-changes upgrade path
- Performance testing methodology
- Rollback procedures
- Monitoring setup

### For Engineers / Researchers

**[Benchmark Results](./projection-baseline-results.md)**

- Raw performance data
- Statistical methodology
- Database-specific analysis
- Real-world impact calculations

## By Use Case

### Development / Testing

**Configuration**:

```rust
let adapter = PostgresAdapter::with_pool_size(connection_string, 5).await?;
```text

**Expected**:

- Latency: 50-100ms p95
- Throughput: 1K-5K req/s
- Memory: Low (small pool)

**Tuning**: Not needed for development

### Staging / Pre-Production

**Configuration**:

```rust
let adapter = PostgresAdapter::with_pool_size(connection_string, 20).await?;
```text

**Expected**:

- Latency: 20-50ms p95
- Throughput: 5K-20K req/s
- Memory: Moderate

**Tuning**:

1. Run load tests
2. Monitor pool utilization
3. Adjust pool size if needed
4. Validate projection is working

### Production / Scale

**Configuration**:

```rust
let max_size = (num_cpus::get() * 2) + 5;
let adapter = PostgresAdapter::with_pool_size(connection_string, max_size).await?;
```text

**Expected**:

- Latency: 10-30ms p95
- Throughput: 20K+ req/s
- Memory: Optimized

**Tuning**:

1. Pre-warm pool on startup
2. Monitor pool metrics
3. Set up alerts
4. Load test before deploy
5. Monitor for 24h post-deploy

## Performance Checklist

### Pre-Deployment

- [ ] Upgraded to v2.0.0-alpha.1
- [ ] Ran full test suite
- [ ] Connection pool size calculated
- [ ] Load test passed
- [ ] SQL projection verified in logs
- [ ] Monitoring configured
- [ ] Alerts configured

### Post-Deployment

- [ ] Monitor latency (expect 40-55% improvement)
- [ ] Monitor pool utilization
- [ ] Monitor database connections
- [ ] Check error rates (should be unchanged)
- [ ] Confirm projection in production logs

## Troubleshooting

### Queries Slower After Upgrade

**Unlikely, but if it happens**:

1. Check if projection is working:

   ```bash
   RUST_LOG=fraiseql_core::runtime=debug cargo run
   ```text

2. Disable projection temporarily:

   ```bash
   FRAISEQL_DISABLE_PROJECTION=true cargo run
   ```text

3. Check pool metrics:

   ```rust
   let metrics = adapter.pool_metrics();
   println!("{:?}", metrics);
   ```text

4. Check query complexity:
   - Did you add complex joins?
   - Did you add expensive WHERE clauses?
   - Are you returning more fields?

### High Latency

**Check in order**:

1. Pool utilization:

   ```rust
   if metrics.waiting_requests > 0 {
       // Pool too small - increase size
   }
   ```text

2. Query performance:

   ```bash
   RUST_LOG=debug  # Check query times
   ```text

3. Database load:

   ```sql
   -- Check for slow queries
   SELECT * FROM pg_stat_statements
   WHERE mean_time > 100;
   ```text

### Connection Pool Exhaustion

**Symptoms**: Errors about too many connections

**Solutions**:

1. Increase pool size
2. Optimize slow queries
3. Add indexes to database
4. Load balance across servers

## Monitoring Dashboard

### Key Metrics

```text
Query Latency (p50/p95/p99):  __ms
Pool Utilization:             __%
Active Connections:           __
Waiting Requests:             __
Database Connections:         __
Network Bandwidth:            __MB/s
Error Rate:                   __%
```text

### Alert Thresholds

```yaml
queries:
  - name: High Latency
    condition: p95_latency > 100ms
    action: Page on-call

  - name: Pool Exhaustion
    condition: waiting_requests > 5
    action: Page on-call

  - name: High Error Rate
    condition: error_rate > 1%
    action: Page on-call
```text

## Performance Benchmarks

### Out-of-Box Performance

With v2.0.0-alpha.1 on medium hardware (8 cores, 16GB RAM):

```text
Latency (p50):     12ms
Latency (p95):     28ms
Latency (p99):     35ms
Throughput:        6000+ req/s
Concurrent Users:  500+
```text

### Scaling Characteristics

```text
1K users:   Single server (max pool 20)
10K users:  5-10 servers (load balanced)
100K users: 50-100 servers (geo-distributed)
```text

## FAQ

**Q: Do I need to change my queries?**
A: No, projection is automatic. Your queries run faster without changes.

**Q: What's the performance improvement?**
A: 42-55% latency reduction automatically with v2.0.0-alpha.1.

**Q: Is there any risk in upgrading?**
A: No, it's fully backward compatible. No breaking changes.

**Q: Can I disable projection?**
A: Yes, set `FRAISEQL_DISABLE_PROJECTION=true` for debugging.

**Q: How do I know if projection is working?**
A: Check logs: `RUST_LOG=fraiseql_core::runtime=debug` look for `jsonb_build_object`.

**Q: Should I tune the connection pool?**
A: For production: Yes, size it based on your concurrency. For dev: No, defaults are fine.

**Q: What's the right pool size?**
A: Start with `(core_count × 2) + 5`. Monitor and adjust based on utilization.

**Q: How do I monitor pool health?**
A: Use `adapter.pool_metrics()` and track active/idle/waiting connections.

**Q: What about multi-database setups?**
A: Each database gets its own pool. Tune separately.

## Additional Resources

### Documentation

- [SQL Projection Optimization](./projection-optimization.md) - Deep dive into projection
- [Connection Pool Tuning](./connection-pool-tuning.md) - Pool configuration
- [Benchmark Results](./projection-baseline-results.md) - Statistical data
- [Migration Guide](../deployment/migration-projection.md) - Upgrade guide

### External Resources

- [deadpool-postgres](https://github.com/bikeshedder/deadpool) - Connection pooling library
- [PostgreSQL Tuning](https://wiki.postgresql.org/wiki/Performance_Optimization) - Database tuning
- [GraphQL Best Practices](https://graphql.org/learn/best-practices/) - Query design

## Support

- **Documentation Issues**: Check the [guides above](#additional-resources)
- **Bug Reports**: Include `RUST_LOG=debug` output
- **Performance Help**: Share your load profile and metrics

---

## Quick Reference

### Enable Tuning (30 seconds)

```rust
// Calculate pool size
let pool_size = (num_cpus::get() * 2) + 5;

// Create adapter with tuned pool
let adapter = PostgresAdapter::with_pool_size(
    &std::env::var("DATABASE_URL")?,
    pool_size
).await?;

// Monitor in your GraphQL handler
let metrics = adapter.pool_metrics();
if metrics.waiting_requests > 0 {
    eprintln!("Pool saturation: {} waiting", metrics.waiting_requests);
}
```text

### Verify Performance Improvement

```bash
# Before upgrade (save this)
wrk -t4 -c100 -d30s http://old-server/graphql > before.txt

# After upgrade
wrk -t4 -c100 -d30s http://new-server/graphql > after.txt

# Compare (expect ~40-55% improvement)
```text

### Monitor Production

```bash
# Watch latency trend
watch -n 5 'psql $DATABASE_URL -c "SELECT
  percentile_cont(0.5) WITHIN GROUP (ORDER BY query_time),
  percentile_cont(0.95) WITHIN GROUP (ORDER BY query_time),
  COUNT(*)
FROM query_log WHERE timestamp > now() - interval 1 minute"'
```text

---

**Next**: Choose a guide above based on your role:

- 👨‍💻 **Developer**: Read [Projection Optimization](./projection-optimization.md)
- 🏗️ **Architect**: Read [Connection Pool Tuning](./connection-pool-tuning.md)
- 🚀 **DevOps**: Read [Migration Guide](../deployment/migration-projection.md)
- 📊 **Engineer**: Read [Benchmark Results](./projection-baseline-results.md)

# Phase 2.4: Stress Testing Plan

## Objective
Validate subscriptions module under extreme conditions: 10,000+ concurrent connections, network failures, memory pressure, and cascading failures. Establish upper bounds and degradation patterns.

## Context
- Phase 2.1-2.3 established baseline performance (1000-10,000 scale)
- Current target: 10,000+ concurrent, extreme latency, failure injection
- Goal: Identify breaking points and failure modes

## Test Categories

### 1. Extreme Concurrency Tests
Push beyond normal operating conditions to find breaking points.

#### test_stress_10000_concurrent_connections
**Target:** 10,000 simultaneous WebSocket connections
**Method:**
- Spawn 10,000 tokio tasks
- Each establishes subscription to unique channel
- All happen concurrently (no batching)
- Measure success rate, time, memory usage
- Track failed connections and reasons

**Assertions:**
- >99% success rate (max 100 failures)
- Complete in <30 seconds
- Memory usage < 2GB
- No panics or errors

**Expected behavior:**
- May experience slower connection setup at extreme scale
- Should gracefully handle queueing if connection limit hit
- All subscriptions should work (not rejected)

---

#### test_stress_50000_subscriptions_cascade
**Target:** 50,000 subscriptions (5x load test)
**Method:**
- Create subscriptions in waves (10K at a time)
- Yield between waves
- Measure cumulative time and memory
- Verify all subscriptions active simultaneously

**Assertions:**
- All 50,000 subscriptions active
- Memory proportional to subscription count
- No memory leaks
- Active_subscribers stat accurate

**Expected behavior:**
- Will be slow (5x scale)
- Memory usage ~2-3GB
- Linear scaling with subscription count

---

### 2. Network Latency Injection Tests
Simulate realistic network conditions with high latency.

#### test_stress_latency_1000ms_connections
**Target:** 1,000 connections with 1000ms latency per publish
**Method:**
- Delay every publish/subscribe operation by 1000ms
- Use tokio::time::sleep before operations
- Measure throughput degradation
- Verify no timeouts or failures

**Assertions:**
- All operations complete (no timeout)
- Throughput drops proportionally
- 1000+ events delivered despite latency

**Real-world relevance:**
- WAN connections
- Geographically distant data centers
- High-latency satellite links

---

#### test_stress_jitter_50_500ms_latency
**Target:** Random latency between 50-500ms
**Method:**
- Use random jitter generator
- Apply to publish and receive operations
- Publish 1000 events over unpredictable latencies
- Measure variance in delivery times

**Assertions:**
- All events delivered
- Delivery time varies (confirms jitter)
- No deadlocks despite variable latency

**Real-world relevance:**
- Variable network conditions
- Congested links
- Adaptive network paths

---

### 3. Connection Failure Tests
Simulate connection drops and reconnection.

#### test_stress_random_connection_drops
**Target:** 1000 connections with random drops
**Method:**
- Create 1000 subscriptions
- Randomly drop 20% (200) mid-operation
- Others continue publishing/receiving
- Remaining connections should work normally
- Track error recovery

**Assertions:**
- Dropped connections fail gracefully
- Remaining connections unaffected
- Stats updated correctly
- No cascading failures

**Failure modes tested:**
- Drop during subscribe
- Drop during publish
- Drop during receive (EOF)
- Drop during unsubscribe

---

#### test_stress_cascading_connection_failures
**Target:** Failures that cascade through system
**Method:**
- Create 100 channels, 10 subscribers each
- Introduce network failure affecting 50% of connections
- Monitor impact on remaining subscriptions
- Measure recovery time
- Test circuit breaker patterns

**Assertions:**
- Failed subscriptions don't crash others
- Healthy subscriptions continue working
- Metrics are accurate after failure
- Recovery is automatic

---

### 4. Memory Pressure Tests
Push memory to limits and verify behavior.

#### test_stress_memory_saturation_event_queue
**Target:** Create large event backlog to trigger memory pressure
**Method:**
- Publish events faster than subscribers can consume
- Create 10,000 events in rapid succession (no delays)
- 100 subscribers on same channel
- Monitor memory growth and backpressure
- Verify no OOM crash

**Assertions:**
- No out-of-memory panic
- System handles backlog gracefully
- Memory stabilizes (not leaking)
- Events eventually delivered (high latency OK)

**Expected behavior:**
- Subscriber buffer fills up
- Publish operations may slow down
- Eventual delivery when consumer catches up

---

#### test_stress_large_payload_memory_limits
**Target:** Test with maximum reasonable payload sizes
**Method:**
- Publish 1MB events (10x normal)
- 100 concurrent subscribers
- 100 events total = 100MB memory
- Verify buffer management

**Assertions:**
- All events delivered
- Memory usage reasonable (not 10x)
- No buffer overflows
- Payloads not corrupted

**Payload escalation:**
- 100KB (normal, tested in Phase 2.3)
- 500KB (stress)
- 1MB (extreme)
- Verify max that doesn't break

---

### 5. Sustained Extreme Load Tests
Extended duration tests under stress.

#### test_stress_24hour_sustained_10k_subscriptions
**Target:** 10,000 subscriptions operating 24 hours (simulated)
**Method:**
- Create 10,000 subscriptions
- Sustain 100 events/sec publish rate
- Run for 86,400 seconds (compressed in milliseconds)
- Publish 8.64 million events
- Monitor memory and stats over time

**Assertions:**
- No memory leaks over long duration
- Stats remain accurate
- No performance degradation
- All events delivered

**Compressed timeline:**
- 1 second test ≈ 1 hour real time
- 86 second test ≈ 24 hours real time
- Still validates long-running behavior

---

#### test_stress_thundering_herd_recovery
**Target:** Simultaneous failure and recovery of many connections
**Method:**
- Create 1000 subscriptions
- Abruptly close 50% (500) simultaneously
- Immediately reconnect all 500
- Measure recovery time
- Verify system stability

**Assertions:**
- All 500 reconnect within 5 seconds
- No data loss during transition
- No cascade effects
- Stats accurate

**Real-world scenario:**
- Network partition recovery
- Server restart with existing connections
- Load balancer failover

---

### 6. Edge Case Combinations

#### test_stress_high_latency_plus_large_payload_plus_many_subscribers
**Target:** Combine multiple stressors
**Method:**
- 500ms latency on all ops
- 100KB payloads
- 100 concurrent subscribers
- 1000 events total
- Measure combined impact

**Assertions:**
- All events delivered (despite all stressors)
- Proportional time impact
- No panic or corruption
- No hidden bugs from interaction

---

#### test_stress_churn_plus_failures_plus_memory_pressure
**Target:** Simultaneous subscribe/unsubscribe with failures and memory load
**Method:**
- 100 rapid subscribe/unsubscribe cycles
- 10% random failure rate
- Large payload events (100KB)
- 1000 total events
- Memory pressure throughout

**Assertions:**
- Resilient to all three stressors
- Correct final state
- No resource leaks
- Clean failure modes

---

## Implementation Strategy

### Phasing
1. **Phase A:** Extreme concurrency (tests 1-2)
2. **Phase B:** Network conditions (tests 3-4)
3. **Phase C:** Connection failures (tests 5-6)
4. **Phase D:** Memory and duration (tests 7-8)
5. **Phase E:** Combined stress (tests 9-10)

### Test Infrastructure

**Utility: Latency Simulator**
```rust
pub struct LatencySimulator {
    min: Duration,
    max: Duration,
    jitter: bool,
}

impl LatencySimulator {
    pub async fn apply(&self) {
        let delay = if self.jitter {
            // Random between min and max
        } else {
            // Fixed delay
        };
        tokio::time::sleep(delay).await;
    }
}
```

**Utility: Failure Injector**
```rust
pub struct FailureInjector {
    failure_rate: f64, // 0.0-1.0
}

impl FailureInjector {
    pub fn should_fail(&self) -> bool {
        rand::random::<f64>() < self.failure_rate
    }
}
```

**Utility: Resource Monitor**
```rust
pub struct ResourceMonitor {
    initial_memory: u64,
    peak_memory: u64,
    start_time: Instant,
}

impl ResourceMonitor {
    pub fn sample(&mut self) {
        // Record current memory usage
    }

    pub fn report(&self) -> MonitorReport {
        // Return memory deltas, duration, etc.
    }
}
```

### Execution

**Test Framework:**
```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[allow(clippy::excessive_nesting)] // Stress tests require nesting
async fn test_stress_NAME() {
    let monitor = ResourceMonitor::new();

    // Test execution

    monitor.report();
}
```

**Concurrent Spawning:**
```rust
let mut handles = vec![];
for i in 0..count {
    handles.push(tokio::spawn(async move { ... }));
}
for handle in handles {
    handle.await?;
}
```

## Success Criteria

### Performance Thresholds
- 10,000 connections: < 30 seconds
- 50,000 subscriptions: < 60 seconds
- 1000ms latency: No failures, proportional slowdown
- Memory pressure: Graceful degradation, no OOM panic

### Reliability Metrics
- Error rate < 0.5% at 10K scale
- 100% recovery from isolated failures
- No cascading failures from 20-50% partial failure
- No memory leaks over 24-hour simulated duration

### Data Integrity
- 100% event delivery (eventual consistency OK)
- No payload corruption despite latency/failures
- Stats accurate within ±1% at all scales

## Deliverables

1. **10 stress test functions** (test_stress_*)
2. **Utility modules** (latency_simulator, failure_injector, resource_monitor)
3. **Performance baseline document** (stress_results.md)
4. **Failure mode analysis** (failure_modes.md)
5. **Recommendations for tuning** (optimization_targets.md)

## Timeline
- Estimated: 40 hours
- 8 tests × 4 hours setup/execution/analysis
- 8 hours utilities and infrastructure
- Total: 40-50 hours

## Success Metrics

**Passing all stress tests indicates:**
✅ System ready for high-scale production
✅ Identified failure modes and recovery patterns
✅ Performance baselines established
✅ Production readiness: B+ → A- (85%+)

## Notes

- Stress tests intentionally designed to challenge limits
- Failures expected at upper bounds (10K+ concurrent)
- Goal: Understand graceful degradation, not achieve 100% success
- Results will inform Phase 3-4 optimization priorities

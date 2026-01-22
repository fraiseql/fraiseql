# Phase 8: Visual Architecture & Data Flow

---

## 🏗️ Complete System Architecture (Phase 8 Complete)

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                         DATABASE MUTATIONS                                   │
│                          (INSERT/UPDATE/DELETE)                              │
└──────────────────────────┬───────────────────────────────────────────────────┘
                           │
                           ▼
        ┌──────────────────────────────────────────┐
        │  PostgreSQL Triggers & Change Log        │
        │  (tb_entity_change_log with Debezium)    │
        │  - Stores every mutation immutably       │
        │  - Available for polling/recovery        │
        └──────────────┬───────────────────────────┘
                       │
         ┌─────────────▼──────────────┐
         │   CHECKPOINT STORE 🔄      │
         │   (PostgreSQL)             │
         │   Persists listener state  │
         │   Last processed ID        │
         └─────────────┬──────────────┘
                       │
                       ▼
        ┌──────────────────────────────────────────┐
        │    ChangeLogListener (Phase 7)           │
        │    ┌──────────────────────────────────┐  │
        │    │ 1. Load checkpoint (resume)      │  │
        │    │ 2. Poll next batch from DB       │  │
        │    │ 3. Parse Debezium envelopes     │  │
        │    │ 4. Convert to EntityEvent       │  │
        │    │ 5. Emit to channel              │  │
        │    │ 6. Save checkpoint ✓            │  │
        │    └──────────────────────────────────┘  │
        └──────────────┬───────────────────────────┘
                       │
         ┌─────────────▼──────────────┐
         │ DEDUPLICATION CHECK 🛡️    │
         │ (Redis)                    │
         │ Skip if seen in 5min       │
         └─────────────┬──────────────┘
                       │
         ┌─────────────▼──────────────────────┐
         │ Bounded Channel (1000 events)      │
         │ With overflow policy handling      │
         └─────────────┬──────────────────────┘
                       │
         ┌─────────────▼────────────────────────────────────────┐
         │           OBSERVER EXECUTOR (Enhanced)               │
         ├─────────────────────────────────────────────────────┤
         │ 1. EventMatcher: O(1) lookup                        │
         │    └─ Find all observers for event type + entity   │
         │                                                      │
         │ 2. ConditionEvaluator: Evaluate conditions         │
         │    └─ "status == 'shipped' && total > 100"        │
         │                                                      │
         │ 3. Observable matching results                      │
         │    ├─ Count of matching observers                   │
         │    └─ Metrics recording                             │
         │                                                      │
         │ 4. ConcurrentActionExecutor 🚀                      │
         │    ├─ All actions run in parallel                   │
         │    ├─ Timeout per action                            │
         │    └─ Metrics per action type                       │
         └───────────────┬────────────────────────────────────┘
                         │
        ┌────────────────▼────────────────┐
        │   For Each Action (Parallel)    │
        └────────────────┬────────────────┘
                         │
         ┌───────────────▼────────────────────────────┐
         │    CachedActionExecutor 💾                 │
         │  ┌────────────────────────────────────┐   │
         │  │ 1. Generate cache key              │   │
         │  │ 2. Check CACHE (Redis) HIT ✓       │   │
         │  │    └─ Return cached result         │   │
         │  │ 3. CACHE MISS:                     │   │
         │  │    └─ Proceed to execution         │   │
         │  └────────────────────────────────────┘   │
         └────────────────┬────────────────────────┘
                          │
         ┌────────────────▼─────────────────────────────┐
         │   CircuitBreaker 🔌 (per endpoint)           │
         │  ┌──────────────────────────────────────┐   │
         │  │ Status: Closed / Open / HalfOpen     │   │
         │  │ - Track consecutive failures         │   │
         │  │ - Open circuit after threshold       │   │
         │  │ - HalfOpen tests recovery            │   │
         │  └──────────────────────────────────────┘   │
         └────────────────┬─────────────────────────┘
                          │
         ┌────────────────▼─────────────────────────────┐
         │      Decision: Execute or Queue?             │
         │  ┌──────────────────────────────────────┐   │
         │  │ Fast actions:                         │   │
         │  │  - Webhook, Slack, Cache, Search     │   │
         │  │  └─ Execute immediately (50-200ms)   │   │
         │  │                                       │   │
         │  │ Slow actions:                         │   │
         │  │  - Email, SMS, Bulk operations       │   │
         │  │  └─ Enqueue to JOB QUEUE             │   │
         │  │     └─ Return immediately to obs     │   │
         │  └──────────────────────────────────────┘   │
         └────────────────┬─────────────────────────┘
                          │
          ┌───────────────┴──────────────┐
          │                              │
          ▼ (Execute)                    ▼ (Queue)
┌─────────────────────────┐   ┌──────────────────────────┐
│   Direct Action         │   │   Job Queue 📮           │
│   Execution             │   │  ┌────────────────────┐  │
│   (Retry on failure)    │   │  │ 1. Create Job      │  │
│                         │   │  │ 2. Enqueue to DB   │  │
│                         │   │  │ 3. Return to obs   │  │
│                         │   │  │    immediately     │  │
│                         │   │  └────────────────────┘  │
│                         │   └──────────┬───────────────┘
│                         │              │
│                         │              ▼ (Async)
│                         │   ┌──────────────────────────┐
│                         │   │  JobQueueWorker 🏗       │
│                         │   │  (1-N parallel workers)  │
│                         │   │  ┌────────────────────┐  │
│                         │   │  │ 1. Dequeue job     │  │
│                         │   │  │ 2. Execute action  │  │
│                         │   │  │ 3. Retry on error  │  │
│                         │   │  │ 4. Mark complete   │  │
│                         │   │  └────────────────────┘  │
│                         │   └──────────────────────────┘
│                         │
└────────────┬────────────┘
             │
    ┌────────▼────────────┐
    │ Result Handling     │
    ├─────────────────────┤
    │ Success:            │
    │ ├─ Cache result 💾  │
    │ ├─ Increment metrics│
    │ └─ Search index 🔍  │
    │                     │
    │ Failure:            │
    │ ├─ To DLQ (retry)   │
    │ ├─ Circuit breaker  │
    │ ├─ Metrics record   │
    │ └─ Alert if needed  │
    └────────┬────────────┘
             │
    ┌────────▼────────────────────────────┐
    │    Event Indexing & Metrics 📊      │
    ├────────────────────────────────────┤
    │ 1. Index event to Elasticsearch 🔍 │
    │    └─ Full audit trail              │
    │                                     │
    │ 2. Record Prometheus metrics        │
    │    ├─ Events processed              │
    │    ├─ Action latencies              │
    │    ├─ Cache hit rates               │
    │    ├─ DLQ depth                     │
    │    └─ Worker health                 │
    │                                     │
    │ 3. Structured logging               │
    │    └─ JSON logs with context        │
    └────────┬────────────────────────────┘
             │
    ┌────────▼──────────────────────────────┐
    │    EXECUTION SUMMARY                  │
    ├───────────────────────────────────────┤
    │ - Entity matched observers: N         │
    │ - Observers executed: N               │
    │ - Actions queued: N                   │
    │ - Actions succeeded: N                │
    │ - Actions failed: N                   │
    │ - DLQ additions: N                    │
    │ - Processing time: XXXms              │
    │ - Metrics recorded: ✓                 │
    │ - Event indexed: ✓                    │
    └───────────────────────────────────────┘
```

---

## 📊 Request Flow Timeline (Single Event)

```
Time (ms)  Event
───────────────────────────────────────────────────────────────
0ms        ┌─ Database INSERT detected
           └─ Trigger fires, writes to change_log

1ms        ┌─ ChangeLogListener polls change_log
           ├─ Loads checkpoint: last_id=1000
           ├─ Fetches entries 1001-1100
           └─ Got 50 new entries

5ms        ┌─ For entry (ID=1050):
           ├─ Parse Debezium envelope
           └─ Convert to EntityEvent

7ms        ┌─ Check DEDUPLICATION (Redis)
           ├─ Key: "order:550e8400:insert:1234567890"
           ├─ Not found (new event)
           └─ Mark seen with 5min TTL

9ms        ┌─ Emit to bounded channel
           └─ Enqueue for processing

10ms       ┌─ ObserverExecutor receives event
           ├─ EventMatcher lookup: 3 matching observers
           ├─ Record metric: observers_matched = 3
           └─ Process each observer

11ms       ┌─ Observer 1: Order Created
           ├─ Condition eval: "total > 100" ✓
           ├─ Actions: [Webhook, Email, Cache]
           ├─ Start concurrent execution:
           │  ├─ Webhook (500ms) ──┐
           │  ├─ Email (queue) ────┼─ All parallel
           │  └─ Cache (50ms) ─────┘
           └─ (continue with other observers)

12ms       ┌─ Observer 2: Notification Service
           ├─ Condition eval: "status == 'new'" ✓
           ├─ Actions: [Slack]
           └─ Start: Slack (100ms)

14ms       ┌─ Observer 3: Search Indexing
           ├─ Condition eval: true (no condition)
           ├─ Actions: [SearchIndex]
           └─ Start: Index to Elasticsearch

61ms       ┌─ Elasticsearch index complete
           └─ Record metric: action_duration_search = 47ms

110ms      ┌─ Slack action complete
           ├─ Result cached for 30s
           └─ Record metric: action_duration_slack = 98ms

115ms      ┌─ Email action enqueued
           ├─ Job created in job_queue table
           ├─ JobQueueWorker will process async
           └─ Return to observer immediately

515ms      ┌─ Webhook action complete (with retry)
           ├─ Result cached for 30s
           ├─ Circuit breaker: Closed
           └─ Record metric: action_duration_webhook = 504ms

520ms      ┌─ All synchronous actions complete
           ├─ Build execution summary:
           │  ├─ Observers: 3
           │  ├─ Actions: 4 (3 sync + 1 queued)
           │  ├─ Successful: 3
           │  ├─ Queued: 1
           │  ├─ Duration: 510ms
           │  └─ Metrics: 8 recorded
           └─ Event processing complete for observers

525ms      ┌─ Post-processing:
           ├─ Index event to Elasticsearch
           └─ Record Prometheus metrics

[Meanwhile, async] ┌─ JobQueueWorker picks up email job
                   ├─ Dequeue from job_queue table
                   ├─ Execute: Send email via SMTP
                   ├─ Retry if transient error
                   └─ Mark completed or move to DLQ

1050ms     ┌─ Checkpoint saved
           ├─ Update observer_checkpoints:
           │  └─ last_processed_id = 1050
           ├─ History recorded
           └─ Ready for restart recovery
```

---

## 🔄 State Machine: Circuit Breaker

```
                    ┌─────────────────┐
                    │     CLOSED      │
                    │  (Normal ops)   │
                    └────────┬────────┘
                             │
                      ┌──────▼──────┐
                      │ Track calls  │
                      └──────┬──────┘
                             │
                    ┌────────▼────────┐
                    │  Consecutive    │
                    │  failures >= N? │
                    └─────┬──────┬────┘
                          │      │
                       YES│      │NO
                          │      └─────────┐
                          │                │
                    ┌─────▼────────┐      ▼ (Success)
                    │     OPEN     │       (reset counter)
                    │ (Reject all) │
                    └─────┬────────┘
                          │
                   ┌──────▼──────┐
                   │   Timeout?  │
                   │  Expired?   │
                   └─────┬───┬───┘
                         │   │
                      YES│   │NO
                         │   └─► (wait)
                         │
                    ┌────▼───────────┐
                    │   HALF-OPEN    │
                    │ (Test recovery)│
                    └────┬───────┬───┘
                         │       │
                    ┌────▼┐   ┌──▼────┐
                    │Test │   │ Still │
                    │pass?│   │fails? │
                    └────┬┘   └──┬────┘
                         │       │
                      YES│       │NO
                         │       └──► OPEN (restart timeout)
                         │
                    ┌────▼──────────┐
                    │     CLOSED    │
                    │  (recovered)  │
                    └───────────────┘
```

---

## 🏛️ Multi-Listener Coordination

```
┌─────────────────────────────────────────────────────────────┐
│              PostgreSQL Database                            │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  observer_checkpoints (shared state)                       │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ listener_id    | last_id | updated_at              │   │
│  ├────────────────┼─────────┼─────────────────────────┤   │
│  │ listener-app1  | 5000    | 2026-01-22 14:35:12 UTC │   │
│  │ listener-app2  | 5000    | 2026-01-22 14:35:11 UTC │   │
│  │ listener-app3  | 4999    | 2026-01-22 14:34:50 UTC │   │
│  └─────────────────────────────────────────────────────┘   │
│       ▲                                    ▲               │
│       │                                    │               │
│  ┌────┴─ Listener 1 reads               ──┴──┐            │
│  │      (app-instance-1, node-1)             │            │
│  │      Last read: ID 5000                   │            │
│  │      Processing: Events 5001-5100        │            │
│  │      (will save checkpoint)               │            │
│  │                                           │            │
│  │  ┌──────────────────────────────────┐    │            │
│  └──► Listener 2 reads                 │    │            │
│       (app-instance-2, node-2)         │    │            │
│       Last read: ID 5000               │    │            │
│       Processing: Events 5001-5100     │    │            │
│       (will save checkpoint)            │    │            │
│                                        │    │            │
│  ┌────────────────────────────────────┼──► Listener 3 reads
│  │ (app-instance-3, node-3)           │    (app-instance-3, node-3)
│  │ Last read: ID 4999                 │    CRASHED! ✗
│  │ Processing: Events 5000-5099       │    (will be ignored, checkpoint
│  │ (will save checkpoint)             │     shows ID 4999 - stale)
│  │                                    │
│  └────────────────────────────────────┘
│
│
│ CHECKPOINT UPDATES (Atomic):
│ ─────────────────────────────
│ 1. Listener-1 finishes batch 5100 ──► UPDATE checkpoint SET last_id=5100
│                                        WHERE listener_id='listener-app1'
│ 2. Listener-2 finishes batch 5100 ──► UPDATE checkpoint SET last_id=5100
│                                        WHERE listener_id='listener-app2'
│ 3. Listener-3 was processing 5099 ──► (stale, won't save if crashed before update)
│
│ If all listeners crash:
│ ─────────────────────
│ On restart, max(checkpoint.last_id) = 5100
│ Next poll starts from 5100 (no duplicate processing)
│
└─────────────────────────────────────────────────────────────┘
```

---

## 💾 Caching Strategy

```
Event: Order INSERT (ID: 550e8400)
│
▼ Webhook Action
┌─────────────────────────────────┐
│ 1. Build cache key              │
│ key = "observer:action:Order:   │
│        550e8400:webhook:123"    │
└────────────────┬────────────────┘
                 │
    ┌────────────▼────────────┐
    │ Check Cache (Redis)      │
    └────────────┬────┬───────┘
                 │    │
            HIT  │    │ MISS
                 │    │
            ┌────▼┐  ┌▼────────┐
            │Return
            │cached│  │Execute │
            │result│  │webhook │
            └──────┘  │(500ms) │
                      └────┬───┘
                           │
                  ┌────────▼────────┐
                  │Cache result     │
                  │with TTL=30s     │
                  │                 │
                  │Key expires:     │
                  │TTL=30s (FIFO)   │
                  │                 │
                  │Invalidation:    │
                  │When Order       │
                  │updated          │
                  └─────────────────┘


Pattern invalidation (batch):
────────────────────────────
DELETE from cache:
  observer:action:Order:550e8400:*

Result:
- Webhook cache cleared
- Slack cache cleared
- Email cache cleared
- (Others unaffected)
- Next webhook executes fresh
```

---

## 📈 Metrics Architecture

```
Prometheus Scrape Interval: 15s
│
▼
┌─────────────────────────────────────────┐
│  ObserverMetrics (Gauges, Counters, Histograms)
├─────────────────────────────────────────┤
│ COUNTERS (only increase):               │
│ - events_processed_total                │
│ - observers_matched_total               │
│ - actions_executed_total (per type)    │
│ - dlq_items_total                       │
│                                         │
│ GAUGES (can go up/down):                │
│ - dlq_items_pending                     │
│ - listener_backoff_level                │
│ - listener_consecutive_errors           │
│ - jobs_pending                          │
│ - jobs_processing                       │
│                                         │
│ HISTOGRAMS (track distribution):        │
│ - event_processing_duration_seconds     │
│ - action_duration_seconds (per type)   │
│ - job_processing_duration_seconds       │
│ - cache_lookup_duration_seconds         │
│                                         │
│ CUSTOM METRICS:                         │
│ - cache_hit_rate (% of lookups)        │
│ - checkpoint_save_duration_ms           │
│ - circuit_breaker_state (per endpoint)  │
└──────────────┬───────────────────────────┘
               │
               ▼
        ┌──────────────┐
        │ Prometheus   │
        │ Time-Series  │
        │ Database     │
        └──────┬───────┘
               │
      ┌────────┴────────┐
      │                 │
      ▼                 ▼
  ┌────────┐      ┌─────────────┐
  │Grafana │      │AlertManager │
  │Dashbrd │      │(alerting)   │
  └────────┘      └─────────────┘
      ▲
      │ Queries (PromQL)
      │
   Examples:
   - rate(events_processed_total[1m])
   - histogram_quantile(0.99, action_duration_seconds)
   - cache_hit_rate
   - dlq_items_pending > 100 (alert)
```

---

## 🔄 Job Queue Lifecycle

```
Event: Order INSERT
│
├─ Webhook: Execute (fast)
├─ Cache invalidation: Execute (fast)
└─ Email: QUEUE (slow)
        │
        ▼
    ┌─────────────────────────────────────┐
    │ Job Creation                        │
    ├─────────────────────────────────────┤
    │ {                                   │
    │   id: "a1b2c3d4",                   │
    │   queue_name: "email",              │
    │   status: "pending",                │
    │   event: { ... },                   │
    │   action: { ... },                  │
    │   retry_count: 0,                   │
    │   priority: 5,                      │
    │   created_at: "2026-01-22 14:35:12" │
    │ }                                   │
    └─────────────────────────────────────┘
            │
            ▼ INSERT into observer_jobs

    ┌──────────────────────────────┐
    │ PostgreSQL Job Queue Table   │
    ├──────────────────────────────┤
    │ Status: pending              │
    │ Retry: 0/3                   │
    │ Worker: (null)               │
    │ Created: 2026-01-22 14:35:12 │
    └──────────────────────────────┘
            │
    ┌───────┴────────┬────────┬────────┐
    │                │        │        │
    ▼                ▼        ▼        ▼
 WORKER-1        WORKER-2  WORKER-3  WORKER-4
 (polling)       (idle)    (polling)  (idle)
    │
    ├─ SELECT * FROM observer_jobs
    │  WHERE status='pending' AND queue_name='email'
    │  ORDER BY priority DESC, created_at ASC
    │  LIMIT 1
    │  FOR UPDATE SKIP LOCKED
    │
    ▼
 Got job a1b2c3d4
    │
    ├─ UPDATE status='processing', worker_id='worker-1'
    │
    ▼
 Execute Action (Send Email)
    │
    ├─ Success ✓
    │  ├─ UPDATE status='completed'
    │  ├─ UPDATE completed_at = NOW()
    │  └─ DELETE from queue
    │
    └─ Failure (transient)
       ├─ IF retry_count < max_retries:
       │  ├─ Create new job with retry_count++
       │  └─ Priority bumped (gets processed sooner)
       │
       └─ Failure (permanent)
          ├─ UPDATE status='failed'
          ├─ UPDATE error_message = '...'
          └─ Manual retry needed (via CLI)
```

---

## 🎯 Deduplication Window

```
Event Timestamp: 2026-01-22T14:35:12Z
Entity: Order 550e8400
Type: INSERT

Dedup Key: "order:550e8400:insert:1234567890"
                                  ▲
                                  │
                    60-second bucket (window alignment)

Window: 2026-01-22T14:35:00Z to 2026-01-22T14:35:59Z
        ├─────────────────────────────────────┤
        │ Event comes in at 14:35:12          │
        │ Mark as seen: Redis SETEX 300s      │
        │ (5 minute TTL)                       │
        └─────────────────────────────────────┘

Second occurrence of same event at 14:35:35Z:
        ├─────────────────────────────────────┤
        │ Check Redis key: EXISTS              │
        │ YES → Skip processing                │
        │ Save: 1 unnecessary webhook call     │
        └─────────────────────────────────────┘

After TTL expires at 14:40:12Z:
        Redis key: EXPIRED (automatic)

New identical event at 14:40:15Z:
        ├─────────────────────────────────────┤
        │ Check Redis key: NOT FOUND           │
        │ Process normally                     │
        │ Window duration: ~5 minutes          │
        └─────────────────────────────────────┘
```

---

## Summary: Phase 8 = Astonishing Framework ✨

Every component designed for:
- ✅ **Reliability**: Zero data loss, automatic recovery
- ✅ **Performance**: Concurrent, caching, async processing
- ✅ **Observability**: Metrics, search, debugging tools
- ✅ **Scalability**: Multi-listener, job workers, distributed state
- ✅ **Developer Experience**: Clear APIs, helpful errors, CLI tools

**Ready to build the framework developers dream about.** 🚀


# Phase 13, Cycle 3 - RED: Audit Logging & Storage Requirements

**Date**: February 15, 2026
**Phase Lead**: Security Lead
**Status**: RED (Defining Audit Logging Requirements)

---

## Objective

Define comprehensive audit logging requirements for FraiseQL v2, specifying what to log, how to store it securely, how to detect tampering, and how to search historical data for incident investigation.

---

## Background: Why Audit Logging?

From Phase 13, Cycle 1 threat modeling:
- **Threat 3.1 (Repudiation)**: User denies query execution → Need audit trail
- **Threat 2.3 (Tampering)**: Audit log tampering → Need tamper detection
- **OWASP #10 (Insufficient Logging)**: No breach detection → Need monitoring integration

Audit logging provides:
1. **Accountability**: Who did what, when, where
2. **Compliance**: Evidence for regulations (GDPR, HIPAA, SOC2, PCI-DSS)
3. **Forensics**: Investigate breaches retroactively
4. **Detection**: Feed anomaly detection engine
5. **Immutability**: Tamper detection prevents cover-ups

---

## Audit Logging Architecture

### Two-Tier Storage Strategy

```
┌─────────────────────────────────────────────────────────┐
│                  Application Layer                       │
│         (writes audit events in real-time)              │
└────────────────┬────────────────┬──────────────────────┘
                 │                │
        ┌────────▼────────┐  ┌────▼─────────────┐
        │  S3 (Immutable) │  │ Kafka (Stream)   │
        │ (append-only)   │  │ (temporary queue)│
        │ WRITE-ONCE      │  └────┬─────────────┘
        │ With HMAC       │       │
        └────────┬────────┘       │
                 │         ┌──────▼──────────────┐
                 │         │  Elasticsearch      │
                 │         │  (searchable index) │
                 │         │  (read-only replica)│
                 │         └─────────────────────┘
                 │
        ┌────────▼────────────────────────┐
        │  Long-term Cold Storage         │
        │  (Glacier/Archive, 7 years)     │
        └─────────────────────────────────┘
```

**Why Two Tiers?**
- **S3**: Immutable, write-once, tamper-proof (primary record)
- **Elasticsearch**: Fast searchable access (replica for investigation)
- **Kafka**: Stream for real-time anomaly detection
- **Glacier**: Long-term compliance retention

---

## What to Log: Event Categories

### Category 1: Query Execution Logs (High Volume)

**When**: Every GraphQL query executed
**What**:
```json
{
  "timestamp": "2026-02-15T10:23:45.123Z",
  "event_type": "query_executed",
  "api_key_id": "fraiseql_us_east_1_abc123",
  "request_id": "req_xyz789",
  "query_hash": "sha256_of_normalized_query",
  "query_size_bytes": 256,
  "query_complexity_score": 1250,
  "execution_time_ms": 45,
  "result_rows": 100,
  "result_size_bytes": 8192,
  "status": "success",
  "error_code": null,
  "client_ip": "203.0.113.42",
  "user_agent": "fraiseql-js/3.2.1"
}
```

**Rationale**: Need to track query performance and resource usage for anomaly detection

**Volume Estimate**:
- 1,000 req/s × 86,400 sec/day = 86.4M events/day
- At ~300 bytes per event = 25.9 GB/day
- 90 days hot storage = 2.3 TB
- 7 years cold storage = 67 TB (Glacier)

**Indexing**: By timestamp, api_key_id, query_hash

---

### Category 2: Authentication Events (Medium Volume)

**When**: Every API key validation attempt
**What**:
```json
{
  "timestamp": "2026-02-15T10:23:45.123Z",
  "event_type": "auth_attempt",
  "api_key_id": "fraiseql_us_east_1_abc123",
  "request_id": "req_xyz789",
  "status": "success",
  "failure_reason": null,
  "client_ip": "203.0.113.42",
  "key_version": 1,
  "key_age_days": 45
}
```

**Success Cases**:
- Valid key accepted
- Expired key (revoked)
- Key not found

**Failure Cases**:
- Invalid key format
- Wrong signature
- Key revoked
- KMS decryption failed

**Volume Estimate**: Same as query execution (~86M/day for 100% auth hit rate)

**Alert Thresholds**:
- >10 failures from same IP in 1 minute
- >100 failures for same key in 1 hour
- >1000 failures globally in 1 minute

---

### Category 3: Authorization Events (Medium Volume)

**When**: Every field-level authorization check
**What**:
```json
{
  "timestamp": "2026-02-15T10:23:45.123Z",
  "event_type": "authz_check",
  "api_key_id": "fraiseql_us_east_1_abc123",
  "request_id": "req_xyz789",
  "resource_type": "users",
  "resource_id": "user_456",
  "field_name": "email",
  "permission_required": "read:pii",
  "permission_granted": true,
  "user_role": "analyst"
}
```

**Rationale**: Track access patterns to detect data exfiltration

**Alert Thresholds**:
- Denied permission on field accessed successfully yesterday
- New fields accessed by API key
- Permission check failures for same key >10 in 1 minute

---

### Category 4: API Key Lifecycle Events (Low Volume)

**When**: Key creation, rotation, revocation, expiration
**What**:
```json
{
  "timestamp": "2026-02-15T10:23:45.123Z",
  "event_type": "api_key_operation",
  "operation": "created",
  "api_key_id": "fraiseql_us_east_1_abc123",
  "tier": "premium",
  "permissions": ["query:read", "batch:100"],
  "created_by": "user_admin_001",
  "created_by_ip": "203.0.113.100",
  "expires_at": "2026-05-16T10:23:45Z"
}
```

**Operations**:
- `created`: New API key generated
- `rotated`: Key automatically rotated
- `revoked`: Key manually revoked
- `expired`: Key passed expiration date
- `accessed_after_rotation`: Old key used during grace period

**Rationale**: Full key lifecycle audit

---

### Category 5: Security Events (Low Volume)

**When**: Security-relevant operations
**What**:
```json
{
  "timestamp": "2026-02-15T10:23:45.123Z",
  "event_type": "security_event",
  "severity": "high",
  "alert_type": "rate_limit_exceeded",
  "api_key_id": "fraiseql_us_east_1_abc123",
  "details": {
    "limit": 1000,
    "actual": 1250,
    "window": "1_minute",
    "client_ip": "203.0.113.42"
  },
  "action_taken": "rate_limited"
}
```

**Alert Types**:
- `rate_limit_exceeded`: Query rate >1.5x baseline
- `brute_force_attempt`: Multiple failed auth attempts
- `anomaly_detected`: ML anomaly detection triggered
- `privilege_escalation_attempt`: Token used for unauthorized scope
- `data_exfiltration_suspected`: Unusual data volume accessed

---

### Category 6: Configuration Changes (Low Volume)

**When**: Admin changes (schema updates, feature flags, etc.)
**What**:
```json
{
  "timestamp": "2026-02-15T10:23:45.123Z",
  "event_type": "config_change",
  "resource": "schema",
  "operation": "deployed",
  "changed_by": "user_admin_001",
  "changed_by_ip": "203.0.113.100",
  "changes": {
    "types_added": ["User", "Order"],
    "types_modified": ["Product"],
    "deprecations": []
  },
  "approval_status": "approved"
}
```

---

## Log Format Specification

### Format: JSON Lines (One Event Per Line)

**Example Stream**:
```
{"timestamp":"2026-02-15T10:23:45.123Z","event_type":"query_executed","api_key_id":"fraiseql_us_east_1_abc123",...}\n
{"timestamp":"2026-02-15T10:23:46.456Z","event_type":"auth_attempt","api_key_id":"fraiseql_us_east_1_abc123",...}\n
```

**Rationale**:
- One event per line → easy streaming
- JSON → structured, queryable
- No compression in S3 → faster access for forensics

### Common Fields (All Events)

| Field | Type | Required | Purpose |
|-------|------|----------|---------|
| `timestamp` | ISO8601 | Yes | When event occurred (UTC) |
| `event_type` | String | Yes | Category (query_executed, auth_attempt, etc.) |
| `api_key_id` | String | Yes | Which API key made request |
| `request_id` | String | Yes | Correlate with application logs |
| `client_ip` | String | Yes | IP address of client |
| `trace_id` | String | Optional | For distributed tracing |

### Sensitive Data Handling

**Never Log**:
- Plaintext API key material
- Database passwords or credentials
- PII fields from query results
- Full query text (use hash instead)
- Raw response data (use size instead)

**Always Log**:
- Query hash (SHA256 of normalized query)
- Query complexity score (not content)
- Result size in bytes (not data)
- Authorization decisions (who was checked, not data)
- Error codes (not full error messages)

---

## Storage Architecture

### Tier 1: S3 (Primary, Immutable, Compliance)

**Bucket Configuration**:
```
Bucket: fraiseql-audit-logs-primary-{region}
Region: us-east-1 (primary), with cross-region replication
Encryption: SSE-S3 (managed by AWS)
Versioning: Enabled
Block Public Access: Yes (all checkboxes enabled)
```

**Object Key Structure**:
```
s3://fraiseql-audit-logs-primary-us-east-1/
  ├── 2026/02/15/
  │   ├── 10/
  │   │   ├── 23/
  │   │   │   ├── 2026-02-15T10-23-00Z.jsonl
  │   │   │   ├── 2026-02-15T10-23-01Z.jsonl
  │   │   │   └── 2026-02-15T10-23-02Z.jsonl  (1 file per second)
  │   │   ├── 24/
  │   │   └── 25/
  │   └── 11/
  └── 2026/02/16/
```

**File Format**:
- Filename: `YYYY-MM-DDTHH-MM-SSZ.jsonl`
- Content: Gzip-compressed JSON Lines
- Size: ~10-50 MB per file (1 second of logs)
- Retention: 90 days hot, then move to Glacier

**Write Strategy**:
```
1. Application writes events to in-memory buffer
2. Every 1 second (or 100K events), flush to S3
3. Each flush = 1 atomic PUT operation
4. If PUT fails, retry with backoff
5. Never overwrite existing objects (append-only)
```

**Cost Estimate**:
- 86.4M events/day × 300 bytes = 25.9 GB/day
- S3 Standard: ~$0.023 per GB/month = $0.017/day
- 90 days × 25.9 GB = 2,331 GB × $0.023 = $53.61/month
- Glacier: $0.004/GB/month = $15 for 7-year archive

---

### Tier 2: Elasticsearch (Searchable, Hot Tier)

**Cluster Configuration**:
```
Cluster: fraiseql-audit-{region}-prod
Nodes: 3 (high availability)
Per-node: 16 GB heap, 256 GB storage
Replication: 1 (data × 2 copies minimum)
Shards: 10 per index (balance between query parallelism and overhead)
```

**Index Strategy**:
```
Index per day: fraiseql-audit-logs-2026.02.15
  ├── Shards: 10
  ├── Replicas: 1
  ├── TTL: 90 days (then delete index)
  └── Refresh: 30 seconds (balance between freshness and performance)
```

**Field Mappings**:
```json
{
  "mappings": {
    "properties": {
      "timestamp": { "type": "date" },
      "event_type": { "type": "keyword" },
      "api_key_id": { "type": "keyword" },
      "request_id": { "type": "keyword" },
      "client_ip": { "type": "ip" },
      "status": { "type": "keyword" },
      "query_complexity_score": { "type": "integer" },
      "execution_time_ms": { "type": "integer" },
      "result_rows": { "type": "integer" }
    }
  }
}
```

**Write Strategy**:
```
1. S3 file written and finalized
2. Elasticsearch ingestion job reads S3 file
3. Bulk index events into ES (1000 at a time)
4. Index refresh (make searchable)
5. Process logs: ~100M events/day = 100s to index
```

**Query Examples**:
```
// Find all queries from API key
GET /fraiseql-audit-logs-*/_search
{
  "query": {
    "term": { "api_key_id": "fraiseql_us_east_1_abc123" }
  }
}

// Find all failed auth attempts
GET /fraiseql-audit-logs-*/_search
{
  "query": {
    "bool": {
      "must": [
        { "term": { "event_type": "auth_attempt" } },
        { "term": { "status": "failure" } }
      ]
    }
  }
}

// Find queries taking >1 second
GET /fraiseql-audit-logs-*/_search
{
  "query": {
    "range": { "execution_time_ms": { "gte": 1000 } }
  }
}
```

**Cost Estimate**:
- 3-node cluster, 16GB per node = 48GB total
- AWS Elasticsearch: ~$4,000/month for managed cluster
- Justification: <2s search latency for forensics queries

---

### Tier 3: Kafka (Stream, Real-time Anomaly)

**Purpose**: Feed events to anomaly detection in real-time

**Topic Configuration**:
```
Topic: fraiseql-audit-log-stream
Partitions: 10 (one per ES shard, for scaling)
Replication Factor: 3 (HA)
Retention: 24 hours (anomaly detection window)
```

**Consumers**:
```
1. Anomaly Detection Service
   - Reads in real-time
   - Computes rolling baselines
   - Triggers alerts (Phase 13, Cycle 4)

2. Elasticsearch Indexer
   - Reads from Kafka
   - Batches into ES bulk operations
```

**Cost Estimate**:
- AWS MSK (Managed Streaming Kafka): ~$2,000/month
- Or: Self-hosted Kafka on EC2: ~$500/month

---

## Tamper Detection: HMAC Signing

### Problem
An attacker with database/S3 access could delete logs or modify them to cover tracks.

### Solution
**HMAC-SHA256 signing** of log batches with a key stored separately (AWS KMS).

### Design

**Signing Strategy**:
```
1. Collect N events (e.g., 1000 events)
2. Create "batch" { events: [...], batch_number: 1, timestamp: ... }
3. Sign batch with HMAC-SHA256:
   key = AWS KMS key (never in code)
   message = serialize(batch as JSON)
   signature = HMAC-SHA256(key, message)
4. Write to S3:
   {
     "batch_number": 1,
     "start_timestamp": "2026-02-15T10:00:00Z",
     "end_timestamp": "2026-02-15T10:00:05Z",
     "event_count": 1000,
     "events": [...],
     "signature": "abc123def456..."
   }
5. Store next batch's hash in current batch (chain):
   "next_batch_hash": SHA256(next batch)
```

**Verification Process**:
```
1. Load batch from S3
2. Recompute HMAC-SHA256 with KMS key
3. Compare stored signature with computed signature
4. Verify next_batch_hash points to actual next batch
5. If mismatch, raise alert (tampering detected)
```

**Chain of Custody**:
```
Batch 1: signature_1, next_hash_1 → Batch 2
Batch 2: signature_2, next_hash_2 → Batch 3
Batch 3: signature_3, next_hash_3 → Batch 4
...
```

If attacker modifies Batch 2:
- Signature verification fails
- Hash chain breaks
- Tampering detected immediately

**Performance**:
- HMAC-SHA256: <1ms per batch (1000 events)
- KMS sign call: ~20ms
- Overhead: Negligible (<0.1% of log writing time)

---

## Retention Policy

### Hot Storage (90 Days)
- **Location**: S3 Standard + Elasticsearch
- **Access**: Fast (milliseconds)
- **Cost**: Higher ($0.023/GB/month)
- **Use Case**: Incident investigation, forensics
- **Compliance**: SOC2, GDPR, HIPAA

### Warm Storage (30 Days - 1 Year)
- **Location**: S3 Standard-IA (Infrequent Access)
- **Access**: Slower (seconds)
- **Cost**: Lower ($0.0125/GB/month)
- **Use Case**: Historical analysis, trend detection

### Cold Storage (1-7 Years)
- **Location**: Glacier Deep Archive
- **Access**: Very slow (hours)
- **Cost**: Lowest ($0.00099/GB/month)
- **Use Case**: Compliance retention, legal hold
- **Retrieval**: 12-48 hour turnaround

**Transition Policy**:
```
Day 0-90:   S3 Standard + Elasticsearch (hot)
Day 91-365: S3 Standard-IA (warm)
Day 366+:   Glacier Deep Archive (cold)
Day 2555+:  Delete (7 years)
```

---

## Testing Strategy

### Unit Tests

**Test 1: Event Serialization**
```rust
#[test]
fn test_audit_event_to_json() {
    let event = AuditEvent::QueryExecuted {
        timestamp: Utc::now(),
        api_key_id: "test_key",
        query_hash: "abc123",
        // ...
    };

    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("query_executed"));
    assert!(json.contains("test_key"));
    assert!(!json.contains("plaintext"));
}
```

**Test 2: HMAC Signing**
```rust
#[tokio::test]
async fn test_batch_signing() {
    let batch = LogBatch { events: vec![...], batch_number: 1 };
    let kms = MockKMS::new();

    let signed = batch.sign(&kms).await.unwrap();
    assert!(signed.signature.len() > 0);

    // Verify
    let verified = signed.verify(&kms).await.unwrap();
    assert!(verified);
}
```

**Test 3: No Plaintext in Logs**
```rust
#[test]
fn test_no_plaintext_in_events() {
    let api_key = "fraiseql_us_east_1_abc123_secret";
    let event = AuditEvent::AuthAttempt {
        api_key_id: "fraiseql_us_east_1_abc123",
        // ...
    };

    let json = serde_json::to_string(&event).unwrap();
    assert!(!json.contains("secret"));
    assert!(!json.contains(&api_key));
}
```

### Integration Tests

**Test 4: S3 Write & Read**
```rust
#[tokio::test]
#[ignore]  // Requires AWS credentials
async fn test_write_to_s3() {
    let s3 = S3Client::new("us-east-1").await.unwrap();
    let batch = create_test_batch(1000);

    let key = s3.write_batch(&batch).await.unwrap();
    assert!(key.contains("2026/02/15"));

    let read_back = s3.read_batch(&key).await.unwrap();
    assert_eq!(read_back.events.len(), 1000);
}
```

**Test 5: Elasticsearch Indexing**
```rust
#[tokio::test]
#[ignore]  // Requires Elasticsearch
async fn test_index_to_elasticsearch() {
    let es = ElasticsearchClient::new("localhost:9200").await.unwrap();
    let events = vec![...];

    es.bulk_index("fraiseql-audit-logs-2026.02.15", events).await.unwrap();

    // Search
    let results = es.search("api_key_id: test_key").await.unwrap();
    assert!(results.hits.total.value > 0);
}
```

### Performance Tests

**Test 6: Latency**
```rust
#[bench]
fn bench_serialize_event(b: &mut Bencher) {
    let event = AuditEvent::QueryExecuted { ... };
    b.iter(|| serde_json::to_string(&event));
}
// Target: <1ms per event serialization
```

**Test 7: Throughput**
```rust
#[tokio::test]
async fn test_write_throughput() {
    let start = Instant::now();
    for i in 0..100_000 {
        write_audit_event(generate_test_event(i)).await.ok();
    }
    let elapsed = start.elapsed();

    let throughput = 100_000 / elapsed.as_secs();
    assert!(throughput > 10_000);  // >10k events/sec
}
```

---

## Success Criteria

### RED Phase (This Phase)
- [x] Event categories defined (6 types)
- [x] Log format specified (JSON Lines)
- [x] Storage architecture documented (S3 + ES + Kafka)
- [x] Retention policy defined (90 days hot → 7 years cold)
- [x] Tamper detection designed (HMAC-SHA256 signing)
- [x] Testing strategy complete (7 test cases)
- [x] Performance targets defined (>10k events/sec)

### GREEN Phase (Next)
- [ ] Audit event types implemented
- [ ] S3 writer implemented
- [ ] Elasticsearch indexer implemented
- [ ] HMAC signing implemented
- [ ] Tests passing
- [ ] No plaintext in logs

### REFACTOR Phase
- [ ] Performance validated (>10k events/sec)
- [ ] Tamper detection verified
- [ ] S3 immutability verified
- [ ] Elasticsearch queries working

### CLEANUP Phase
- [ ] Linting clean
- [ ] Documentation complete
- [ ] Ready for Phase 13, Cycle 4 (Anomaly Detection)

---

## External Dependencies

### AWS Services Required
- **S3**: Log storage (primary)
- **Glacier**: Long-term retention
- **KMS**: HMAC signing key
- **Elasticsearch Service** (or self-hosted): Log indexing
- **Kafka** (MSK or self-hosted): Stream for anomaly detection

### Rust Dependencies
- `serde` - JSON serialization
- `tokio` - Async runtime
- `aws-sdk-s3` - S3 client
- `elasticsearch` - Elasticsearch client
- `rdkafka` - Kafka client
- `sha2` - SHA256 hashing
- `hmac` - HMAC-SHA256 signing
- `chrono` - Timestamp handling

---

## Risk Assessment

### Risk 1: Log Volume Exceeds Capacity
- **Risk**: 86.4M events/day might overwhelm S3 or Elasticsearch
- **Mitigation**: Partition by time (hourly), batch writes, compression
- **Contingency**: Archive older data more aggressively

### Risk 2: Tamper Detection Bypass
- **Risk**: Attacker could forge HMAC if KMS key leaked
- **Mitigation**: KMS key never in code, AWS IAM restricted
- **Contingency**: Periodic audit of signed batches

### Risk 3: Elasticsearch Availability
- **Risk**: If ES down, can't search logs during incident
- **Mitigation**: S3 is primary (always searchable), ES is replica
- **Contingency**: Manual S3 forensics if ES unavailable

### Risk 4: GDPR "Right to be Forgotten"
- **Risk**: Can't delete logs (immutable), conflicts with GDPR
- **Mitigation**: Hash user IDs (not stored directly), data retention policy
- **Contingency**: Legal review, data masking procedures

---

## Next Steps

### Immediate (Phase 13, Cycle 3 GREEN)
1. Implement audit event types (6 categories)
2. Implement S3 writer (batching, compression)
3. Implement Elasticsearch indexer
4. Implement HMAC signing (KMS integration)
5. Get tests green

### Short-term (Phase 13, Cycle 3 REFACTOR)
1. Performance validation (>10k events/sec)
2. Tamper detection verification
3. S3 immutability verification

### Medium-term (Phase 13, Cycle 3 CLEANUP)
1. Linting clean
2. Documentation complete
3. Ready for Cycle 4 (Anomaly Detection)

---

**RED Phase Status**: ✅ READY FOR IMPLEMENTATION
**Ready for**: GREEN Phase (Audit Logging Implementation)
**Target Date**: February 15-16, 2026


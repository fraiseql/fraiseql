# Phase 13, Cycle 4 - RED: Anomaly Detection & Response Requirements

**Date**: February 17, 2026
**Phase Lead**: Security Lead
**Status**: RED (Defining Anomaly Detection Requirements)

---

## Objective

Define comprehensive anomaly detection system for FraiseQL v2, specifying detection rules, baseline calculations, alert conditions, and incident response procedures for real-time threat identification.

---

## Background: Why Anomaly Detection?

From Phase 13, Cycle 1 threat modeling:
- **Threat 5.3 (DoS via Rate Limiting)**: Attacks may come from legitimate-looking traffic
- **Threat 4.1 (Data Exfiltration)**: Slow, low-rate attacks escape notice
- **Threat 3.1 (Repudiation)**: Breaches detected retroactively from audit logs

Anomaly detection provides:
1. **Real-time Detection**: Alert within seconds of attack start
2. **Behavioral Baseline**: Normal for API key A ≠ normal for API key B
3. **Slow Attack Detection**: Multi-day exfiltration at 1% above normal
4. **Compliance**: SOC2, GDPR breach notification requirements
5. **Incident Response**: Quick action reduces damage

---

## Anomaly Detection Architecture

### Three-Tier Detection Strategy

```
┌─────────────────────────────────────────────────┐
│            Kafka Stream (Real-time)             │
│   Audit events from Phase 13, Cycle 3           │
└────────────────┬────────────────────────────────┘
                 │
        ┌────────▼────────────────┐
        │  Tier 1: Real-time      │
        │  Anomaly Detection      │
        │  (milliseconds)         │
        │  - Spike detection      │
        │  - Threshold violations │
        │  - Pattern matching     │
        └────────┬────────────────┘
                 │
        ┌────────▼─────────────────────┐
        │  Tier 2: Batch Analysis     │
        │  (hourly)                    │
        │  - Statistical anomalies     │
        │  - Trend changes            │
        │  - Correlation analysis     │
        └────────┬─────────────────────┘
                 │
        ┌────────▼──────────────────────┐
        │  Tier 3: Historical Deep Dive │
        │  (daily)                       │
        │  - Pattern recognition        │
        │  - Breach forensics           │
        │  - Root cause analysis        │
        └────────┬──────────────────────┘
                 │
        ┌────────▼─────────────────────────┐
        │  Alert & Incident Response       │
        │  - Slack/PagerDuty              │
        │  - Create incident ticket       │
        │  - Notify security team         │
        │  - Auto-remediate (optional)    │
        └────────────────────────────────┘
```

---

## Detection Rules

### Category 1: Query Rate Anomalies (Tier 1 & 2)

**Rule 1.1: Per-API-Key Rate Spike**

**Trigger**:
- Baseline: 95th percentile of queries/minute for API key over past 14 days
- Alert threshold: >1.5× baseline for 5 consecutive minutes
- Example:
  - API key normally peaks at 500 queries/min
  - Baseline = 95th percentile = 450 queries/min
  - Alert if >675 queries/min for ≥5 minutes

**Detection Method**:
```
baseline = calculate_p95(historical_query_rates[api_key_id][14 days])
current_rate = sum(queries[api_key_id] in last minute)

if current_rate > baseline × 1.5:
  increment anomaly_counter
  if anomaly_counter >= 5:
    trigger_alert("rate_spike", api_key_id, current_rate, baseline)
```

**Whitelisting**:
- Scheduled jobs (known high-volume keys) are configured with different thresholds
- Allowlist for known batch operations
- Override: 24-hour exception if approved by admin

**False Positive Handling**:
- First alert: Notify (don't auto-remediate)
- If no security event follows: Lower threshold by 10% next day
- If legitimate: Add to whitelist

**Severity**: MEDIUM → HIGH if combined with auth failures

---

**Rule 1.2: Global Rate Spike**

**Trigger**:
- Baseline: 95th percentile of total queries/second across all API keys (14 days)
- Alert threshold: >1.5× baseline for 2 minutes
- Example:
  - System normally handles 8,500 qps peak
  - Baseline = 8,000 qps
  - Alert if >12,000 qps for ≥2 minutes

**Action**:
- Alert: "Possible DDoS attack"
- If combined with same-IP pattern: Auto rate-limit that IP

**Severity**: MEDIUM → HIGH if sustained

---

### Category 2: Query Complexity Anomalies (Tier 1)

**Rule 2.1: High Complexity Threshold**

**Trigger**:
- Alert if query complexity score >1,500 (approaching max 2,000)
- Alert if 10+ consecutive high-complexity queries in 30 seconds
- Example:
  - Single query: 1,800 complexity points (out of 2,000 max)
  - Indicates: Sophisticated query designed to exhaust resources

**Detection Method**:
```
for each query:
  if complexity_score > 1500:
    log_warning("high_complexity_query", api_key_id, score)
    if count_in_window(30s) > 10:
      trigger_alert("complexity_attack", api_key_id)
```

**Action**:
- First occurrence: Log and alert (observe mode)
- If pattern continues: Rate-limit key

**Severity**: LOW → MEDIUM if sustained

---

### Category 3: Authorization Anomalies (Tier 1 & 2)

**Rule 3.1: Failed Authorization Spike**

**Trigger**:
- >10 failed authz attempts per minute from single API key
- >100 failed authz attempts globally per minute
- Example:
  - API key normally has 0-1 failures/min
  - Suddenly: 50 failures/min
  - Indicates: Privilege escalation attempt or permission mismatch

**Detection Method**:
```
failed_authz_per_key = count(authz_check[status=denied]) in last minute
if failed_authz_per_key[api_key_id] > 10:
  trigger_alert("authz_spike", api_key_id, count)
```

**Action**:
- Alert: "Authorization failures detected"
- If combined with query spike: Suspect data exfiltration attempt
- Recommend: Review key permissions, consider rotation

**Severity**: MEDIUM

---

**Rule 3.2: New Field Access**

**Trigger**:
- API key accesses field it has never accessed before
- Example:
  - API key historically: queries User.id, User.name
  - New: queries User.email, User.ssn (PII fields)
  - Indicates: Possible data exfiltration attempt or credential theft

**Detection Method**:
```
field_baseline = set of fields accessed by api_key_id in past 14 days
current_fields = set of fields accessed by api_key_id in last 1 hour

new_fields = current_fields - field_baseline
if len(new_fields) > 0:
  trigger_alert("new_field_access", api_key_id, new_fields)
```

**Action**:
- Alert: "New fields accessed"
- Security review: Is access legitimate?
- Consider: Field access policy violation

**Severity**: MEDIUM → HIGH if PII fields

---

### Category 4: Data Access Anomalies (Tier 2)

**Rule 4.1: Data Volume Anomaly**

**Trigger**:
- API key retrieves significantly more rows than baseline
- Baseline: 95th percentile of rows returned in single query (14 days)
- Alert: >3× baseline or >10,000 rows (whichever is larger)
- Example:
  - API key baseline: max 500 rows per query
  - Sudden query: 15,000 rows
  - Indicates: Possible bulk exfiltration

**Detection Method**:
```
baseline = calculate_p95(result_rows[api_key_id][14 days])
current_query_rows = result_rows[api_key_id] in last query

if current_query_rows > max(baseline × 3, 10000):
  trigger_alert("data_volume_anomaly", api_key_id, rows, baseline)
```

**Action**:
- Alert: "Unusual data volume"
- Review query: Is it legitimate?
- Consider: Rate-limit if suspicious

**Severity**: HIGH (potential data breach)

---

**Rule 4.2: Cross-Table Data Access**

**Trigger**:
- API key accesses multiple unrelated tables in short time window
- Example:
  - Normally accesses: User table only
  - Suddenly: User, Order, Payment, InventoryAdjustment (all in 5 min)
  - Indicates: Lateral movement / privilege escalation

**Detection Method**:
```
tables_baseline = set of tables accessed by api_key_id in past 14 days
current_tables = set of tables accessed by api_key_id in last 5 minutes

new_tables = current_tables - tables_baseline
if len(new_tables) > 2:
  trigger_alert("cross_table_access", api_key_id, new_tables)
```

**Action**:
- Alert: "Unusual table access pattern"
- Investigate: Why does this key need these tables?
- Consider: Revoke access if not legitimate

**Severity**: HIGH (likely lateral movement)

---

### Category 5: Authentication Anomalies (Tier 1)

**Rule 5.1: Failed Authentication Spike**

**Trigger**:
- >10 failed authentication attempts per minute from same IP
- >100 globally per minute
- Example:
  - API key attempts wrong signature 15 times in 60 seconds
  - Indicates: Brute force attack or credential stuffing

**Detection Method**:
```
failed_auth_per_ip = count(auth_attempt[status=failure]) by client_ip in last minute
if failed_auth_per_ip[ip] > 10:
  trigger_alert("auth_brute_force", ip, count)
```

**Action**:
- Immediate: Rate-limit IP to 1 request/minute
- Alert: "Brute force attack detected"
- After 100 failures: Auto-block IP for 24 hours

**Severity**: MEDIUM → HIGH

---

**Rule 5.2: Unknown API Key Pattern**

**Trigger**:
- Valid API key used from unusual geographic location
- Example:
  - API key registered in US
  - Suddenly used from China (different timezone, ping time)
  - Indicates: Possible credential theft

**Detection Method**:
```
historical_geos = set of countries used by api_key_id
current_geo = geoip(client_ip)

if current_geo not in historical_geos:
  if no VPN/proxy detected:
    trigger_alert("unusual_geo", api_key_id, current_geo)
```

**Note**: Requires GeoIP database and VPN detection service

**Action**:
- Alert: "API key used from unusual location"
- Recommend: Verify with customer
- Consider: Require re-authentication

**Severity**: MEDIUM

---

### Category 6: System Health Anomalies (Tier 1 & 2)

**Rule 6.1: Query Latency Spike**

**Trigger**:
- Query execution time >3× baseline for single query
- Baseline: p95 execution time (14 days)
- Example:
  - Query normally: 50ms p95
  - Sudden query: 500ms
  - Indicates: Database issue or resource contention

**Detection Method**:
```
baseline = calculate_p95(execution_time[14 days])
current_query_time = execution_time[last query]

if current_query_time > baseline × 3:
  trigger_alert("latency_spike", query_hash, time, baseline)
```

**Action**:
- Alert: "Query latency spike"
- Recommend: Investigate database performance
- Not security issue, but operational concern

**Severity**: LOW (operational)

---

**Rule 6.2: Error Rate Spike**

**Trigger**:
- >5% of queries failing when baseline is <1%
- >50 errors per minute (regardless of baseline)
- Example:
  - Baseline: 0.5% errors
  - Sudden: 8% errors
  - Indicates: Code bug, infrastructure issue, or attack

**Detection Method**:
```
baseline_error_rate = count(errors) / count(total_queries) over 14 days
current_error_rate = count(errors in last minute) / count(queries in last minute)

if current_error_rate > baseline × 5 or current_error_rate > 0.05:
  trigger_alert("error_rate_spike", rate, baseline)
```

**Action**:
- Alert: "High error rate detected"
- Investigate: Is it code bug or attack?
- If attack-like: Rate-limit suspicious keys

**Severity**: MEDIUM

---

## Baseline Calculation

### Baseline Source Data
- **Window**: Past 14 days of audit logs (from Phase 13, Cycle 3)
- **Frequency**: Recalculated daily at 2 AM (off-peak)
- **Granularity**: Per API key, per user role, global

### Calculation Method

**Percentile Baselines**:
```
P95 = sort(values)[count × 0.95]
P99 = sort(values)[count × 0.99]
```

**Per-API-Key Baseline**:
```
for each api_key_id:
  baseline[api_key_id] = {
    query_rate_p95: calculate_p95(query_rate for last 14 days),
    query_time_p95: calculate_p95(execution_time for last 14 days),
    result_rows_p95: calculate_p95(result_rows for last 14 days),
    authz_failures: count(authz_failures for last 14 days) / (14 × 1440),
    tables_accessed: set of unique tables accessed,
    fields_accessed: set of unique fields accessed,
  }
```

**Cold Start Problem**:
- New API key (< 7 days data): Use global baseline
- New API key (7-14 days data): Use weighted average (key 50%, global 50%)
- New API key (> 14 days data): Use key baseline

**Baseline Override**:
- Scheduled jobs: Admin-configured multiplier (e.g., 5× normal)
- Known maintenance: Disable alerting for specific keys (time window)
- Gradual ramp: New service gradual increase over 7 days

---

## Alert Conditions & Severity

### Alert Priority Matrix

| Severity | Response Time | Action | Examples |
|----------|---|---|---|
| **CRITICAL** | < 2 minutes | Page on-call security | Multiple rules triggered simultaneously |
| **HIGH** | < 15 minutes | Slack notification | Data exfiltration likely, auth compromise |
| **MEDIUM** | < 1 hour | Create incident ticket | Suspicious pattern, requires investigation |
| **LOW** | Daily summary | Log and review | Performance issue, informational alert |

### Alert Escalation

**Tier 1** (Immediate):
- Auth brute force (Rule 5.1): >20 failures in 1 minute
- Data volume spike + auth failures combined
- Multiple rules triggered in 5 minutes

**Tier 2** (1 hour):
- Single high-confidence rule triggered
- Medium severity, needs investigation

**Tier 3** (Daily):
- Informational alerts
- Performance metrics
- Trend analysis

---

## Incident Response Procedures

### Phase 1: Detection (Automated)

```
Anomaly Detected
  ↓
Trigger Alert (Slack, PagerDuty)
  ↓
Create Incident Ticket
  ↓
Log Full Context (audit events + anomaly details)
  ↓
Escalate if CRITICAL or cascading alerts
```

### Phase 2: Investigation (Manual, 15-30 min)

**Security Team**:
1. Acknowledge alert
2. Review audit logs in Elasticsearch
3. Determine if legitimate or attack
4. Check API key permissions and recent usage
5. Communicate with customer if applicable

**Decision Tree**:
```
Is it legitimate?
  → YES: Update whitelist/baseline, close ticket
  → NO: Proceed to response
  → UNKNOWN: Investigate further, maintain observation mode
```

### Phase 3: Response (Varies by threat)

**If Brute Force (Rule 5.1)**:
1. Rate-limit IP to 1 request/min
2. Alert customer
3. Recommend IP allowlist

**If Rate Spike (Rule 1.1)**:
1. Check if scheduled batch job
2. If yes: Allowlist, update baseline
3. If no: Rate-limit key temporarily
4. Investigate with customer

**If Data Exfiltration Likely (Rules 4.1 + 4.2)**:
1. **CRITICAL**: Revoke API key immediately
2. Alert customer: "Key may be compromised"
3. Require new key generation
4. Forensics: Review all queries with that key
5. Estimate data exposure
6. File breach notification (if PII accessed)

**If Auth Compromise Suspected (Rule 5.2 + auth failures)**:
1. Rate-limit key
2. Require re-authentication
3. Alert customer
4. Log evidence for forensics

### Phase 4: Remediation (Ongoing)

- Update baseline if legitimate spike
- Create post-mortem if security incident
- Update alert rules if false positives
- Implement preventive measures

---

## False Positive Handling

### Feedback Loop

**Algorithm**:
```
for each alert:
  if human_decides == "false_positive":
    confidence_score -= 5% (for that rule)
    if confidence_score < 50%:
      disable rule, manual review needed
  else if human_decides == "confirmed_attack":
    confidence_score += 10%
```

### Configuration

**Per-Rule Tuning**:
- `threshold_multiplier`: Adjust sensitivity (1.5x → 2.0x)
- `window_size`: Change detection window (5 min → 10 min)
- `confidence_minimum`: Only alert if confidence > X%

**Learning Mode** (Optional):
- First 7 days: Log alerts but don't notify
- Review false positives
- Adjust thresholds
- Enable notifications

---

## Testing Strategy

### Unit Tests

**Test 1: Baseline Calculation**
```rust
#[test]
fn test_baseline_calculation() {
    let values = vec![10, 20, 30, ..., 1000];  // 100 values
    let p95 = calculate_percentile(&values, 0.95);
    assert_eq!(p95, 955);  // p95 of 0-1000
}
```

**Test 2: Anomaly Detection**
```rust
#[test]
fn test_rate_spike_detection() {
    let baseline = 500;  // 500 queries/min
    let current = 800;   // 800 queries/min
    let should_alert = is_anomaly(current, baseline, 1.5);
    assert!(should_alert);
}
```

**Test 3: False Positive Feedback**
```rust
#[test]
fn test_feedback_adjusts_threshold() {
    let mut rule = create_rule();
    assert_eq!(rule.confidence, 100);

    rule.feedback_false_positive();
    assert_eq!(rule.confidence, 95);

    for _ in 0..10 {
        rule.feedback_false_positive();
    }
    assert_eq!(rule.confidence, 50);  // Disable
}
```

### Integration Tests

**Test 4: Kafka Consumption**
```rust
#[tokio::test]
async fn test_kafka_event_processing() {
    let detector = create_detector();
    let event = create_test_audit_event();

    let alerts = detector.process_event(event).await.unwrap();
    assert_eq!(alerts.len(), 0);  // Single event shouldn't trigger
}
```

**Test 5: Alert Generation**
```rust
#[tokio::test]
async fn test_alert_on_anomaly() {
    let detector = create_detector();

    // Simulate 10 consecutive high-rate events
    for i in 0..10 {
        let event = create_high_rate_event();
        let alerts = detector.process_event(event).await.unwrap();

        if i < 4 {
            assert_eq!(alerts.len(), 0);
        } else {
            assert_eq!(alerts[0].alert_type, "rate_spike");
        }
    }
}
```

---

## Success Criteria

### RED Phase (This Phase)
- [x] 6 detection rules defined (rules 1.1-6.2)
- [x] Baseline calculation method specified
- [x] Alert conditions and severity defined
- [x] Incident response procedures documented
- [x] False positive handling algorithm defined
- [x] Testing strategy complete (5 test cases)
- [x] Cold start problem addressed
- [x] Integration with Kafka specified

### GREEN Phase (Next)
- [ ] Anomaly detection engine implemented
- [ ] Kafka consumer working
- [ ] Baseline calculator working
- [ ] Alert generator working
- [ ] Tests passing
- [ ] Integration with alert system (Slack, PagerDuty)

### REFACTOR Phase
- [ ] False positive rates validated (<5%)
- [ ] Alert accuracy tuned
- [ ] Performance validated (anomaly detection <1s latency)
- [ ] Baseline stability verified

### CLEANUP Phase
- [ ] Linting clean
- [ ] Documentation complete
- [ ] Ready for Phase 13, Cycle 5 (Penetration Testing)

---

## External Dependencies

### Runtime
- **Kafka**: Event stream (from Cycle 3)
- **Elasticsearch**: Historical baseline data
- **Redis**: Baseline cache, alert dedupe
- **Slack/PagerDuty**: Alert notifications

### Rust Crates
- `rdkafka` - Kafka consumer
- `elasticsearch` - Query baselines
- `redis` - Cache
- `tokio` - Async runtime
- `serde_json` - Data structures

---

## Risk Assessment

### Risk 1: False Positives
- **Risk**: Too many alerts → alert fatigue → missed real alerts
- **Mitigation**: Conservative thresholds, feedback loop, learning mode
- **Target**: <5% false positive rate

### Risk 2: False Negatives
- **Risk**: Slow attacks escape detection
- **Mitigation**: Multiple rules (spike + baseline + cross-table)
- **Target**: >95% detection rate

### Risk 3: Baseline Pollution
- **Risk**: Attacker's baseline includes their attack
- **Mitigation**: 14-day rolling window, excluding outliers, manual review
- **Contingency**: Recompute baseline on suspicious key

---

**RED Phase Status**: ✅ READY FOR IMPLEMENTATION
**Ready for**: GREEN Phase (Anomaly Detection Engine)
**Target Date**: February 17-18, 2026


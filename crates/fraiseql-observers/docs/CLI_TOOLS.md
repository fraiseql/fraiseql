# CLI Tools - Phase 8.10

The FraiseQL Observer CLI provides commands for runtime status monitoring, event debugging, dead letter queue management, configuration validation, and metrics inspection.

## Installation

```bash
# Build from source
cd crates/fraiseql-observers
cargo install --path . --bin fraiseql-observers

# Verify installation
fraiseql-observers --version
```

## Global Options

All commands support these global options:

```bash
--config FILE          Path to observer configuration file (optional)
--verbose, -v          Enable verbose logging
--format FORMAT        Output format: text or json (default: text)
--help, -h            Show help message
```

## Commands

### 1. Status Command

Display observer runtime status and listener health.

#### Syntax

```bash
fraiseql-observers status [OPTIONS]
```

#### Options

```
--listener ID         Show specific listener status
--detailed, -d        Show detailed information
```

#### Examples

```bash
# Basic status
fraiseql-observers status

# Detailed status with all listeners
fraiseql-observers status --detailed

# Check specific listener
fraiseql-observers status --listener listener-1

# JSON output
fraiseql-observers status --format json
```

#### Output

```
Observer Runtime Status
Leader: listener-1 (3/3 healthy)

Listeners:
  1. listener-1 [running] ✓
  2. listener-2 [running] ✓
  3. listener-3 [running] ✓
```

---

### 2. Debug-Event Command

Inspect event processing and observer matching.

#### Syntax

```bash
fraiseql-observers debug-event [OPTIONS]
```

#### Options

```
--event-id ID         Inspect specific event
--history N           Show recent N events
--entity-type TYPE    Filter by entity type (Order, User, etc.)
--kind KIND           Filter by kind (created, updated, deleted)
--format FORMAT       Output format (text or json)
```

#### Examples

```bash
# Debug specific event
fraiseql-observers debug-event --event-id evt-123

# Show last 10 Order events
fraiseql-observers debug-event --entity-type Order --history 10

# Show created events
fraiseql-observers debug-event --kind created

# JSON output for scripting
fraiseql-observers debug-event --event-id evt-123 --format json | jq '.matched_observers'
```

#### Output

```
Event Details
Event ID: evt-12345
Entity: Order (00000000-0000-0000-0000-000000000001)
Kind: created
Timestamp: 2026-01-22T12:00:00Z

Data:
  id: 00000000-0000-0000-0000-000000000001
  status: new
  total: 250.00
  customer_id: cust-123

Matched Observers: ✓
  1. Email Order Notification [webhook] ✓
     Action 1: email [success] 150ms

  2. Webhook Order Logger [webhook] ✓
     Action 1: webhook [success] 85ms

Unmatched Observers: ⚠
  1. High Value Order Handler (Condition false)
```

---

### 3. DLQ Commands

Manage dead letter queue (failed actions).

#### Syntax

```bash
fraiseql-observers dlq <SUBCOMMAND> [OPTIONS]
```

#### Subcommands

##### 3.1 List

List failed items in dead letter queue.

```bash
fraiseql-observers dlq list [OPTIONS]

OPTIONS:
  --limit N           Show N items (default: 10)
  --offset N          Skip first N items (for pagination)
  --observer NAME     Filter by observer name
  --after TIMESTAMP   Show items after timestamp (ISO 8601)
  --format FORMAT     Output format
```

**Examples**:

```bash
# List recent 20 failures
fraiseql-observers dlq list --limit 20

# Pagination
fraiseql-observers dlq list --limit 10 --offset 10  # Items 11-20

# Filter by observer
fraiseql-observers dlq list --observer obs-webhook

# Show JSON
fraiseql-observers dlq list --format json
```

**Output**:

```
Dead Letter Queue Items
Total: 3
Showing: 10

dlq-001
  Observer: obs-webhook-logger
  Error: Connection timeout
  Retries: 3/5
  Timestamp: 2026-01-22T10:30:00Z

dlq-002
  Observer: obs-email-notifier
  Error: Invalid email address
  Retries: 1/5
  Timestamp: 2026-01-22T11:15:00Z

dlq-003
  Observer: obs-slack-notifier
  Error: Rate limit exceeded
  Retries: 2/5
  Timestamp: 2026-01-22T11:45:00Z
```

##### 3.2 Show

Display details of a specific DLQ item.

```bash
fraiseql-observers dlq show <ITEM_ID> [OPTIONS]

ARGUMENTS:
  ITEM_ID             ID of the DLQ item
```

**Examples**:

```bash
# Show item details
fraiseql-observers dlq show dlq-001

# JSON format
fraiseql-observers dlq show dlq-001 --format json
```

**Output**:

```
DLQ Item Details
ID: dlq-001
Observer: obs-webhook-logger
Event: evt-00001
Entity: Order (00000000-0000-0000-0000-000000000001)

Error:
  Message: Connection timeout after 30s
  Code: TIMEOUT

Retry Status:
  Attempts: 3/5
  Last Retry: 2026-01-22T10:35:00Z
  Next Retry: 2026-01-22T10:40:00Z
```

##### 3.3 Retry

Retry a specific DLQ item.

```bash
fraiseql-observers dlq retry <ITEM_ID> [OPTIONS]

OPTIONS:
  --force             Force retry regardless of max attempts
```

**Examples**:

```bash
# Retry single item
fraiseql-observers dlq retry dlq-001

# Force retry (even if max attempts exceeded)
fraiseql-observers dlq retry dlq-001 --force

# Check result
fraiseql-observers dlq show dlq-001 | grep "Attempt"
```

**Output**:

```
Retry Result
Item ID: dlq-001
Status: Success
Message: Item queued for retry
Attempt: 4/5
```

##### 3.4 RetryAll

Retry all items matching criteria.

```bash
fraiseql-observers dlq retry-all [OPTIONS]

OPTIONS:
  --observer NAME     Retry only from this observer
  --after TIMESTAMP   Retry items after timestamp
  --dry-run          Show what would be retried (don't actually retry)
```

**Examples**:

```bash
# Preview what would be retried
fraiseql-observers dlq retry-all --dry-run
# Output: "Would retry 15 items"

# Retry all webhook failures
fraiseql-observers dlq retry-all --observer obs-webhook

# Retry all failures from last 24 hours
fraiseql-observers dlq retry-all --after "$(date -u -d '24 hours ago' +%Y-%m-%dT%H:%M:%SZ)"

# Full batch retry
fraiseql-observers dlq retry-all
```

**Output**:

```
Batch Retry Result
Items Retried: 5
Items Failed: 0
Items Skipped: 2
Message: Batch retry completed
```

##### 3.5 Remove

Remove an item from the dead letter queue.

```bash
fraiseql-observers dlq remove <ITEM_ID> [OPTIONS]

OPTIONS:
  --force             Skip confirmation prompt
```

**Examples**:

```bash
# Remove with confirmation
fraiseql-observers dlq remove dlq-001
# Prompts: "Are you sure? (y/n)"

# Remove without confirmation
fraiseql-observers dlq remove dlq-001 --force
```

**Output**:

```
Remove Result
Item ID: dlq-001
Status: Removed
```

##### 3.6 Stats

Display dead letter queue statistics.

```bash
fraiseql-observers dlq stats [OPTIONS]

OPTIONS:
  --by-observer       Show breakdown by observer
  --by-error         Show breakdown by error type
  --format FORMAT    Output format
```

**Examples**:

```bash
# Overall DLQ statistics
fraiseql-observers dlq stats

# By observer
fraiseql-observers dlq stats --by-observer

# By error type
fraiseql-observers dlq stats --by-error

# Both breakdowns
fraiseql-observers dlq stats --by-observer --by-error

# JSON export
fraiseql-observers dlq stats --format json > dlq-stats.json
```

**Output**:

```
DLQ Statistics
Total Items: 15
Total Retries: 32
Failure Rate: 85%

By Observer:
  obs-webhook: 5
  obs-email: 7
  obs-slack: 3

By Error Type:
  timeout: 8
  invalid_input: 4
  rate_limit: 3
```

---

### 4. Validate-Config Command

Validate observer configuration.

#### Syntax

```bash
fraiseql-observers validate-config [OPTIONS] [FILE]

ARGUMENTS:
  FILE               Configuration file to validate (optional)

OPTIONS:
  --detailed, -d     Show detailed report
  --format FORMAT    Output format
```

#### Examples

```bash
# Validate configuration file
fraiseql-observers validate-config observers.yaml

# Detailed validation with per-observer status
fraiseql-observers validate-config observers.yaml --detailed

# Use global config
fraiseql-observers --config /etc/observers.yaml validate-config

# JSON output
fraiseql-observers validate-config observers.yaml --format json
```

#### Output

```
Configuration Validation Report

Status: Valid
Summary: 0 errors, 2 warnings

Issues:
  1. [warning] webhook.timeout_ms (line 42): Timeout is very high (60000ms).
              Consider reducing to improve responsiveness.
  2. [warning] email.retry_config.max_attempts (line 78): Max attempts is 10,
              which is unusual. Most systems use 3-5.

Observers:
  1. OrderNotifier [webhook] (1 action(s)) ✓
  2. EmailAlert [email] (1 action(s)) ✓

✓ Configuration is valid and ready for deployment
```

---

### 5. Metrics Command

Inspect Prometheus metrics.

#### Syntax

```bash
fraiseql-observers metrics [OPTIONS]
```

#### Options

```
--metric NAME       Show specific metric
--help              Show metric documentation
--format FORMAT     Output format
```

#### Examples

```bash
# Show all metrics
fraiseql-observers metrics

# Show specific metric
fraiseql-observers metrics --metric observer_events_processed_total

# Get help on metrics
fraiseql-observers metrics --help

# JSON export
fraiseql-observers metrics --format json > metrics.json
```

#### Available Metrics

```
Counters:
  observer_events_processed_total              Total events processed
  observer_events_matched_total                Total events matched by observers
  observer_actions_executed_total              Total actions executed
  observer_actions_failed_total                Total actions that failed

Gauges:
  observer_dlq_items_total                    Current items in DLQ
  observer_listener_health                    Listener health (1=healthy, 0=unhealthy)

Histograms:
  observer_action_duration_seconds            Action execution time distribution
  observer_event_processing_duration_seconds  Event processing time distribution
```

#### Output

```
Observer Metrics

observer_events_processed_total
  Type: counter
  Help: Total number of events processed
  Value: 12850

observer_actions_executed_total
  Type: counter
  Help: Total number of actions executed
  Value: 15234

observer_actions_failed_total
  Type: counter
  Help: Total number of actions that failed
  Value: 342

observer_dlq_items_total
  Type: gauge
  Help: Current number of items in DLQ
  Value: 28

observer_action_duration_seconds
  Type: histogram
  Buckets:
    0.01 s: 4521
    0.05 s: 8234
    0.1 s: 2156
    0.5 s: 256
    1.0 s: 45

Summary Statistics
Total Events: 12,850
Matched Events: 9,124
Match Rate: 70.9%
Failed Actions: 342
Failure Rate: 2.2%
DLQ Items: 28
```

---

## Common Workflows

### Workflow 1: Diagnose Processing Issues

```bash
# 1. Check system status
fraiseql-observers status

# 2. View recent metrics
fraiseql-observers metrics

# 3. Check DLQ for failures
fraiseql-observers dlq stats

# 4. Debug specific event
fraiseql-observers debug-event --event-id evt-123

# 5. Show DLQ items
fraiseql-observers dlq list --limit 5
```

### Workflow 2: Recover from Action Failures

```bash
# 1. See what failed
fraiseql-observers dlq stats --by-observer

# 2. Check details of first failure
fraiseql-observers dlq list --limit 1

# 3. Investigate error
fraiseql-observers dlq show dlq-001

# 4. Fix issue (e.g., update webhook URL)
# Edit configuration, redeploy

# 5. Retry with dry-run first
fraiseql-observers dlq retry-all --observer obs-webhook --dry-run

# 6. Actually retry
fraiseql-observers dlq retry-all --observer obs-webhook

# 7. Verify success
fraiseql-observers dlq stats
```

### Workflow 3: Performance Diagnosis

```bash
# 1. Check throughput
fraiseql-observers metrics --metric observer_events_processed_total

# 2. Check latency (P99)
fraiseql-observers metrics | grep action_duration

# 3. Check cache hit rate (if caching enabled)
fraiseql-observers metrics | grep cache_hit_rate

# 4. Debug a slow event
fraiseql-observers debug-event --history 10 \
  | grep -i "duration\|latency"

# 5. Review configuration
fraiseql-observers validate-config --detailed
```

### Workflow 4: Configuration Validation

```bash
# 1. Validate syntax
fraiseql-observers validate-config observers.yaml

# 2. Check for warnings
fraiseql-observers validate-config observers.yaml --detailed

# 3. Export validated config
fraiseql-observers validate-config observers.yaml --format json > validated.json

# 4. Deploy if valid
if fraiseql-observers validate-config observers.yaml; then
    docker-compose up -d
fi
```

---

## Integration with Scripts

### Bash Scripting

```bash
#!/bin/bash

# Health check script
status_json=$(fraiseql-observers status --format json)

healthy=$(echo "$status_json" | jq '.healthy_listeners')
total=$(echo "$status_json" | jq '.total_listeners')

if [ "$healthy" != "$total" ]; then
    echo "ALERT: Only $healthy/$total listeners healthy"
    exit 1
fi

echo "Health check passed"
```

### Python Integration

```python
import json
import subprocess

def get_dlq_stats():
    result = subprocess.run(
        ['fraiseql-observers', 'dlq', 'stats', '--format', 'json'],
        capture_output=True,
        text=True
    )
    return json.loads(result.stdout)

stats = get_dlq_stats()
print(f"DLQ has {stats['total_items']} items")
print(f"Failure rate: {stats['failure_rate']*100:.1f}%")
```

### Kubernetes Integration

```yaml
# readiness_probe.yaml
apiVersion: v1
kind: Pod
metadata:
  name: observer
spec:
  containers:
  - name: observer
    image: fraiseql-observer:latest
    readinessProbe:
      exec:
        command:
        - fraiseql-observers
        - status
      initialDelaySeconds: 5
      periodSeconds: 10
```

---

## Exit Codes

```
0   - Success
1   - General error
2   - Configuration error
3   - Connection error
4   - Validation failed
5   - Item not found
```

**Example**:

```bash
fraiseql-observers validate-config observers.yaml
if [ $? -eq 0 ]; then
    echo "Valid configuration"
else
    echo "Invalid configuration"
fi
```

---

## Environment Variables

```bash
# Database connection
DATABASE_URL=postgresql://user:pass@localhost/db

# Redis connection
REDIS_URL=redis://localhost:6379

# Elasticsearch
ELASTICSEARCH_URL=http://localhost:9200

# Logging
RUST_LOG=info
```

---

## Tips & Tricks

### Tip 1: Monitor Events in Real-Time

```bash
watch -n 1 'fraiseql-observers metrics --metric observer_events_processed_total'
```

### Tip 2: Find Issues Quickly

```bash
# Show failed actions
fraiseql-observers dlq list --format json | jq '.items[] | select(.error | length > 0)'

# Show slowest actions
fraiseql-observers metrics --format json | jq '.action_duration_seconds | max'
```

### Tip 3: Automate Retries

```bash
#!/bin/bash
# retry_failed.sh - Retry all failures every 5 minutes

while true; do
    fraiseql-observers dlq retry-all --observer obs-webhook --dry-run | grep -q "Would retry"
    if [ $? -eq 0 ]; then
        echo "Retrying failed items..."
        fraiseql-observers dlq retry-all --observer obs-webhook
    fi
    sleep 300
done
```

### Tip 4: Export Metrics

```bash
fraiseql-observers metrics --format json | jq . > metrics-export.json
```

---

## Troubleshooting

### "Connection refused" when running commands

```bash
# Verify observer is running
docker ps | grep observer

# Check port is open
curl http://localhost:8000/metrics

# Check logs
docker logs observer-listener
```

### "Configuration file not found"

```bash
# Verify file exists
ls -la observers.yaml

# Use absolute path
fraiseql-observers validate-config /full/path/observers.yaml
```

### JSON parsing errors

```bash
# Verify JSON output is valid
fraiseql-observers status --format json | jq .

# If error, check with grep
fraiseql-observers status --format json | head -20
```

---

## Next Steps

- Read Architecture Guide: `ARCHITECTURE_PHASE_8.md`
- Troubleshoot Issues: `TROUBLESHOOTING.md`
- Performance Tuning: `PERFORMANCE_TUNING.md`
- Integration Guide: `INTEGRATION_GUIDE.md`


# FraiseQL Observer System - Database Schema

This document describes the database schema for the FraiseQL observer system's Dead Letter Queue (DLQ) and event logging.

## Overview

The observer system uses three main tables to track events and handle failed action executions:

1. **observer_events** - Event audit log for all processed events
2. **observer_dlq_items** - Failed actions waiting for retry
3. **observer_dlq_history** - Audit trail of all retry attempts

## Tables

### observer_events

Stores all events processed by the observer system for debugging and audit trails.

| Column | Type | Constraints | Description |
|--------|------|-------------|-------------|
| id | UUID | PRIMARY KEY | Unique event identifier |
| event_type | VARCHAR(50) | NOT NULL | Event kind: INSERT, UPDATE, DELETE, CUSTOM |
| entity_type | VARCHAR(100) | NOT NULL | Entity type name (e.g., "Order", "User") |
| entity_id | UUID | NOT NULL | Entity instance ID |
| data | JSONB | NOT NULL | Full event data as JSON |
| created_at | TIMESTAMP | NOT NULL DEFAULT NOW() | When event was recorded |
| processed_at | TIMESTAMP | NULL | When event processing completed |
| status | VARCHAR(50) | DEFAULT 'pending' | pending, processing, completed, failed |

**Indexes:**
- `idx_observer_events_entity` - (entity_type, event_type) - for event lookup
- `idx_observer_events_status` - (status) - for status filtering
- `idx_observer_events_created` - (created_at) - for time-range queries

**Usage:**
```sql
-- Find all created order events in last hour
SELECT * FROM observer_events
WHERE entity_type = 'Order'
  AND event_type = 'INSERT'
  AND created_at > NOW() - INTERVAL '1 hour'
ORDER BY created_at DESC;

-- Find failed events
SELECT * FROM observer_events
WHERE status = 'failed'
ORDER BY created_at DESC;
```

### observer_dlq_items

Stores failed action executions for manual retry and debugging.

| Column | Type | Constraints | Description |
|--------|------|-------------|-------------|
| id | UUID | PRIMARY KEY | Unique DLQ item identifier |
| event_id | UUID | NOT NULL, FK | Reference to observer_events |
| action_type | VARCHAR(50) | NOT NULL | Action type: webhook, slack, email, sms, push, search, cache |
| action_config | JSONB | NOT NULL | Action configuration for retry |
| error_message | TEXT | NOT NULL | Error from the failure |
| attempt_count | INT | DEFAULT 1 | Number of attempts made |
| max_attempts | INT | DEFAULT 3 | Maximum retry attempts configured |
| created_at | TIMESTAMP | NOT NULL DEFAULT NOW() | When added to DLQ |
| last_retry_at | TIMESTAMP | NULL | When last retry was attempted |
| status | VARCHAR(50) | DEFAULT 'pending' | pending, processing, success, retry_failed, manually_resolved |

**Indexes:**
- `idx_observer_dlq_items_status` - (status) - for finding pending items
- `idx_observer_dlq_items_created` - (created_at) - for old items
- `idx_observer_dlq_items_action` - (action_type) - for action type filtering
- `idx_observer_dlq_items_event` - (event_id) - for finding items by event

**Status Values:**
- `pending` - Waiting for retry
- `processing` - Currently being retried
- `success` - Retry succeeded
- `retry_failed` - All retry attempts exhausted
- `manually_resolved` - Resolved by manual intervention

**Usage:**
```sql
-- Find all pending SMS failures
SELECT * FROM observer_dlq_items
WHERE action_type = 'sms'
  AND status = 'pending'
ORDER BY created_at ASC;

-- Find items that exceeded max attempts
SELECT * FROM observer_dlq_items
WHERE attempt_count >= max_attempts
  AND status != 'success'
ORDER BY created_at DESC;

-- Retry manually resolved item
UPDATE observer_dlq_items
SET status = 'pending', attempt_count = 1, last_retry_at = NULL
WHERE id = '...'
```

### observer_dlq_history

Tracks all retry attempts and their results for comprehensive audit trails.

| Column | Type | Constraints | Description |
|--------|------|-------------|-------------|
| id | BIGSERIAL | PRIMARY KEY | Auto-incrementing history ID |
| dlq_item_id | UUID | NOT NULL, FK | Reference to observer_dlq_items |
| attempt_number | INT | NOT NULL | Attempt number (1 = first, 2 = first retry) |
| error_message | TEXT | NOT NULL | Error message from this attempt |
| executed_at | TIMESTAMP | NOT NULL DEFAULT NOW() | When this attempt occurred |
| result | VARCHAR(50) | NOT NULL | success, transient_error, permanent_error, timeout |

**Indexes:**
- `idx_observer_dlq_history_item` - (dlq_item_id) - for finding history by item
- `idx_observer_dlq_history_result` - (result) - for finding failures
- `idx_observer_dlq_history_executed` - (executed_at) - for time queries

**Result Values:**
- `success` - Action executed successfully
- `transient_error` - Temporary error, retry should happen
- `permanent_error` - Error won't resolve with retry
- `timeout` - Action exceeded timeout

**Usage:**
```sql
-- View all attempts for a specific DLQ item
SELECT * FROM observer_dlq_history
WHERE dlq_item_id = '...'
ORDER BY attempt_number ASC;

-- Find permanent errors
SELECT dlq.id, dlq.action_type, hist.error_message
FROM observer_dlq_items dlq
JOIN observer_dlq_history hist ON dlq.id = hist.dlq_item_id
WHERE hist.result = 'permanent_error'
ORDER BY hist.executed_at DESC;
```

## Views

### observer_pending_retries

All items pending retry, joined with event information.

```sql
SELECT * FROM observer_pending_retries;
```

Returns: All pending DLQ items with entity and event type information, ordered by creation time.

### observer_retry_exhausted

Items where all retry attempts have been exhausted.

```sql
SELECT * FROM observer_retry_exhausted;
```

Returns: All items that failed all configured retry attempts.

### observer_recent_failures

Failures from the last 24 hours with retry attempt counts.

```sql
SELECT * FROM observer_recent_failures;
```

Returns: Recent failures with count of retry attempts, useful for monitoring dashboards.

## Migration

To apply this schema to your database:

```bash
# Using psql
psql -d your_database -f migrations/01_create_dlq_schema.sql

# Or with your migration tool (sqlx, diesel, etc.)
sqlx migrate run --database-url postgresql://...
```

## Maintenance

### Cleanup Old Records

Remove DLQ items older than 30 days that were successfully resolved:

```sql
DELETE FROM observer_dlq_items
WHERE created_at < NOW() - INTERVAL '30 days'
  AND status = 'success';

-- Also clean up their history (cascade will handle this automatically)
```

### Monitor DLQ Size

Check the size of DLQ tables:

```sql
SELECT
    'observer_events' as table_name,
    pg_size_pretty(pg_total_relation_size('observer_events')) as size
UNION ALL
SELECT
    'observer_dlq_items',
    pg_size_pretty(pg_total_relation_size('observer_dlq_items'))
UNION ALL
SELECT
    'observer_dlq_history',
    pg_size_pretty(pg_total_relation_size('observer_dlq_history'));
```

### Archive Old Records

For long-term storage, consider archiving:

```sql
-- Create archive table
CREATE TABLE observer_dlq_items_archive AS
SELECT * FROM observer_dlq_items
WHERE created_at < NOW() - INTERVAL '90 days'
  AND status = 'success';

-- Delete archived records
DELETE FROM observer_dlq_items
WHERE id IN (SELECT id FROM observer_dlq_items_archive);
```

## Performance Considerations

1. **Event Logging**: The observer_events table can grow quickly. Consider archiving based on age.
2. **DLQ Cleanup**: Successful DLQ items should be cleaned up regularly.
3. **History Retention**: Keep enough history for debugging but consider retention policies.
4. **Indexes**: All critical indexes are created for efficient querying.

## Application User Permissions

If using a separate application database user, grant appropriate permissions:

```sql
GRANT SELECT, INSERT, UPDATE ON observer_events TO app_user;
GRANT SELECT, INSERT, UPDATE ON observer_dlq_items TO app_user;
GRANT SELECT, INSERT ON observer_dlq_history TO app_user;
GRANT SELECT ON observer_pending_retries TO app_user;
GRANT SELECT ON observer_retry_exhausted TO app_user;
GRANT SELECT ON observer_recent_failures TO app_user;
```

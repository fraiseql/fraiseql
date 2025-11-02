# Extracted from: docs/core/fraiseql-philosophy.md
# Block number: 11
# Traces and metrics automatically stored in PostgreSQL
# Full correlation with errors and business events

SELECT
    e.message as error,
    t.duration_ms as trace_duration,
    c.entity_name as affected_entity
FROM monitoring.errors e
JOIN monitoring.traces t ON e.trace_id = t.trace_id
JOIN tb_entity_change_log c ON t.trace_id = c.trace_id::text
WHERE e.fingerprint = 'payment_processing_error'
ORDER BY e.occurred_at DESC
LIMIT 10;

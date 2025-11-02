# Extracted from: docs/production/monitoring.md
# Block number: 2
# Errors are automatically grouped by fingerprint
# Similar to Sentry's issue grouping

# Example: All "payment timeout" errors grouped together
SELECT
    fingerprint,
    COUNT(*) as occurrences,
    MAX(occurred_at) as last_seen,
    MIN(occurred_at) as first_seen
FROM monitoring.errors
WHERE environment = 'production'
  AND resolved_at IS NULL
GROUP BY fingerprint
ORDER BY occurrences DESC;

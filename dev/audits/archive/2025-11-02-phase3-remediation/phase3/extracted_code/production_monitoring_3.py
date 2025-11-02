# Extracted from: docs/production/monitoring.md
# Block number: 3
# Resolve errors
await tracker.resolve_error(fingerprint="payment_timeout_error")

# Ignore specific errors
await tracker.ignore_error(fingerprint="known_external_api_issue")

# Assign errors to team members
await tracker.assign_error(fingerprint="critical_bug", assignee="dev@example.com")

# Extracted from: docs/production/observability.md
# Block number: 14
# Only notify on new fingerprints
tracker.set_notification_rule("new_errors_only", notify_on_new_fingerprint=True)

# Rate limit notifications
tracker.set_notification_rule(
    "rate_limited",
    notify_on_occurrence=[1, 10, 100, 1000],  # 1st, 10th, 100th, 1000th
)

# Critical errors only
tracker.set_notification_rule(
    "critical_only", notify_when=lambda error: "critical" in error.context.get("severity", "")
)

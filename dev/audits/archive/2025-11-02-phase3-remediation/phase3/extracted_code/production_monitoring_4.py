# Extracted from: docs/production/monitoring.md
# Block number: 4
from fraiseql.monitoring.notifications import EmailNotifier, SlackNotifier

# Configure email notifications
email_notifier = EmailNotifier(
    smtp_host="smtp.gmail.com",
    smtp_port=587,
    from_email="alerts@myapp.com",
    to_emails=["team@myapp.com"],
)

# Configure Slack notifications
slack_notifier = SlackNotifier(webhook_url="https://hooks.slack.com/services/YOUR/WEBHOOK/URL")

# Add to tracker
tracker.add_notification_channel(email_notifier)
tracker.add_notification_channel(slack_notifier)

# Rate limiting: Only notify on first occurrence and every 100th occurrence
tracker.set_notification_rate_limit(
    fingerprint="payment_timeout_error",
    notify_on_occurrence=[1, 100, 200, 300],  # 1st, 100th, 200th, etc.
)

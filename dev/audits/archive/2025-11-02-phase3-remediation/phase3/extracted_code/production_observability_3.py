# Extracted from: docs/production/observability.md
# Block number: 3
from fraiseql.monitoring.notifications import EmailChannel, NotificationManager

# Configure email channel
email_channel = EmailChannel(
    smtp_host="smtp.gmail.com",
    smtp_port=587,
    smtp_user="alerts@myapp.com",
    smtp_password="app_password",
    use_tls=True,
    from_address="noreply@myapp.com",
)

# Create notification manager
notification_manager = NotificationManager(db_pool)
notification_manager.register_channel("email", lambda **kwargs: email_channel)

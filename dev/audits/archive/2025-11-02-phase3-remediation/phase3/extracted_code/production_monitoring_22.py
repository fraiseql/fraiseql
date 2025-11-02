# Extracted from: docs/production/monitoring.md
# Block number: 22
import httpx


async def send_pagerduty_alert(summary: str, severity: str, details: dict):
    """Send alert to PagerDuty."""
    payload = {
        "routing_key": os.getenv("PAGERDUTY_ROUTING_KEY"),
        "event_action": "trigger",
        "payload": {
            "summary": summary,
            "severity": severity,
            "source": "fraiseql",
            "custom_details": details,
        },
    }

    async with httpx.AsyncClient() as client:
        await client.post("https://events.pagerduty.com/v2/enqueue", json=payload)


# Example usage
if error_rate > 0.1:
    await send_pagerduty_alert(
        summary="High GraphQL error rate detected",
        severity="error",
        details={
            "error_rate": error_rate,
            "time_window": "5m",
            "affected_operations": ["getUser", "getOrders"],
        },
    )

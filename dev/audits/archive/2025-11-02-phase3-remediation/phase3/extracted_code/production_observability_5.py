# Extracted from: docs/production/observability.md
# Block number: 5
import httpx

from fraiseql.monitoring.notifications import NotificationManager


class TwilioSMSChannel:
    """SMS notification channel using Twilio."""

    def __init__(self, account_sid: str, auth_token: str, from_number: str):
        self.account_sid = account_sid
        self.auth_token = auth_token
        self.from_number = from_number

    async def send(self, error: dict, config: dict) -> tuple[bool, str | None]:
        """Send SMS notification."""
        try:
            to_number = config.get("to")
            if not to_number:
                return False, "No recipient phone number"

            message = self.format_message(error)

            async with httpx.AsyncClient() as client:
                response = await client.post(
                    f"https://api.twilio.com/2010-04-01/Accounts/{self.account_sid}/Messages.json",
                    auth=(self.account_sid, self.auth_token),
                    data={"From": self.from_number, "To": to_number, "Body": message},
                )

                if response.status_code == 201:
                    return True, None
                return False, f"Twilio API returned {response.status_code}"

        except Exception as e:
            return False, str(e)

    def format_message(self, error: dict, template: str | None = None) -> str:
        """Format error for SMS (160 char limit)."""
        return (
            f"🚨 {error['error_type']}: {error['error_message'][:80]}\n"
            f"Env: {error['environment']} | Count: {error['occurrence_count']}"
        )


# Register custom channel
notification_manager = NotificationManager(db_pool)
notification_manager.register_channel(
    "twilio_sms",
    lambda **config: TwilioSMSChannel(
        account_sid=config["account_sid"],
        auth_token=config["auth_token"],
        from_number=config["from_number"],
    ),
)

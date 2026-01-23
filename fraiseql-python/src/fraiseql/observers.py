"""
Observer authoring API for FraiseQL.

Observers react to database changes with configurable actions like webhooks,
Slack notifications, and emails.

Example:
    from fraiseql import type, observer, webhook

    @type
    class Order:
        id: int
        status: str
        total: float

    @observer(
        entity="Order",
        event="INSERT",
        condition="status == 'paid'",
        actions=[webhook("https://api.example.com/orders")]
    )
    def on_order_created():
        '''Triggered when a paid order is created.'''
        pass
"""

from dataclasses import dataclass, field
from typing import Any, Callable

from fraiseql.registry import SchemaRegistry


@dataclass
class RetryConfig:
    """Retry configuration for observer actions."""

    max_attempts: int = 3
    backoff_strategy: str = "exponential"  # exponential, linear, fixed
    initial_delay_ms: int = 100
    max_delay_ms: int = 60000

    def to_dict(self) -> dict[str, Any]:
        """Convert retry config to dictionary for schema generation."""
        return {
            "max_attempts": self.max_attempts,
            "backoff_strategy": self.backoff_strategy,
            "initial_delay_ms": self.initial_delay_ms,
            "max_delay_ms": self.max_delay_ms,
        }


@dataclass
class Observer:
    """Observer definition."""

    name: str
    entity: str
    event: str  # INSERT, UPDATE, DELETE
    actions: list[dict[str, Any]]
    condition: str | None = None
    retry: RetryConfig = field(default_factory=RetryConfig)

    def to_dict(self) -> dict[str, Any]:
        """Convert observer to dictionary for schema generation."""
        result = {
            "name": self.name,
            "entity": self.entity,
            "event": self.event.upper(),
            "actions": self.actions,
            "retry": self.retry.to_dict(),
        }
        if self.condition:
            result["condition"] = self.condition
        return result


def observer(
    entity: str,
    event: str,
    actions: list[dict[str, Any]],
    condition: str | None = None,
    retry: RetryConfig | None = None,
) -> Callable:
    """
    Decorator to define an observer.

    Observers react to database changes (INSERT, UPDATE, DELETE) with configurable
    actions like webhooks, Slack notifications, and emails.

    Args:
        entity: Entity type to observe (must match a @type definition)
        event: Event type (INSERT, UPDATE, or DELETE)
        actions: List of actions to execute (webhook, slack, email)
        condition: Optional condition expression (Python-like syntax)
        retry: Optional retry configuration

    Returns:
        Decorator function that marks the function as an observer

    Example:
        @observer(
            entity="Order",
            event="INSERT",
            condition="status == 'paid'",
            actions=[
                webhook("https://api.example.com/orders"),
                slack("#orders", "New order {id}: ${total}")
            ]
        )
        def on_order_created():
            '''Triggered when a paid order is created.'''
            pass

    Example with environment variables:
        @observer(
            entity="Order",
            event="UPDATE",
            condition="status.changed() and status == 'shipped'",
            actions=[webhook(url_env="SHIPPING_WEBHOOK_URL")]
        )
        def on_order_shipped():
            pass
    """

    def decorator(func: Callable) -> Callable:
        observer_obj = Observer(
            name=func.__name__,
            entity=entity,
            event=event,
            actions=actions,
            condition=condition,
            retry=retry or RetryConfig(),
        )

        # Attach observer to function (for direct access)
        func._fraiseql_observer = observer_obj

        # Register with schema registry (for export_schema)
        SchemaRegistry.register_observer(
            name=observer_obj.name,
            entity=observer_obj.entity,
            event=observer_obj.event,
            actions=observer_obj.actions,
            condition=observer_obj.condition,
            retry=observer_obj.retry.to_dict(),
        )

        return func

    return decorator


def webhook(
    url: str | None = None,
    url_env: str | None = None,
    headers: dict[str, str] | None = None,
    body_template: str | None = None,
) -> dict[str, Any]:
    """
    Define a webhook action.

    Args:
        url: Static webhook URL (or None if using url_env)
        url_env: Environment variable containing webhook URL
        headers: HTTP headers to send
        body_template: Optional Jinja2 template for request body

    Returns:
        Action configuration dictionary

    Raises:
        ValueError: If neither url nor url_env is provided

    Example:
        webhook("https://api.example.com/orders")

    Example with environment variable:
        webhook(url_env="ORDER_WEBHOOK_URL")

    Example with custom headers:
        webhook(
            "https://api.example.com/orders",
            headers={"Authorization": "Bearer {token}"}
        )

    Example with body template:
        webhook(
            "https://api.example.com/orders",
            body_template='{"order_id": "{{id}}", "total": {{total}}}'
        )
    """
    if url is None and url_env is None:
        msg = "Either url or url_env must be provided"
        raise ValueError(msg)

    action: dict[str, Any] = {
        "type": "webhook",
        "headers": headers or {"Content-Type": "application/json"},
    }

    if url:
        action["url"] = url
    if url_env:
        action["url_env"] = url_env
    if body_template:
        action["body_template"] = body_template

    return action


def slack(
    channel: str,
    message: str,
    webhook_url: str | None = None,
    webhook_url_env: str | None = "SLACK_WEBHOOK_URL",
) -> dict[str, Any]:
    """
    Define a Slack notification action.

    Args:
        channel: Slack channel (e.g., "#orders")
        message: Message template (supports {field} placeholders)
        webhook_url: Static Slack webhook URL
        webhook_url_env: Environment variable containing Slack webhook URL

    Returns:
        Action configuration dictionary

    Example:
        slack("#orders", "New order {id}: ${total}")

    Example with custom webhook:
        slack(
            "#orders",
            "New order {id}",
            webhook_url="https://hooks.slack.com/services/..."
        )

    Example with environment variable:
        slack(
            "#alerts",
            "Order {id} failed",
            webhook_url_env="SLACK_ALERTS_WEBHOOK"
        )
    """
    action: dict[str, Any] = {
        "type": "slack",
        "channel": channel,
        "message": message,
    }

    if webhook_url:
        action["webhook_url"] = webhook_url
    if webhook_url_env:
        action["webhook_url_env"] = webhook_url_env

    return action


def email(
    to: str,
    subject: str,
    body: str,
    from_email: str | None = None,
) -> dict[str, Any]:
    """
    Define an email action.

    Args:
        to: Recipient email address
        subject: Email subject (supports {field} placeholders)
        body: Email body (supports {field} placeholders)
        from_email: Sender email address

    Returns:
        Action configuration dictionary

    Example:
        email(
            to="admin@example.com",
            subject="Order {id} created",
            body="Order {id} for ${total} was created"
        )

    Example with sender:
        email(
            to="customer@example.com",
            subject="Your order {id} has shipped",
            body="Order {id} is on its way!",
            from_email="noreply@example.com"
        )
    """
    action: dict[str, Any] = {
        "type": "email",
        "to": to,
        "subject": subject,
        "body": body,
    }

    if from_email:
        action["from"] = from_email

    return action

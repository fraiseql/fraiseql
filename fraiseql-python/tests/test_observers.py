"""Tests for observer authoring API."""

import pytest

from fraiseql import email, observer, slack, webhook
from fraiseql.observers import Observer, RetryConfig


def test_observer_decorator():
    """Test that @observer decorator attaches metadata to function."""

    @observer(
        entity="Order", event="INSERT", actions=[webhook("https://example.com")]
    )
    def on_order():
        pass

    assert hasattr(on_order, "_fraiseql_observer")
    assert isinstance(on_order._fraiseql_observer, Observer)
    assert on_order._fraiseql_observer.entity == "Order"
    assert on_order._fraiseql_observer.event == "INSERT"
    assert on_order._fraiseql_observer.name == "on_order"


def test_observer_with_condition():
    """Test observer with condition expression."""

    @observer(
        entity="Order",
        event="UPDATE",
        condition="status == 'paid'",
        actions=[webhook("https://example.com")],
    )
    def on_order_paid():
        pass

    assert on_order_paid._fraiseql_observer.condition == "status == 'paid'"


def test_observer_with_custom_retry():
    """Test observer with custom retry configuration."""
    retry_config = RetryConfig(max_attempts=5, backoff_strategy="linear")

    @observer(
        entity="Order",
        event="INSERT",
        actions=[webhook("https://example.com")],
        retry=retry_config,
    )
    def on_order():
        pass

    assert on_order._fraiseql_observer.retry.max_attempts == 5
    assert on_order._fraiseql_observer.retry.backoff_strategy == "linear"


def test_observer_to_dict():
    """Test Observer.to_dict() serialization."""

    @observer(
        entity="Order",
        event="INSERT",
        condition="total > 100",
        actions=[webhook("https://example.com")],
    )
    def on_large_order():
        """Notify for large orders."""
        pass

    observer_dict = on_large_order._fraiseql_observer.to_dict()

    assert observer_dict["name"] == "on_large_order"
    assert observer_dict["entity"] == "Order"
    assert observer_dict["event"] == "INSERT"
    assert observer_dict["condition"] == "total > 100"
    assert len(observer_dict["actions"]) == 1
    assert observer_dict["actions"][0]["type"] == "webhook"
    assert "retry" in observer_dict


def test_webhook_action():
    """Test webhook action configuration."""
    action = webhook("https://example.com/orders")

    assert action["type"] == "webhook"
    assert action["url"] == "https://example.com/orders"
    assert action["headers"] == {"Content-Type": "application/json"}


def test_webhook_with_env_var():
    """Test webhook with environment variable."""
    action = webhook(url_env="ORDER_WEBHOOK_URL")

    assert action["type"] == "webhook"
    assert action["url_env"] == "ORDER_WEBHOOK_URL"
    assert "url" not in action


def test_webhook_with_custom_headers():
    """Test webhook with custom headers."""
    action = webhook(
        "https://example.com", headers={"Authorization": "Bearer token123"}
    )

    assert action["headers"]["Authorization"] == "Bearer token123"


def test_webhook_with_body_template():
    """Test webhook with custom body template."""
    action = webhook(
        "https://example.com", body_template='{"order_id": "{{id}}"}'
    )

    assert action["body_template"] == '{"order_id": "{{id}}"}'


def test_webhook_requires_url():
    """Test webhook raises error without URL."""
    with pytest.raises(ValueError, match="Either url or url_env must be provided"):
        webhook()


def test_slack_action():
    """Test Slack action configuration."""
    action = slack("#orders", "New order {id}: ${total}")

    assert action["type"] == "slack"
    assert action["channel"] == "#orders"
    assert action["message"] == "New order {id}: ${total}"
    assert action["webhook_url_env"] == "SLACK_WEBHOOK_URL"


def test_slack_with_custom_webhook():
    """Test Slack with custom webhook URL."""
    action = slack(
        "#orders",
        "New order",
        webhook_url="https://hooks.slack.com/services/XXX",
    )

    assert action["webhook_url"] == "https://hooks.slack.com/services/XXX"


def test_slack_with_custom_env_var():
    """Test Slack with custom environment variable."""
    action = slack(
        "#alerts", "Alert!", webhook_url_env="SLACK_ALERTS_WEBHOOK"
    )

    assert action["webhook_url_env"] == "SLACK_ALERTS_WEBHOOK"


def test_email_action():
    """Test email action configuration."""
    action = email(
        to="admin@example.com",
        subject="Order {id} created",
        body="Order {id} for ${total} was created",
    )

    assert action["type"] == "email"
    assert action["to"] == "admin@example.com"
    assert action["subject"] == "Order {id} created"
    assert action["body"] == "Order {id} for ${total} was created"


def test_email_with_from():
    """Test email with custom sender."""
    action = email(
        to="customer@example.com",
        subject="Order shipped",
        body="Your order is on its way!",
        from_email="noreply@example.com",
    )

    assert action["from"] == "noreply@example.com"


def test_retry_config_to_dict():
    """Test RetryConfig.to_dict() serialization."""
    retry = RetryConfig(
        max_attempts=5,
        backoff_strategy="linear",
        initial_delay_ms=200,
        max_delay_ms=30000,
    )

    retry_dict = retry.to_dict()

    assert retry_dict["max_attempts"] == 5
    assert retry_dict["backoff_strategy"] == "linear"
    assert retry_dict["initial_delay_ms"] == 200
    assert retry_dict["max_delay_ms"] == 30000


def test_multiple_actions():
    """Test observer with multiple actions."""

    @observer(
        entity="Order",
        event="INSERT",
        actions=[
            webhook("https://example.com/orders"),
            slack("#orders", "New order {id}"),
            email("admin@example.com", "Order created", "Order {id} created"),
        ],
    )
    def on_order():
        pass

    actions = on_order._fraiseql_observer.actions
    assert len(actions) == 3
    assert actions[0]["type"] == "webhook"
    assert actions[1]["type"] == "slack"
    assert actions[2]["type"] == "email"


def test_event_type_normalization():
    """Test that event types are normalized to uppercase."""

    @observer(
        entity="Order", event="insert", actions=[webhook("https://example.com")]
    )
    def on_order():
        pass

    observer_dict = on_order._fraiseql_observer.to_dict()
    assert observer_dict["event"] == "INSERT"

"""Tests for observer schema export functionality."""

import pytest

from fraiseql import email, observer, slack, webhook
from fraiseql.observers import RetryConfig
from fraiseql.registry import SchemaRegistry


@pytest.fixture(autouse=True)
def clear_registry():
    """Clear registry before each test."""
    SchemaRegistry.clear()
    yield
    SchemaRegistry.clear()


def test_observer_exports_to_schema():
    """Test that @observer decorator registers in schema."""

    @observer(
        entity="Order",
        event="INSERT",
        actions=[webhook("https://example.com/orders")],
    )
    def on_order_created():
        """Triggered when order is created."""
        pass

    schema = SchemaRegistry.get_schema()

    assert "observers" in schema
    assert len(schema["observers"]) == 1

    observer_def = schema["observers"][0]
    assert observer_def["name"] == "on_order_created"
    assert observer_def["entity"] == "Order"
    assert observer_def["event"] == "INSERT"
    assert len(observer_def["actions"]) == 1
    assert observer_def["actions"][0]["type"] == "webhook"


def test_multiple_observers_export():
    """Test that multiple observers export correctly."""

    @observer(
        entity="Order",
        event="INSERT",
        actions=[webhook("https://example.com/orders")],
    )
    def on_order_created():
        pass

    @observer(
        entity="Order",
        event="UPDATE",
        condition="status == 'shipped'",
        actions=[slack("#orders", "Order {id} shipped")],
    )
    def on_order_shipped():
        pass

    @observer(
        entity="User",
        event="INSERT",
        actions=[email("admin@example.com", "New user", "User {id} created")],
    )
    def on_user_created():
        pass

    schema = SchemaRegistry.get_schema()

    assert "observers" in schema
    assert len(schema["observers"]) == 3

    # Check names
    observer_names = [o["name"] for o in schema["observers"]]
    assert "on_order_created" in observer_names
    assert "on_order_shipped" in observer_names
    assert "on_user_created" in observer_names


def test_observer_with_condition_exports():
    """Test that observer condition exports correctly."""

    @observer(
        entity="Order",
        event="UPDATE",
        condition="status == 'paid' and total > 100",
        actions=[webhook("https://example.com/orders")],
    )
    def on_large_order_paid():
        pass

    schema = SchemaRegistry.get_schema()
    observer_def = schema["observers"][0]

    assert observer_def["condition"] == "status == 'paid' and total > 100"


def test_observer_with_custom_retry_exports():
    """Test that custom retry config exports correctly."""
    retry_config = RetryConfig(
        max_attempts=5, backoff_strategy="linear", initial_delay_ms=200, max_delay_ms=30000
    )

    @observer(
        entity="Order",
        event="INSERT",
        actions=[webhook("https://example.com/orders")],
        retry=retry_config,
    )
    def on_order_created():
        pass

    schema = SchemaRegistry.get_schema()
    observer_def = schema["observers"][0]

    assert observer_def["retry"]["max_attempts"] == 5
    assert observer_def["retry"]["backoff_strategy"] == "linear"
    assert observer_def["retry"]["initial_delay_ms"] == 200
    assert observer_def["retry"]["max_delay_ms"] == 30000


def test_observer_with_multiple_actions_exports():
    """Test that multiple actions export correctly."""

    @observer(
        entity="Order",
        event="INSERT",
        actions=[
            webhook("https://example.com/orders"),
            slack("#orders", "New order {id}"),
            email("admin@example.com", "Order created", "Order {id} created"),
        ],
    )
    def on_order_created():
        pass

    schema = SchemaRegistry.get_schema()
    observer_def = schema["observers"][0]

    assert len(observer_def["actions"]) == 3
    assert observer_def["actions"][0]["type"] == "webhook"
    assert observer_def["actions"][1]["type"] == "slack"
    assert observer_def["actions"][2]["type"] == "email"


def test_observer_event_type_normalized():
    """Test that event type is normalized to uppercase."""

    @observer(
        entity="Order", event="insert", actions=[webhook("https://example.com")]
    )
    def on_order():
        pass

    schema = SchemaRegistry.get_schema()
    observer_def = schema["observers"][0]

    assert observer_def["event"] == "INSERT"


def test_schema_without_observers():
    """Test that schema works without any observers."""
    schema = SchemaRegistry.get_schema()

    # Should not have "observers" key if no observers defined
    assert "observers" not in schema
